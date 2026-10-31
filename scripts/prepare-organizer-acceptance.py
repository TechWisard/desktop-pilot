"""Build an instrumented acceptance copy; never patch the release checkout.

Only entry points and UI input/screenshot probes are added. The actual organizer,
SQLite store, Windows file operations, worker and rendered controls are unchanged.
Every replacement must match exactly once. The original checkout remains clean.
"""
from __future__ import annotations
import hashlib
import json
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "target" / "organizer-acceptance"
WORK = ROOT / "target" / "organizer-acceptance-source"


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise RuntimeError(f"Acceptance hook no longer matches exactly once: {path.name}: {old[:100]!r}")
    path.write_text(text.replace(old, new), encoding="utf-8", newline="\n")


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    if WORK.exists():
        shutil.rmtree(WORK)
    WORK.mkdir(parents=True)
    for directory in ("src", "assets"):
        shutil.copytree(ROOT / directory, WORK / directory)
    for name in ("Cargo.toml", "Cargo.lock", "build.rs"):
        shutil.copy2(ROOT / name, WORK / name)

    ui = WORK / "src/ui.rs"
    replace_once(ui,
        '        let view = app.store.get::<String>("test_view")?;\n',
        '        let view = app.store.get::<String>("test_view")?;\n        organization::configure_capture(&mut app, &view)?;\n')
    replace_once(ui,
        '    fn screenshots(&mut self, ctx: &egui::Context) {\n',
        '    fn screenshots(&mut self, ctx: &egui::Context) {\n        if organization::is_interactive_capture(self) { return; }\n')
    replace_once(ui,
        '        self.confirmations(ctx);\n    }\n}\n\npub fn run',
        '        self.confirmations(ctx);\n        organization::drive_capture(self, ctx);\n    }\n}\n\npub fn run')

    organization = WORK / "src/ui/organization.rs"
    replace_once(organization, '#[cfg(test)]\nmod tests;',
        'mod live_capture;\npub(super) use live_capture::{configure_capture, drive_capture, is_interactive_capture};\n\n#[cfg(test)]\nmod tests;')
    text = organization.read_text(encoding="utf-8")
    # Same production buttons and responses; record their visible coordinates.
    text = text.replace('if primary(', 'if live_capture::primary(')
    old = '''if ui
                                .add_enabled(
                                    available,
                                    egui::Button::new(t(
                                        "Отменить последнюю уборку",
                                        "Undo last organization",
                                    )),
                                )
                                .clicked()'''
    new = '''if live_capture::secondary(
                                ui, available,
                                t("Отменить последнюю уборку", "Undo last organization"),
                            ).clicked()'''
    if text.count(old) != 1:
        raise RuntimeError("Undo control changed; review acceptance input mapping")
    organization.write_text(text.replace(old, new), encoding="utf-8", newline="\n")
    shutil.copy2(ROOT / "test-support/organizer-ui-driver.rs",
                 WORK / "src/ui/organization/live_capture.rs")

    replace_once(WORK / "src/organize/mod.rs", 'mod fs;\n',
        'mod fs;\npub(crate) mod runtime_acceptance;\n')
    shutil.copy2(ROOT / "test-support/organizer-runtime.rs",
                 WORK / "src/organize/runtime_acceptance.rs")
    replace_once(WORK / "src/cli.rs", '| "--disable" | "--self-test" => false,',
        '| "--disable" | "--self-test" | "--organizer-acceptance" => false,')
    replace_once(WORK / "src/cli.rs", '                    | "--self-test"\n',
        '                    | "--self-test"\n                    | "--organizer-acceptance"\n')
    replace_once(WORK / "src/main.rs", '    if args[0] == "--self-test" {\n',
        '    if args[0] == "--organizer-acceptance" {\n        return emit(&args, &organize::runtime_acceptance::run(&h)?);\n    }\n    if args[0] == "--self-test" {\n')

    changed = []
    for path in (WORK / "src").rglob("*.rs"):
        relative = path.relative_to(WORK)
        original = ROOT / relative
        payload = path.read_bytes()
        if not original.exists() or original.read_bytes() != payload:
            changed.append({"path": relative.as_posix(), "sha256": hashlib.sha256(payload).hexdigest(),
                            "original_sha256": hashlib.sha256(original.read_bytes()).hexdigest() if original.exists() else None})
    (OUT / "harness-changes.json").write_text(json.dumps(changed, indent=2), encoding="utf-8")
    print(json.dumps(changed, indent=2))


if __name__ == "__main__":
    main()
