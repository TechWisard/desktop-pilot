//! Actual Windows Cloud Files placeholders; never a user cloud account.
use crate::organize::*;
use std::{
    ffi::c_void,
    fs::{self as disk, OpenOptions},
    os::windows::{ffi::OsStrExt, fs::MetadataExt, fs::OpenOptionsExt},
};
use windows::{
    Win32::{Storage::FileSystem::FILE_BASIC_INFO, UI::Shell::FOLDERID_Desktop},
    core::{GUID, HRESULT},
};

// cfapi.h uses USHORT wrappers for the two policy pairs. These declarations
// are test-only and follow the Windows SDK ABI; no cloud API enters the app.
#[repr(C)]
struct Registration {
    size: u32,
    name: *const u16,
    version: *const u16,
    root_identity: *const c_void,
    root_length: u32,
    file_identity: *const c_void,
    file_length: u32,
    provider: GUID,
}
#[repr(C)]
struct Policies {
    size: u32,
    hydration: [u16; 2],
    population: [u16; 2],
    in_sync: u32,
    hard_links: u32,
    management: u32,
}
#[repr(C)]
struct Metadata {
    basic: FILE_BASIC_INFO,
    size: i64,
}
#[repr(C)]
struct Placeholder {
    name: *const u16,
    metadata: Metadata,
    identity: *const c_void,
    identity_length: u32,
    flags: u32,
    result: HRESULT,
    usn: i64,
}
#[link(name = "CldApi")]
unsafe extern "system" {
    fn CfRegisterSyncRoot(
        path: *const u16,
        registration: *const Registration,
        policies: *const Policies,
        flags: u32,
    ) -> HRESULT;
    fn CfUnregisterSyncRoot(path: *const u16) -> HRESULT;
    fn CfCreatePlaceholders(
        path: *const u16,
        items: *mut Placeholder,
        count: u32,
        flags: u32,
        processed: *mut u32,
    ) -> HRESULT;
}
#[link(name = "Shell32")]
unsafe extern "system" {
    fn SHSetKnownFolderPath(
        folder: *const GUID,
        flags: u32,
        token: *mut c_void,
        path: *const u16,
    ) -> HRESULT;
}
fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
struct Cleanup {
    home: PathBuf,
    registered: Option<Vec<u16>>,
    old_desktop: Option<Vec<u16>>,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        unsafe {
            if let Some(path) = &self.old_desktop {
                let _ =
                    SHSetKnownFolderPath(&FOLDERID_Desktop, 0, std::ptr::null_mut(), path.as_ptr());
            }
            if let Some(path) = &self.registered {
                let _ = CfUnregisterSyncRoot(path.as_ptr());
            }
        }
        let _ = disk::remove_dir_all(&self.home);
    }
}
fn placeholder_identity(path: &Path) -> Result<String> {
    // Metadata-only handle; never request cloud hydration to inspect identity.
    let handle = OpenOptions::new()
        .access_mode(0x80)
        .share_mode(7)
        .custom_flags(0x0020_0000)
        .open(path)?;
    fs::identity(&handle)
}

#[test]
#[ignore = "requires the explicit disposable Windows Cloud Files acceptance job"]
fn real_cloud_placeholder_and_redirected_desktop_preserve_local_choices() -> Result<()> {
    ensure!(
        std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true")
            && std::env::var("RUNNER_ENVIRONMENT").as_deref() == Ok("github-hosted")
            && std::env::var("DESKTOP_PILOT_DISPOSABLE_DESKTOP").as_deref() == Ok("1"),
        "Cloud acceptance requires a disposable hosted desktop"
    );
    let _com = native::Com::new()?;
    let home = std::env::temp_dir().join(format!("test-cloud-organizer-{}", fs::operation_id()?));
    let desktop = home.join("SyncRoot").join("Desktop");
    disk::create_dir_all(&desktop)?;
    let mut cleanup = Cleanup {
        home: home.clone(),
        registered: None,
        old_desktop: None,
    };
    let store = Store::open(&home)?;
    let old = native::desktop()?;
    cleanup.old_desktop = Some(wide(&old));
    unsafe {
        SHSetKnownFolderPath(
            &FOLDERID_Desktop,
            0,
            std::ptr::null_mut(),
            wide(&desktop).as_ptr(),
        )
        .ok()?;
    }
    // Resolve the actual per-user Known Folder, without a test path override.
    ensure!(
        norm_literal_path(&root(&store)?.to_string_lossy())
            == norm_literal_path(&desktop.to_string_lossy()),
        "The organizer did not resolve the redirected Known Folder"
    );
    unsafe {
        SHSetKnownFolderPath(
            &FOLDERID_Desktop,
            0,
            std::ptr::null_mut(),
            wide(&old).as_ptr(),
        )
        .ok()?;
    }
    cleanup.old_desktop = None;
    ensure!(native::desktop()? == old, "Known Folder restoration failed");

    // The filesystem/cloud scenario is isolated from Explorer icon geometry;
    // the separate live Explorer test covers the real view and positions.
    store.set("test_desktop", &desktop.to_string_lossy())?;
    store.set("setup_complete", &true)?;
    store.set("background_disabled", &true)?;
    let config = Config {
        collect: false,
        autostart: false,
        learning_enabled: false,
        respect_taskbar_pins: false,
        ..Default::default()
    };
    store.set("config", &config)?;
    let sync_root = wide(desktop.parent().unwrap());
    let provider = wide(Path::new("Isolated organizer acceptance"));
    let version = wide(Path::new("1"));
    let registration = Registration {
        size: std::mem::size_of::<Registration>() as u32,
        name: provider.as_ptr(),
        version: version.as_ptr(),
        root_identity: std::ptr::null(),
        root_length: 0,
        file_identity: std::ptr::null(),
        file_length: 0,
        provider: GUID::from_u128(0x7cb3a7f1_fef4_495a_92dd_63b9df91fd62),
    };
    let policies = Policies {
        size: std::mem::size_of::<Policies>() as u32,
        hydration: [2, 0],  // FULL permits a dehydrated file.
        population: [3, 0], // Namespace is already complete; no provider callback.
        in_sync: 0,
        hard_links: 0,
        management: 0,
    };
    unsafe { CfRegisterSyncRoot(sync_root.as_ptr(), &registration, &policies, 2).ok()? };
    cleanup.registered = Some(sync_root);
    let remote_name = wide(Path::new("Remote.url"));
    let identity = b"isolated-remote-shortcut";
    let mut remote = Placeholder {
        name: remote_name.as_ptr(),
        metadata: Metadata {
            basic: FILE_BASIC_INFO {
                FileAttributes: 0x80,
                ..Default::default()
            },
            size: 128,
        },
        identity: identity.as_ptr().cast(),
        identity_length: identity.len() as u32,
        flags: 2, // MARK_IN_SYNC, but deliberately not hydrated.
        result: HRESULT(0),
        usn: 0,
    };
    let mut processed = 0;
    unsafe {
        CfCreatePlaceholders(wide(&desktop).as_ptr(), &mut remote, 1, 1, &mut processed).ok()?;
    }
    remote.result.ok()?;
    ensure!(processed == 1, "No real cloud placeholder was created");
    let remote_path = desktop.join("Remote.url");
    let remote_id = placeholder_identity(&remote_path)?;
    let attributes = remote_path.symlink_metadata()?.file_attributes();
    ensure!(
        attributes & 0x400 != 0,
        "Expected a Cloud Files reparse object"
    );
    ensure!(fs::inspect(&remote_path, true).is_err());
    let mut originals = Vec::new();
    for name in ["Alpha", "Beta", "RareImportant"] {
        let path = desktop.join(format!("{name}.url"));
        disk::write(
            &path,
            format!("[InternetShortcut]\r\nURL=https://example.invalid/{name}\r\n"),
        )?;
        let app = native::read_link(&path)?;
        store.put_app(&app)?;
        if name == "RareImportant" {
            store.policy(&app.id, Policy::Always)?;
        }
        originals.push((path.clone(), fs::inspect(&path, true)?));
    }
    let plan = analyze(&store, Options::default(), &config, &AtomicBool::new(false))?;
    ensure!(plan.summary() == (2, 1, 1), "Unexpected local proposals");
    ensure!(plan.skipped.iter().any(|item| item.name == "Remote.url"));
    ensure!(apply(&store, &plan, &AtomicBool::new(false))?.moved == 2);
    let operation = status(&store)?.undo.context("Missing local Undo")?;
    drop(store);
    let store = Store::open(&home)?;
    ensure!(undo(&store, &operation)?.restored == 2);
    for (path, expected) in originals {
        ensure!(
            fs::inspect(&path, true)? == expected,
            "Local original changed"
        );
    }
    ensure!(placeholder_identity(&remote_path)? == remote_id);
    ensure!(remote_path.symlink_metadata()?.file_attributes() == attributes);
    ensure!(store.get::<bool>("background_disabled")?);
    ensure!(!status(&store)?.pending);
    // No sync provider was connected and no data was ever supplied, so a
    // hydration-dependent code path could not have completed successfully.
    disk::remove_file(&remote_path)?;
    unsafe { CfUnregisterSyncRoot(cleanup.registered.as_ref().unwrap().as_ptr()).ok()? };
    cleanup.registered = None;
    drop(store);
    disk::remove_dir_all(&home)?;
    println!(
        "CLOUD_ACCEPTANCE: real dehydrated placeholder untouched; two local moves/Undo; Known Folder restored; no account or network"
    );
    Ok(())
}
