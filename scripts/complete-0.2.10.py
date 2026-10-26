import base64
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import urllib.request

SOURCE = "d0fb8578fe377b749319ea8d4702b933210dd867"
MAIN = "43878aac6bababcaae1b60d871198c21f6c40c90"
BRANCH = "fix/manual-shortcut-long-absence-20261026"
REPO = "TechWisard/desktop-pilot"
MANUAL_EN = "The manually protected quick shortcut stays in place until you change its rule or remove it. The original source or target is unavailable:"
MANUAL_RU = "Закреплённый вами быстрый ярлык остаётся на месте, пока вы не измените правило или не удалите его. Исходный ярлык или цель недоступны:"
AUTO_EN = "The existing quick copy is held for up to 72 hours; new copies are not created. Original source or target is temporarily unavailable:"
AUTO_RU = "Существующая быстрая копия сохраняется до 72 часов; новые копии не создаются. Исходный ярлык или цель временно недоступны:"
ALLOWED = {
    "Cargo.toml", "Cargo.lock", "src/engine.rs", "src/i18n.rs",
    "src/i18n/offline_tests.rs", "README.md", "README.ru.md",
    "docs/releases/0.2.10.md", "docs/research/2026-10-26-manual-absence.md",
    "docs/research/2026-10-26-manual-absence-completion.md",
    "docs/research/2026-10-26-manual-absence-validation.json",
    "docs/verification.md", "docs/verification.ru.md",
}

def read(path):
    return Path(path).read_text(encoding="utf-8")

def write(path, value):
    p = Path(path)
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(value, encoding="utf-8", newline="\n")

def replace(path, old, new):
    value = read(path)
    if value.count(old) != 1:
        raise RuntimeError(f"Expected exactly one replacement in {path}: {old[:90]}")
    write(path, value.replace(old, new, 1))

def prepare():
    if subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip() != SOURCE:
        raise RuntimeError("Unexpected source checkout")
    tests = r'''use super::translate;

fn verify_notice(en: &str, ru: &str) {
    for path in [
        r"C:\Tools\Undo\Rare tool.lnk",
        r"\\server\team\Избранное\Rare tool.url",
        r"C:\Tools\Original shortcut is unavailable\Undo.url",
    ] {
        let english = format!("{en} {path}");
        let russian = format!("{ru} {path}");
        assert_eq!(translate(&english, true), russian);
        assert_eq!(translate(&russian, false), english);
        assert_eq!(translate(&english, false), english);
        assert_eq!(translate(&russian, true), russian);
    }
}

'''
    for name, en, ru in [
        ("protected_offline_warning_translates_without_changing_paths", MANUAL_EN, MANUAL_RU),
        ("automatic_offline_warning_translates_without_changing_paths", AUTO_EN, AUTO_RU),
    ]:
        tests += "#[test]\nfn " + name + "() {\n    verify_notice(\n        " + json.dumps(en, ensure_ascii=False) + ",\n        " + json.dumps(ru, ensure_ascii=False) + ",\n    );\n}\n\n"
    write("src/i18n/offline_tests.rs", tests)
    write("src/i18n.rs", read("src/i18n.rs") + "\n#[cfg(test)]\nmod offline_tests;\n")
    baseline = subprocess.run(["cargo", "test", "--locked", "i18n::offline_tests"], text=True, encoding="utf-8", errors="replace", stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    write("target/completion-baseline.log", baseline.stdout)
    print(baseline.stdout[-14000:])
    if baseline.returncode == 0 or "test result: FAILED. 0 passed; 2 failed" not in baseline.stdout:
        raise RuntimeError("The two localization regressions did not fail in the expected way")
    replace("src/engine.rs", "Original source or target is unavailable: {}. Its manually protected quick shortcut will stay until you change its rule or remove it.", MANUAL_EN + " {}")
    replace("src/engine.rs", "Original source or target is temporarily unavailable: {}. The existing quick copy is held for up to 72 hours; new copies are not created.", AUTO_EN + " {}")
    entries = ""
    for ru, en in [(MANUAL_RU, MANUAL_EN), (AUTO_RU, AUTO_EN)]:
        entries += "\n        (\n            " + json.dumps(ru, ensure_ascii=False) + ",\n            " + json.dumps(en, ensure_ascii=False) + ",\n        ),"
    replace("src/i18n.rs", "    let translations = [", "    let translations = [" + entries)
    old = '''            let mut missing = store.app(&rare.id).unwrap();
            assert!(!missing.valid);
            missing.unavailable_since = now() - 6 * 86400;
            store.put_app(&missing).unwrap();

            assert_eq!(
                preview(store, false).unwrap().selected.as_slice(),
                std::slice::from_ref(&rare.id)
            );
            apply(store, false).unwrap();
            let warning: String = store.get("catalog_warning").unwrap();
            assert!(warning.contains("manually protected"));
            let retained: State = store.get("state").unwrap();
            assert_eq!(retained.owned[0].path, protected_path);
            assert_eq!(retained.owned[0].position, before.owned[0].position);
            assert!(retained.owned[0].manual);
            assert_eq!(std::fs::read(&protected_path).unwrap(), protected_bytes);'''
    new = '''            // Replay successive reviews across weekends and an extended absence.
            // Only the outage clock is advanced; selection, copying and SQLite
            // persistence are the actual application implementations.
            for days_away in [1, 2, 3, 6, 21, 45] {
                let mut missing = store.app(&rare.id).unwrap();
                assert!(!missing.valid);
                missing.unavailable_since = now() - days_away * 86400;
                store.put_app(&missing).unwrap();
                assert_eq!(
                    preview(store, false).unwrap().selected.as_slice(),
                    std::slice::from_ref(&rare.id)
                );
                apply(store, false).unwrap();
                let warning: String = store.get("catalog_warning").unwrap();
                assert!(warning.contains("manually protected"));
                let retained: State = store.get("state").unwrap();
                assert_eq!(retained.owned.len(), 1);
                assert_eq!(retained.owned[0].path, protected_path);
                assert_eq!(retained.owned[0].position, before.owned[0].position);
                assert_eq!(retained.owned[0].file_id, before.owned[0].file_id);
                assert!(retained.owned[0].manual);
                assert_eq!(std::fs::read(&protected_path).unwrap(), protected_bytes);
            }'''
    replace("src/engine.rs", old, new)
    replace("Cargo.toml", 'version = "0.2.9"', 'version = "0.2.10"')
    replace("Cargo.lock", 'name = "desktop-pilot"\nversion = "0.2.9"', 'name = "desktop-pilot"\nversion = "0.2.10"')
    for path in ("README.md", "README.ru.md"):
        replace(path, "DesktopPilot-0.2.9-windows-x64.zip", "DesktopPilot-0.2.10-windows-x64.zip")
    replace("README.md", "## Improvements in 0.2.9", """## Improvements in 0.2.10

- **Your manual choices outlast a disconnected source.** An already managed shortcut marked **Always** or moved by you stays in its familiar place even after the ordinary 72-hour grace period. Returning the unchanged original does not remove and recreate the quick copy.
- **Clear offline explanations in both languages.** The warning explains whether a copy is protected by your choice or only by the temporary grace period; Windows and network paths are preserved exactly.
- Automatic copies still expire normally; **Never add**, changed launch identity, user deletion/edit and explicitly returning to **Automatic** remain authoritative. No new shortcut is created from an unavailable source. [Scenarios and validation](docs/research/2026-10-26-manual-absence-completion.md).

## Improvements in 0.2.9""")
    replace("README.ru.md", "## Улучшения версии 0.2.9", """## Улучшения версии 0.2.10

- **Ваше закрепление не исчезает из-за недоступного источника.** Уже размещённый ярлык с правилом **«Всегда»** или выбранной вами позицией остаётся на месте и после обычных 72 часов. Возвращение неизменённого исходника не вызывает удаление и повторное создание копии.
- **Понятные объяснения на русском и английском.** Предупреждение различает ручное закрепление и временную защиту; пути к файлам и сетевым папкам сохраняются без изменений.
- Автоматические копии по-прежнему имеют ограниченный срок защиты. **«Не добавлять»**, изменение цели запуска, ручное удаление/редактирование и явный возврат к **«Автоматически»** сохраняют приоритет. Новые копии из недоступных источников не создаются. [Сценарии и проверки](docs/research/2026-10-26-manual-absence-completion.md).

## Улучшения версии 0.2.9""")
    replace("docs/research/2026-10-26-manual-absence.md", "Microsoft documents missing Windows desktop icons as a user-facing recovery problem with multiple possible causes: https://support.microsoft.com/en-us/onedrive/where-have-my-desktop-icons-gone.", "A real PowerToys user describes unpredictable restoration between office and home workspaces: https://github.com/microsoft/PowerToys/issues/38900 (2025-04-16). This is an analogous need for predictable recovery, not evidence of the same shortcut defect in Desktop Pilot.")
    write("docs/research/2026-10-26-manual-absence.md", read("docs/research/2026-10-26-manual-absence.md") + "\nCompletion, expanded 45-day sequence and verified results are recorded in [the continuation](2026-10-26-manual-absence-completion.md). The original pre-fix evidence above is retained.\n")
    write("docs/releases/0.2.10.md", """Desktop Pilot 0.2.10 сохраняет выбранные вами ярлыки при длительной недоступности источника.

## Что изменилось

- Уже размещённые ярлыки с правилом **«Всегда»** или ручной позицией больше не исчезают только потому, что исходник недоступен дольше 72 часов. После восстановления неизменённого исходника сохраняются прежняя копия и позиция.
- Предупреждения на русском и английском объясняют, что защищено вашим выбором, а что сохраняется временно. Пути к локальным и сетевым файлам не переводятся и не изменяются.
- Обычные автоматические копии сохраняют 72-часовой предел. Явное исключение, изменение цели запуска, удаление/редактирование копии и снятие закрепления остаются приоритетными. Недоступный источник не используется для создания новых копий.

Проверки используют настоящее ядро выбора, SQLite, файлы ярлыков и последовательность пересмотров с моделируемой недоступностью на 1, 2, 3, 6, 21 и 45 дней, восстановлением и явным снятием закрепления. Это не измерение экономии времени реальных людей и не физическое отключение диска.

Для установки скачайте **DesktopPilot-0.2.10-windows-x64.zip**, распакуйте и запустите `desktop-pilot.exe`. Контрольные суммы архивов — в **SHA256SUMS.txt**. Предварительный EXE не подписан. История, настройки, исходные файлы, локальная работа и безопасный откат сохраняются; скрытая телеметрия и новые сервисы не добавлены.

[Проверки и ограничения](https://github.com/TechWisard/desktop-pilot/blob/v0.2.10/docs/research/2026-10-26-manual-absence-completion.md)

---

## English

Desktop Pilot 0.2.10 preserves your deliberately protected shortcuts through extended source outages.

- An already managed **Always** or manually positioned shortcut no longer expires merely because its source stays unavailable beyond 72 hours. Restoring an unchanged source preserves its existing copy and position.
- English and Russian warnings distinguish manual protection from temporary grace. Local and network paths remain unchanged.
- Ordinary automatic copies retain the 72-hour limit. Explicit exclusion, changed launch identity, user deletion/edit and releasing a pin remain authoritative. Missing sources never create new shortcuts.

Regressions exercise the production selector, SQLite store, shortcut files and successive reviews at simulated outage ages of 1, 2, 3, 6, 21 and 45 days, then recovery and manual release. These are not physical drive-disconnection tests or measured human time savings.

Download **DesktopPilot-0.2.10-windows-x64.zip**, extract and launch `desktop-pilot.exe`. Check **SHA256SUMS.txt** for archive integrity. The preview executable is unsigned. History, settings, original files, offline operation and safe Undo are retained; no hidden telemetry or new service was added.

[Validation and limits](https://github.com/TechWisard/desktop-pilot/blob/v0.2.10/docs/research/2026-10-26-manual-absence-completion.md)
""")
    subprocess.run(["cargo", "fmt"], check=True)

def api(path, payload=None):
    req = urllib.request.Request("https://api.github.com/repos/" + REPO + "/" + path, headers={"Authorization": "Bearer " + os.environ["GH_TOKEN"], "Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28", "User-Agent": "DesktopPilot-release-preparation"}, data=None if payload is None else json.dumps(payload).encode("utf-8"), method="GET" if payload is None else "POST")
    with urllib.request.urlopen(req, timeout=30) as response:
        return json.load(response)

def publish():
    log = read("target/completion-tests.log")
    match = re.search(r"test result: ok\. (\d+) passed; 0 failed; 0 ignored", log)
    if not match or int(match[1]) != 174:
        raise RuntimeError("Unexpected regression test result")
    if "Native integration: 28 checks passed" not in read("target/completion-native.log"):
        raise RuntimeError("Native verification did not complete")
    if "Release publication: 9 checks passed" not in read("target/completion-guards.log"):
        raise RuntimeError("Release guards did not complete")
    run = "https://github.com/" + REPO + "/actions/runs/" + os.environ["GITHUB_RUN_ID"]
    result = {"source_commit": SOURCE, "prepared_version": "0.2.10", "preparation_run": run, "environment": "windows-2022 / Rust 1.93.1", "localization_before": {"passed": 0, "failed": 2}, "after": {"rust_tests_passed": int(match[1]), "failed": 0, "ignored": 0, "native_checks": 28, "release_guards": 9, "fmt": "passed", "clippy_warnings_denied": "passed", "debug_build": "passed"}, "ui_tests_passed": re.findall(r"test (ui::tests::\S+) \.\.\. ok", log), "simulated_outage_days": [1, 2, 3, 6, 21, 45], "physical_windows11_tested": False, "human_time_savings_measured": False, "release_status": "Candidate only; branch and main checks plus publication still required"}
    write("docs/research/2026-10-26-manual-absence-validation.json", json.dumps(result, indent=2, ensure_ascii=False) + "\n")
    write("docs/research/2026-10-26-manual-absence-completion.md", f"""# Manual absence: completion and handover (2026-10-26)

Continues [the original investigation](2026-10-26-manual-absence.md), not a new ranking policy. The published baseline was 0.2.9; PR #8 already held the manual-retention correction and two failure-first regressions. Existing profile recovery, bounded SQLite retries and user-control safeguards were retained.

## Additional confirmed defect and correction

The new protected-offline warning and the ordinary grace warning were English-only. Adding translation fragments alone would not fix the old sentence structure: the runtime translator deliberately protects everything following a Windows/UNC path until the next line. Put the complete explanation before the path and translate that prefix. No path parser or broad replacement of filenames is introduced. Two tests run the real translator both ways for local paths, UNC paths, Cyrillic folders and filenames containing words that are themselves status phrases.

## Reproduced sequences and measured scope

The production Store/refresh/preview/apply regression now checks six successive reviews at simulated source-outage ages **1, 2, 3, 6, 21 and 45 days**, followed by restoring the original and explicitly switching Always to Auto. Every outage review preserves one protected shortcut, its exact bytes, path, filesystem identity and position. Selection-level tests cover the ordinary automatic expiry, Never, legacy manual placement and changed launch identity. The original pre-fix six-day reproduction is preserved in the earlier document. Zero unsolicited replacements in the protected sequence is a synthetic result, not measured time saved by users.

## Executed checks

Preparation run: {run}. Immutable input: `{SOURCE}`. The two new localization tests first failed on the old translator (0 passed, 2 failed). After correction, **174 Rust tests passed, 0 failed, 0 ignored**; formatting, warnings-denied Clippy, debug build, **28 native integration checks** and **9 publication guards** passed on Windows 2022 with Rust 1.93.1. The UI layout/interaction tests included in the full suite ran; exact test names and outcomes are in [the machine-readable record](2026-10-26-manual-absence-validation.json).

The preparation workflow creates detached Git objects only. Its maintenance workflow/script are not part of the candidate source tree. Main history, branch protection and repository visibility are not changed by preparation. Branch advancement uses an expected-head check. The normal unchanged Windows 2022/2025 workflow must still pass for the candidate and main before its existing release job can publish. Final PR/run/tag/asset verification is recorded in PR #8; the preparation pass alone is not a release certificate.

## Evidence reused and extended

- Findlater and McGrenere, CHI 2004, compared static/adaptive/adaptable menus with 27 participants: https://doi.org/10.1145/985692.985704.
- Findlater et al., CHI 2009, report visual-search improvements while preserving spatial consistency: https://www.cs.ubc.ca/labs/imager/tr/2009/findlater_chi_ephemeral/.
- PowerToys user report, 2025-04-16, describes unpredictable layout restoration when switching between home and office: https://github.com/microsoft/PowerToys/issues/38900. This is analogous user evidence, not the same root cause or an incidence estimate for Desktop Pilot.

The choice to preserve manually protected copies through a longer outage is an engineering interpretation of user control and continuity, not a result established by those studies for this product.

## Remaining work

Physical Windows 11 Explorer, real unplug/reconnect, visual reading at 200% in an interactive session and human usability/timing were not tested. Complete loss of the whole custom profile drive, broader long-term project changes and adaptation delay remain separate investigations. No new services, telemetry, schema migration, permissions request or user-facing settings were added.
""")
    for path, heading, body in [
        ("docs/verification.md", "## 0.2.10: manual protection during long source outages", "A 45-day simulated review/recovery sequence preserves a manually protected shortcut; two additional regressions cover English/Russian offline warnings and unchanged paths."),
        ("docs/verification.ru.md", "## 0.2.10: ручное закрепление при длительной недоступности", "Последовательность пересмотров с моделируемой недоступностью до 45 дней и восстановлением сохраняет ручное закрепление; два дополнительных теста проверяют русские/английские предупреждения и неизменность путей."),
    ]:
        write(path, read(path) + f"\n{heading}\n\n{body}\n\n[Validation, run and limitations](research/2026-10-26-manual-absence-completion.md).\n")
    subprocess.run(["git", "diff", "--check"], check=True)
    changed = set(subprocess.check_output(["git", "ls-files", "--modified", "--others", "--exclude-standard"], text=True).splitlines())
    if changed != ALLOWED:
        raise RuntimeError("Unexpected changed files: " + repr(changed ^ ALLOWED))
    if api("git/ref/heads/" + BRANCH)["object"]["sha"] != SOURCE or api("git/ref/heads/main")["object"]["sha"] != MAIN:
        raise RuntimeError("A branch changed during preparation; nothing was advanced")
    entries = []
    for path in sorted(changed):
        data = Path(path).read_bytes()
        blob = api("git/blobs", {"content": base64.b64encode(data).decode("ascii"), "encoding": "base64"})
        expected = hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
        if blob["sha"] != expected:
            raise RuntimeError("Blob digest mismatch: " + path)
        entries.append({"path": path, "mode": "100644", "type": "blob", "sha": blob["sha"]})
    tree = api("git/trees", {"base_tree": api("git/commits/" + SOURCE)["tree"]["sha"], "tree": entries})
    commit = api("git/commits", {"message": "Release Desktop Pilot 0.2.10\n\nPreserve manually protected shortcuts through extended source outages, explain offline states in both languages, and verify real review/recovery sequences without altering user data.", "tree": tree["sha"], "parents": [SOURCE]})
    print("PREPARED_COMMIT_SHA=" + commit["sha"])
    print("PREPARED_TREE_SHA=" + tree["sha"])
    print("PREPARED_FILES=" + json.dumps(sorted(changed)))

if __name__ == "__main__":
    if len(sys.argv) != 2 or sys.argv[1] not in ("prepare", "publish"):
        raise SystemExit("Expected prepare or publish")
    (prepare if sys.argv[1] == "prepare" else publish)()
