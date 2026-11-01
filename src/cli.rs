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
            ensure!(!replace_out, "Option --replace-out was supplied more than once");
            replace_out = true;
            index += 1;
            continue;
        }
        if argument == "--home" || argument == "--out" {
            let value = input
                .get(index + 1)
                .with_context(|| format!("Missing value for {argument}"))?;
            ensure!(
                !value.trim().is_empty() && !value.starts_with("--"),
                "Missing value for {argument}"
            );
            let slot = if argument == "--home" {
                &mut home
            } else {
                &mut out
            };
            ensure!(
                slot.is_none(),
                "Option {argument} was supplied more than once"
            );
            *slot = Some(value.clone());
            index += 2;
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

Usage: desktop-pilot.exe [COMMAND] [--home DIRECTORY] [--out FILE] [--replace-out]

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
            vec!["--inspect", "--out", "report.json", "--replace-out", "--replace-out"],
            vec!["--help", "--replace-out"],
            vec!["--inspect", "--out", "notes.txt", "--replace-out"],
        ] {
            assert!(parse(&args(&input)).is_err(), "{input:?}");
        }
        let expected = args(&["--inspect", "--out", "report.json", "--replace-out"]);
        assert_eq!(parse(&expected).unwrap(), expected);
        assert_eq!(
            parse(&args(&["--replace-out", "--out", "report.json", "--inspect"]))
                .unwrap(),
            expected
        );
    }

    #[test]
    fn informational_commands_need_no_profile() {
        assert_eq!(parse(&args(&["-h"])).unwrap(), args(&["--help"]));
        assert_eq!(parse(&args(&["-V"])).unwrap(), args(&["--version"]));
        assert_eq!(parse(&[]).unwrap(), args(&["--settings"]));
    }
}
