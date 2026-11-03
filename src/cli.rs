use anyhow::{Context, Result, bail, ensure};

/// Validate the complete invocation before opening a profile or changing files.
/// Options may precede or follow the command; downstream code receives one order.
pub fn parse(input: &[String]) -> Result<Vec<String>> {
    let mut command: Option<String> = None;
    let mut operand = None;
    let mut home = None;
    let mut out = None;
    let mut replace_out = false;
    let mut index = 0;
    while index < input.len() {
        let argument = &input[index];
        if argument == "--replace-out" {
            ensure!(
                !replace_out,
                "Option --replace-out was supplied more than once"
            );
            replace_out = true;
            index += 1;
            continue;
        }
        // Windows shortcut properties commonly use --home=PATH. Accept both
        // spellings without allowing a second --home or --out to override a
        // previously selected profile or report destination.
        let inline = argument
            .strip_prefix("--home=")
            .map(|value| ("--home", value))
            .or_else(|| argument.strip_prefix("--out=").map(|value| ("--out", value)));
        if argument == "--home" || argument == "--out" || inline.is_some() {
            let (option, value, consumed) = if let Some((option, value)) = inline {
                (option, value.to_owned(), 1)
            } else {
                (
                    argument.as_str(),
                    input
                        .get(index + 1)
                        .with_context(|| format!("Missing value for {argument}"))?
                        .clone(),
                    2,
                )
            };
            ensure!(
                !value.trim().is_empty() && !value.starts_with("--") && !value.contains('\0'),
                "Missing or invalid value for {option}"
            );
            let slot = if option == "--home" {
                &mut home
            } else {
                &mut out
            };
            ensure!(
                slot.is_none(),
                "Option {option} was supplied more than once"
            );
            *slot = Some(value);
            index += consumed;
            continue;
        }
        let name = match argument.as_str() {
            "-h" => "--help",
            "-V" => "--version",
            value => value,
        };
        let needs_operand = match name {
            "--init" | "--catalog" | "--launch" | "--validate-launch" | "--command" | "--probe"
            | "--ui-smoke" => true,
            "--help" | "--version" | "--settings" | "--inspect" | "--layout" | "--pins"
            | "--apply-initial" | "--apply" | "--rollback" | "--background" | "--stop"
            | "--disable" | "--self-test" => false,
            _ => bail!("Unknown argument: {argument}. Use --help to see available commands."),
        };
        ensure!(command.is_none(), "Only one command may be used at a time");
        command = Some(name.to_owned());
        if needs_operand {
            let value = input
                .get(index + 1)
                .with_context(|| format!("Missing value for {name}"))?;
            ensure!(
                !value.trim().is_empty() && !value.starts_with("--"),
                "Missing value for {name}"
            );
            operand = Some(value.clone());
            index += 1;
        }
        index += 1;
    }
    let command = command.unwrap_or_else(|| "--settings".into());
    if command == "--help" || command == "--version" {
        ensure!(
            home.is_none() && out.is_none(),
            "Help and version do not use profile or output options"
        );
    }
    // A JSON report must never replace unrelated user documents or archives
    // merely because their path was supplied to --out. The internal
    // last-error.txt diagnostic is not a CLI report and remains unchanged.
    if let Some(destination) = out.as_deref() {
        ensure!(
            std::path::Path::new(destination)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json")),
            "A report destination must end in .json; no files were changed"
        );
    }
    ensure!(
        !replace_out || out.is_some(),
        "--replace-out requires an explicit --out JSON destination"
    );
    if out.is_some() {
        ensure!(
            matches!(
                command.as_str(),
                "--init"
                    | "--inspect"
                    | "--layout"
                    | "--pins"
                    | "--catalog"
                    | "--apply-initial"
                    | "--apply"
                    | "--rollback"
                    | "--validate-launch"
                    | "--disable"
                    | "--self-test"
            ),
            "The {command} command does not produce a JSON report"
        );
    }
    let mut args = vec![command];
    args.extend(operand);
    if let Some(home) = home {
        args.extend(["--home".into(), home]);
    }
    if let Some(out) = out {
        args.extend(["--out".into(), out]);
    }
    if replace_out {
        args.push("--replace-out".into());
    }
    Ok(args)
}

pub const HELP: &str = "Desktop Pilot — adaptive desktop shortcuts for Windows

Usage: desktop-pilot.exe [COMMAND] [--home DIRECTORY] [--out FILE] [--replace-out]\n\n--home=DIR and --out=FILE are also accepted (including paths with spaces).

  --settings                 Open settings (default)
  --background               Start the background process
  --stop                     Stop the background process and wait for completion
  --inspect                  Export current settings, status and selection
  --apply                    Apply the current shortcut selection
  --apply-initial            Create the initial shortcut selection
  --rollback                 Undo the most recent placement
  --disable                  Stop work and disable collection, automation and startup
  --launch ID                Open a legacy managed shortcut
  --validate-launch ID       Inspect a shortcut without launching it
  --layout                   Export desktop geometry
  --pins                     Export taskbar shortcut identities
  --catalog FILE             Inspect shortcuts listed in a JSON file
  --init FILE                Import a catalog into a new profile
  --command ACTION           Queue an action for the background process
  --self-test                Run fixtures in a new, empty test-* directory
  --ui-smoke DIRECTORY       Save screenshots of an isolated test profile
  --help, -h                 Show this help
  --version, -V              Show the version

--home explicitly selects the profile. --out requires a .json file for report commands.
An existing JSON file is never replaced unless --replace-out is explicitly supplied.
Close the background process before direct apply or rollback commands.
--disable stops it automatically and stays disabled until explicitly resumed.
";

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).into()).collect()
    }

    #[test]
    fn global_options_work_before_the_command() {
        assert_eq!(
            parse(&args(&[
                "--home",
                "C:\\A folder",
                "--inspect",
                "--out",
                "report.json"
            ]))
            .unwrap(),
            args(&[
                "--inspect",
                "--home",
                "C:\\A folder",
                "--out",
                "report.json"
            ])
        );
        assert_eq!(
            parse(&args(&["--home", "profile"])).unwrap(),
            args(&["--settings", "--home", "profile"])
        );
    }

    #[test]
    fn incomplete_or_conflicting_invocations_are_rejected() {
        for input in [
            vec!["--home"],
            vec!["--home", "--stop"],
            vec!["--home", ""],
            vec!["--home", "one", "--home", "two"],
            vec!["--stop", "--apply"],
            vec!["--launch", "--home", "profile"],
            vec!["--unknown"],
            vec!["--settings", "ignored"],
            vec!["--settings", "--out", "file"],
            vec!["--inspect", "--out", "one", "--out", "two"],
        ] {
            assert!(parse(&args(&input)).is_err(), "{input:?}");
        }
    }

    #[test]
    fn json_report_option_rejects_document_and_backup_paths() {
        // All commands use this same parser before opening any profile.
        for destination in [
            "notes.txt",
            "photo.jpg",
            "report.pdf",
            "document.docx",
            "budget.xlsx",
            "archive.zip",
            "backup.json.bak",
            "report",
            "report.json.",
        ] {
            assert!(
                parse(&args(&["--inspect", "--out", destination])).is_err(),
                "Non-JSON output {destination} was accepted"
            );
        }
        for destination in ["report.json", "REPORT.JSON", r"C:\Reports\audit.json"] {
            assert!(
                parse(&args(&["--inspect", "--out", destination])).is_ok(),
                "Valid JSON output {destination} was rejected"
            );
        }
    }

    #[test]
    fn existing_json_overwrite_requires_explicit_opt_in() {
        for input in [
            vec!["--replace-out"],
            vec!["--inspect", "--replace-out"],
            vec![
                "--inspect",
                "--out",
                "report.json",
                "--replace-out",
                "--replace-out",
            ],
            vec!["--help", "--replace-out"],
            vec!["--inspect", "--out", "notes.txt", "--replace-out"],
        ] {
            assert!(parse(&args(&input)).is_err(), "{input:?}");
        }
        let expected = args(&["--inspect", "--out", "report.json", "--replace-out"]);
        assert_eq!(parse(&expected).unwrap(), expected);
        assert_eq!(
            parse(&args(&[
                "--replace-out",
                "--out",
                "report.json",
                "--inspect"
            ]))
            .unwrap(),
            expected
        );
    }

    #[test]
    fn inline_options_keep_profile_and_report_paths_without_allowing_duplicates() {
        assert_eq!(
            parse(&args(&[
                "--home=C:\\Projects\\My profile",
                "--inspect",
                "--out=C:\\Reports\\session report.JSON"
            ]))
            .unwrap(),
            args(&[
                "--inspect",
                "--home",
                "C:\\Projects\\My profile",
                "--out",
                "C:\\Reports\\session report.JSON"
            ])
        );
        assert_eq!(
            parse(&args(&["--out=report.json", "--home=profile", "--inspect"]))
                .unwrap(),
            args(&["--inspect", "--home", "profile", "--out", "report.json"])
        );
        for invocation in [
            vec!["--home=one", "--home", "two"],
            vec!["--home=one", "--home=two"],
            vec!["--out=one.json", "--out", "two.json"],
            vec!["--out=one.json", "--out=two.json"],
            vec!["--inspect", "--out="],
            vec!["--settings", "--home="],
            vec!["--inspect", "--out=backup.zip"],
            vec!["--inspect", "--out=report.json", "--replace-out", "--replace-out"],
            vec!["--home=--disable", "--settings"],
        ] {
            assert!(parse(&args(&invocation)).is_err(), "{invocation:?}");
        }
    }

    #[test]
    fn inline_home_reopens_the_same_sqlite_profile_instead_of_a_default_profile() {
        use crate::store::Store;
        use std::time::{SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "desktop-pilot-inline-home-{}-{unique}",
            std::process::id()
        ));
        let profile = root.join("Project Alpha");
        let store = Store::open(&profile).unwrap();
        store.set("inline_home_probe", &"existing user settings").unwrap();
        drop(store);

        let arguments = vec![
            format!("--home={}", profile.display()),
            "--inspect".into(),
        ];
        let normalized = parse(&arguments).unwrap();
        let selected = crate::profile::home(&normalized).unwrap();
        let expected = crate::profile::ordinary_path(std::fs::canonicalize(&profile).unwrap());
        assert_eq!(selected, expected);
        let reopened = Store::open(&selected).unwrap();
        assert_eq!(
            reopened.get::<String>("inline_home_probe").unwrap(),
            "existing user settings"
        );
        assert!(!root.join("DesktopPilot").exists());
        drop(reopened);
        for attempt in 0..5 {
            match std::fs::remove_dir_all(&root) {
                Ok(()) => break,
                Err(_) if attempt < 4 => std::thread::sleep(std::time::Duration::from_millis(50)),
                Err(error) => panic!("could not remove isolated test profile: {error}"),
            }
        }
    }

    #[test]
    fn informational_commands_need_no_profile() {
        assert_eq!(parse(&args(&["-h"])).unwrap(), args(&["--help"]));
        assert_eq!(parse(&args(&["-V"])).unwrap(), args(&["--version"]));
        assert_eq!(parse(&[]).unwrap(), args(&["--settings"]));
    }
}
