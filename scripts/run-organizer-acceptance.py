"""Exercise the actual Windows renderer and native desktop using synthetic files only."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import uuid


def invoke(exe: Path, args: list[str], log: Path, timeout: int = 120) -> None:
    result = subprocess.run([str(exe), *args], capture_output=True, timeout=timeout)
    log.write_bytes(result.stdout + b"\n--- stderr ---\n" + result.stderr)
    if result.returncode:
        raise RuntimeError(f"Executable failed ({result.returncode}); see {log}")


def signature(path: Path) -> tuple[int, str]:
    return path.stat().st_ino, hashlib.sha256(path.read_bytes()).hexdigest()


def ui_case(exe: Path, output: Path, language: str, scale: float) -> dict:
    output.mkdir(parents=True, exist_ok=True)
    home = Path(tempfile.mkdtemp(prefix="test-organizer-gui-", dir=os.environ["RUNNER_TEMP"]))
    desktop = home / "Desktop"
    desktop.mkdir()
    project = desktop / "Existing project"
    project.mkdir()
    (project / "keep.txt").write_text("Existing project contents must stay here", encoding="utf-8")
    original_paths = []
    for name, target in [
        ("Alpha", "https://example.invalid/Alpha"),
        ("Beta", "https://example.invalid/Beta"),
        ("Рабочий сайт", "https://example.invalid/Work"),
        ("Личный сайт", "https://example.invalid/Personal"),
        ("Game One", "steam://rungameid/100001"),
        ("Game Two", "steam://rungameid/100002"),
    ]:
        path = desktop / f"{name}.url"
        path.write_text(f"[InternetShortcut]\nURL={target}\n", encoding="utf-8")
        original_paths.append(path)
    (desktop / "Notes.txt").write_text("Not selected for organization", encoding="utf-8")
    (desktop / "Photo.jpg").write_bytes(b"Synthetic image object; not decoded")
    (desktop / "Photo.jpg.xmp").write_text("Synthetic sidecar", encoding="utf-8")
    originals = {str(path.relative_to(desktop)): signature(path) for path in desktop.rglob("*") if path.is_file()}
    listing = home / "paths.json"
    listing.write_text(json.dumps([str(path) for path in original_paths], ensure_ascii=False), encoding="utf-8")
    catalog = home / "catalog.json"
    invoke(exe, ["--catalog", str(listing), "--out", str(catalog)], output / "catalog.log")
    invoke(exe, ["--home", str(home), "--init", str(catalog), "--out", str(home / "init.json")], output / "init.log")
    with sqlite3.connect(home / "usage.sqlite") as db:
        config = json.loads(db.execute("SELECT v FROM kv WHERE k='config'").fetchone()[0])
        config.update(language=language, learning_enabled=False, respect_taskbar_pins=False)
        values = {"config": config, "test_desktop": str(desktop), "setup_complete": True,
                  "background_disabled": True, "ui_scale": scale,
                  "test_view": "organization-interactive", "test_viewport": [1120.0, 820.0],
                  "test_organization_files": False}
        db.executemany("INSERT INTO kv(k,v) VALUES(?,?) ON CONFLICT(k) DO UPDATE SET v=excluded.v",
                       [(key, json.dumps(value, ensure_ascii=False)) for key, value in values.items()])
    try:
        invoke(exe, ["--home", str(home), "--ui-smoke", str(output)], output / "renderer.log")
        proof = json.loads((output / "interaction.json").read_text(encoding="utf-8-sig"))
        if not proof["success"] or proof["native_mouse_clicks"] < 3:
            raise RuntimeError(f"Actual mouse interaction did not complete: {proof}")
        for stage in ("preview", "applied", "restored"):
            image = output / f"{stage}.png"
            if not image.is_file() or image.stat().st_size < 1000:
                raise RuntimeError(f"Missing actual renderer capture: {stage}")
        after = {str(path.relative_to(desktop)): signature(path) for path in desktop.rglob("*") if path.is_file()}
        if after != originals:
            raise RuntimeError("Apply/Undo changed fixture identities, bytes or paths")
        if sorted(path.name for path in desktop.iterdir() if path.is_dir()) != ["Existing project"]:
            raise RuntimeError("Undo left unexpected category directories")
        with sqlite3.connect(home / "usage.sqlite") as db:
            saved = json.loads(db.execute("SELECT v FROM kv WHERE k='config'").fetchone()[0])
            if saved != config:
                raise RuntimeError("UI organization changed saved user preferences")
            rows = [json.loads(row[0]) for row in db.execute("SELECT data FROM apps")]
            if sorted(Path(row["source"]).name for row in rows) != sorted(path.name for path in original_paths):
                raise RuntimeError("Catalog identities were not retained")
        proof["original_identities_and_bytes_verified"] = True
        proof["preferences_and_project_folder_verified"] = True
        (output / "verified.json").write_text(json.dumps(proof, ensure_ascii=False, indent=2), encoding="utf-8")
        shutil.rmtree(home)
        return proof
    except Exception:
        error = home / "last-error.txt"
        if error.exists():
            shutil.copy2(error, output / error.name)
        raise


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--desktop", action="store_true")
    args = parser.parse_args()
    exe = args.executable.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if os.environ.get("GITHUB_ACTIONS") != "true" or os.name != "nt":
        raise RuntimeError("Only an explicitly disposable Windows runner is supported")
    results, errors = [], []
    for language in ("ru", "en"):
        for scale in (1.0, 2.0):
            label = f"{language}-{int(scale * 100)}"
            try:
                results.append(ui_case(exe, output / label, language, scale))
                print(f"PASS actual GUI: {label}", flush=True)
            except Exception as error:
                errors.append({"case": label, "error": str(error)})
                print(f"FAIL actual GUI: {label}: {error}", flush=True)
    if args.desktop:
        home = Path(os.environ["RUNNER_TEMP"]) / ("test-organizer-live-" + uuid.uuid4().hex)
        try:
            invoke(exe, ["--home", str(home), "--organizer-acceptance", "--out", str(output / "desktop.json")],
                   output / "desktop.log", timeout=180)
            report = json.loads((output / "desktop.json").read_text(encoding="utf-8-sig"))
            if not report.get("success"):
                raise RuntimeError("Live desktop acceptance did not confirm completion")
            results.append(report)
            print("PASS actual Explorer and background-process lifecycle", flush=True)
            shutil.rmtree(home)
        except Exception as error:
            errors.append({"case": "actual-desktop", "error": str(error)})
            print(f"FAIL actual desktop: {error}", flush=True)
    (output / "results.json").write_text(json.dumps({"passed": results, "failed": errors}, ensure_ascii=False, indent=2), encoding="utf-8")
    if errors:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
