//! Read local shortcut metadata without resolving or probing its launch target.
//! An offline target must not hold organization hostage or start an application.
use crate::{model::App, native::wide};
use anyhow::{Context, Result, ensure};
use std::{io::Read, path::Path};
use windows::{
    Win32::{
        Globalization::{CP_ACP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS, MultiByteToWideChar},
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READ},
        UI::Shell::{IShellLinkW, SLGP_RAWPATH, ShellLink},
    },
    core::{Interface, PCWSTR},
};

fn decode(bytes: &[u8]) -> Result<String> {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        ensure!(bytes.len().is_multiple_of(2), "Incomplete UTF-16 shortcut");
        let little = bytes[0] == 0xff;
        let chars: Vec<_> = bytes[2..]
            .chunks_exact(2)
            .map(|pair| {
                if little {
                    u16::from_le_bytes([pair[0], pair[1]])
                } else {
                    u16::from_be_bytes([pair[0], pair[1]])
                }
            })
            .collect();
        return Ok(String::from_utf16(&chars)?);
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Ok(text.to_owned());
    }
    unsafe {
        let size = MultiByteToWideChar(CP_ACP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), bytes, None);
        ensure!(size > 0, "Cannot decode shortcut");
        let mut chars = vec![0u16; size as usize];
        ensure!(
            MultiByteToWideChar(
                CP_ACP,
                MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0),
                bytes,
                Some(&mut chars)
            ) == size,
            "Cannot decode shortcut"
        );
        Ok(String::from_utf16(&chars)?)
    }
}

fn text(chars: &[u16]) -> String {
    String::from_utf16_lossy(&chars[..chars.iter().position(|c| *c == 0).unwrap_or(chars.len())])
}

pub(super) fn read(path: &Path) -> Result<App> {
    let absolute = crate::profile::ordinary_path(std::fs::canonicalize(path)?);
    let mut app = App {
        source: absolute.to_string_lossy().into_owned(),
        name: path
            .file_stem()
            .context("Shortcut has no name")?
            .to_string_lossy()
            .into_owned(),
        ..Default::default()
    };
    if path
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("url"))
    {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 1024 * 1024, "Shortcut is too large");
        let text = decode(&bytes)?;
        ensure!(!text.contains('\0'), "Shortcut contains an embedded NUL");
        let mut section = false;
        let mut target_seen = false;
        for line in text.lines().map(str::trim) {
            if line.starts_with([';', '#']) || line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1]
                    .trim()
                    .eq_ignore_ascii_case("InternetShortcut");
            } else if section && let Some((key, value)) = line.split_once('=') {
                let value = value.trim();
                let value = value
                    .strip_prefix('"')
                    .and_then(|v| v.strip_suffix('"'))
                    .unwrap_or(value);
                if key.trim().eq_ignore_ascii_case("URL") && !target_seen {
                    target_seen = true;
                    app.target = value.to_owned();
                } else if key.trim().eq_ignore_ascii_case("IconFile") && app.icon.is_empty() {
                    app.icon = value.to_owned();
                } else if key.trim().eq_ignore_ascii_case("IconIndex") {
                    app.icon_index = value.parse().unwrap_or(0);
                }
            }
        }
    } else {
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            let file: IPersistFile = link.cast()?;
            file.Load(PCWSTR(wide(&app.source).as_ptr()), STGM_READ)?;
            let mut buffer = vec![0u16; 32768];
            link.GetPath(&mut buffer, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32)?;
            app.target = text(&buffer);
            buffer.fill(0);
            link.GetArguments(&mut buffer)?;
            app.arguments = text(&buffer);
            buffer.fill(0);
            link.GetWorkingDirectory(&mut buffer)?;
            app.cwd = text(&buffer);
            buffer.fill(0);
            link.GetIconLocation(&mut buffer, &mut app.icon_index)?;
            app.icon = text(&buffer);
        }
    }
    ensure!(
        !app.target.is_empty() && !app.target.contains('\0'),
        "No standalone launch target: left in place"
    );
    app.assign_id();
    Ok(app)
}
