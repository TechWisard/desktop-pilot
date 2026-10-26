"""Failure-first reproduction and bounded candidate preparation on one branch."""
import json
import os
from pathlib import Path
import re
import subprocess

BASE = 'feb1041d837adc3eeb823515f92f6b0bb01c0e93'
BRANCH = 'fix/import-ownership-20261027'
assert os.environ['GITHUB_REF'] == 'refs/heads/' + BRANCH
START = os.environ['GITHUB_SHA']
LOG = Path('target/import-ownership')
LOG.mkdir(parents=True, exist_ok=True)


def run(args, name=None, check=True):
    result = subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, encoding='utf-8', errors='replace', env={**os.environ, 'CARGO_TERM_COLOR': 'never'})
    print(result.stdout, flush=True)
    if name:
        (LOG / (name + '.txt')).write_text(result.stdout, encoding='utf-8')
    if check and result.returncode:
        raise RuntimeError('Command failed: ' + ' '.join(args))
    return result


def replace_once(path, old, new):
    path = Path(path)
    text = path.read_text(encoding='utf-8')
    assert text.count(old) == 1, 'Unexpected source context: ' + str(path)
    path.write_text(text.replace(old, new), encoding='utf-8', newline='\n')


expected_blobs = {
    'src/ui.rs': '8d304c2988c35f5a68a7f6697dc6b71cac9786aa',
    'src/engine.rs': '5eddd7d1aa3179c7aab6d616ded39277c6f16410',
    'src/i18n.rs': '94a58a83a0ed31e05944e9188b00b3bce0571d38',
}
for path, expected in expected_blobs.items():
    assert run(['git', 'rev-parse', 'HEAD:' + path]).stdout.strip() == expected
assert run(['git', 'rev-parse', 'HEAD^']).stdout.strip() == BASE
ui = Path('src/ui.rs')
ui.write_text(ui.read_text(encoding='utf-8') + '\n#[cfg(test)]\nmod import_ownership_tests;\n', encoding='utf-8', newline='\n')
run(['cargo', 'fmt'])
before = run(['cargo', 'test', '--locked', 'ui::import_ownership_tests', '--', '--test-threads=1'], 'before', check=False)
expected_failures = [
    'reimporting_a_quick_copy_preserves_the_original_rule_history_and_undo',
    'reimporting_a_quick_copy_does_not_block_undo',
    'renamed_quick_copy_is_recognized_before_reconciliation',
    'quick_copy_does_not_mask_a_missing_source_or_reverse_never',
    'changed_managed_copy_is_rejected_without_blocking_other_imports',
    'unfinished_changes_block_import_without_mutating_the_catalog',
    'mixed_original_and_quick_copy_imports_are_order_independent',
]
assert before.returncode != 0 and 'test result: FAILED.' in before.stdout
assert 'error[E' not in before.stdout, 'A compilation failure is not behavioral evidence'
for name in expected_failures:
    assert 'test ui::import_ownership_tests::' + name + ' ... FAILED' in before.stdout, name
assert re.search(r'test result: FAILED\. 2 passed; 7 failed;', before.stdout)

replace_once('src/engine.rs', 'fn ensure_no_unfinished(store: &Store) -> Result<()> {', 'pub(crate) fn ensure_no_unfinished(store: &Store) -> Result<()> {')
engine = Path('src/engine.rs')
engine.write_text(engine.read_text(encoding='utf-8') + '\nmod import_guard;\npub(crate) use import_guard::imported_quick_copy;\n', encoding='utf-8', newline='\n')
replace_once('src/ui.rs', '''        self.start_job("Adding shortcuts…", move || {
            let _com = native::Com::new()?;
            let mut apps = Vec::new();''', '''        self.start_job("Adding shortcuts…", move || {
            let _com = native::Com::new()?;
            let store = Store::open(&home)?;
            // Import changes source ownership too. Serialize it with placement
            // and Undo, and never alter the source set of an unfinished change.
            let _changes = native::exclusive("changes", &store.home)?;
            engine::ensure_no_unfinished(&store)?;
            let mut apps = Vec::new();''')
replace_once('src/ui.rs', '''            let store = Store::open(&home)?;
            store.transaction(|| {
                let mut existing: HashMap<_, _> = store''', '''            store.transaction(|| {
                let mut existing: HashMap<_, _> = store''')
replace_once('src/ui.rs', '''                for mut app in apps {
                    if let Err(error) = prepare_imported_app(&mut app, &existing) {''', '''                for mut app in apps {
                    match engine::imported_quick_copy(&store, &app) {
                        Ok(Some(original)) => {
                            result.added.push((original.id, original.name));
                            continue;
                        }
                        Ok(None) => {}
                        Err(error) => {
                            result.failed.push((PathBuf::from(&app.source), format!("{error:#}")));
                            continue;
                        }
                    }
                    if let Err(error) = prepare_imported_app(&mut app, &existing) {''')
replace_once('src/i18n.rs', '    let translations = [\n', '''    let translations = [
        (
            "Этот быстрый ярлык был изменён. Добавьте его исходный ярлык вместо быстрой копии.",
            "This quick shortcut was changed. Add its original shortcut instead.",
        ),
''')
i18n = Path('src/i18n.rs')
i18n.write_text(i18n.read_text(encoding='utf-8') + '\n#[cfg(test)]\nmod import_ownership_tests;\n', encoding='utf-8', newline='\n')
replace_once('Cargo.toml', 'version = "0.2.16"', 'version = "0.2.17"')
replace_once('Cargo.lock', 'name = "desktop-pilot"\nversion = "0.2.16"', 'name = "desktop-pilot"\nversion = "0.2.17"')
run(['cargo', 'fmt'])
run(['cargo', 'fmt', '--check'], 'format')
after = run(['cargo', 'test', '--locked'], 'after')
assert re.search(r'test result: ok\. (\d+) passed; 0 failed; 0 ignored;', after.stdout)
run(['cargo', 'clippy', '--locked', '--all-targets', '--', '-D', 'warnings'], 'clippy')
run(['cargo', 'build', '--locked'], 'build')
native = run(['pwsh', '-NoProfile', '-File', 'scripts/test-native.ps1'], 'native')
assert 'Native integration: 28 checks passed' in native.stdout
guards = run(['pwsh', '-NoProfile', '-File', 'scripts/test-release-workflow.ps1'], 'publication')
assert 'Release publication: 9 checks passed' in guards.stdout
validation = {
    'baseline_commit': BASE,
    'baseline_source_blobs': expected_blobs,
    'baseline_ui_regressions': {'passed': 2, 'failed': 7, 'failure_names': expected_failures},
    'after_full_tests_passed': int(re.search(r'test result: ok\. (\d+) passed;', after.stdout)[1]),
    'after_new_ui_regressions_passed': 9,
    'after_new_translation_regressions_passed': 1,
    'native_checks_passed': 28,
    'publication_guards_passed': 9,
    'format_clippy_build': 'passed',
    'workflow_run': 'https://github.com/TechWisard/desktop-pilot/actions/runs/' + os.environ['GITHUB_RUN_ID'],
    'environment': 'GitHub-hosted windows-2022; Rust 1.93.1',
    'limits': ['No physical Windows 11 desktop', 'No human task-time measurement', 'PR/main/release validation is separate'],
}
Path('docs/research/2026-10-27-import-ownership-validation.json').write_text(json.dumps(validation, ensure_ascii=False, indent=2) + '\n', encoding='utf-8', newline='\n')
print(json.dumps(validation, ensure_ascii=False, indent=2), flush=True)
# Preparation machinery is not shipped. Keep only the narrow implementation,
# reproducible regression tests, release notes and actual verification evidence.
Path('.github/workflows/prepare-import-ownership.yml').unlink()
Path('scripts/prepare-import-ownership.py').unlink()
run(['git', 'diff', '--check'])
remote = run(['git', 'ls-remote', 'origin', 'refs/heads/' + BRANCH]).stdout.split()[0]
assert remote == START, 'Branch changed concurrently; do not overwrite it'
run(['git', 'config', 'user.name', 'TechWisard'])
run(['git', 'config', 'user.email', '152439417+TechWisard@users.noreply.github.com'])
run(['git', 'add', 'src', 'Cargo.toml', 'Cargo.lock', 'docs', 'scripts/prepare-import-ownership.py', '.github/workflows/prepare-import-ownership.yml'])
run(['git', 'diff', '--cached', '--stat'])
run(['git', 'commit', '-m', 'fix: preserve original ownership and undo when quick copies are reimported'])
run(['git', 'push', 'origin', 'HEAD:refs/heads/' + BRANCH])
print('VERIFIED_CANDIDATE=' + run(['git', 'rev-parse', 'HEAD']).stdout.strip(), flush=True)
