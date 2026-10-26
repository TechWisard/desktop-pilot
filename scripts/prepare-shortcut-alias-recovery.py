import json
import os
import re
import subprocess
from pathlib import Path

BASE = '79b304a4bff84a8220486805e77da45e947d52e6'
BRANCH = 'fix/shortcut-alias-recovery-20261026'
ENGINE_BLOB = '97320834c5aeccbcf1b3d6b6c42a383af59fc294'
OUT = Path('target/alias-recovery')
OUT.mkdir(parents=True, exist_ok=True)
os.environ['CARGO_TERM_COLOR'] = 'never'

def command(args, logfile=None, expect_failure=False):
    result = subprocess.run(args, capture_output=True, text=True, encoding='utf-8', errors='replace')
    text = result.stdout + result.stderr
    if logfile:
        (OUT / logfile).write_text(text, encoding='utf-8')
    print(text, flush=True)
    if expect_failure:
        if result.returncode == 0:
            raise RuntimeError('The unchanged implementation unexpectedly passed the reproduction')
    elif result.returncode != 0:
        raise RuntimeError(f'Command failed: {args!r}')
    return text

def replace_once(text, old, new):
    if text.count(old) != 1:
        raise RuntimeError(f'Expected one exact patch anchor: {old!r}')
    return text.replace(old, new, 1)

def write(path, text):
    Path(path).write_bytes(text.encode('utf-8'))

if os.environ.get('GITHUB_REF') != 'refs/heads/' + BRANCH:
    raise RuntimeError('This preparation is restricted to its development branch')
if command(['git', 'rev-parse', 'HEAD:src/engine.rs']).strip() != ENGINE_BLOB:
    raise RuntimeError('Source changed; inspect and rebase the patch instead of overwriting it')
engine_path = Path('src/engine.rs')
source = engine_path.read_text(encoding='utf-8')
source += '\n#[cfg(test)]\nmod alias_recovery_tests;\n'
write(engine_path, source)
command(['cargo', 'fmt'])
before = command(['cargo', 'test', '--locked', 'alias_recovery_tests'], 'before.log', True)
if not re.search(r'test result: FAILED\. 2 passed; 3 failed;', before):
    raise RuntimeError('Baseline failed for an unexpected reason, not the three regression assertions')
source = engine_path.read_text(encoding='utf-8')
source = replace_once(source, '    let mut warnings = Vec::new();\n    let pins =', '    let mut warnings = Vec::new();\n    let mut unreadable_sources = HashSet::new();\n    let pins =')
source = replace_once(source, '            Err(error) => {\n                let missing = match std::fs::metadata(&app.source) {', '            Err(error) => {\n                unreadable_sources.insert(app.id.clone());\n                let missing = match std::fs::metadata(&app.source) {')
source = replace_once(source, '                if known[index].valid {\n                    continue;\n                }', '''                // Keep a stable verified source, but do not let its cached
                // validity hide a readable alias during a lock or partial write.
                // Full launch identity was matched above; an invalid alias must
                // never replace the last verified state of an unreadable source.
                if known[index].valid
                    && (!unreadable_sources.contains(&known[index].id) || !app.valid)
                {
                    continue;
                }''')
source = replace_once(source, '            if let Some(index) = old {\n                known[index] = app;', '''            if let Some(index) = old {
                if app.valid {
                    unreadable_sources.remove(&app.id);
                }
                known[index] = app;''')
write(engine_path, source)
manifest = Path('Cargo.toml')
write(manifest, replace_once(manifest.read_text(encoding='utf-8'), 'version = "0.2.12"', 'version = "0.2.13"'))
lock = Path('Cargo.lock')
write(lock, replace_once(lock.read_text(encoding='utf-8'), 'name = "desktop-pilot"\nversion = "0.2.12"', 'name = "desktop-pilot"\nversion = "0.2.13"'))
command(['cargo', 'fmt'])
command(['cargo', 'fmt', '--check'], 'format.log')
after = command(['cargo', 'test', '--locked'], 'after.log')
counts = re.search(r'test result: ok\. (\d+) passed; 0 failed;', after)
if not counts:
    raise RuntimeError('Cannot confirm the full test result')
command(['cargo', 'clippy', '--locked', '--all-targets', '--', '-D', 'warnings'], 'clippy.log')
command(['cargo', 'build', '--locked'], 'build.log')
native = command(['pwsh', '-NoProfile', '-File', 'scripts/test-native.ps1'], 'native.log')
guards = command(['pwsh', '-NoProfile', '-File', 'scripts/test-release-workflow.ps1'], 'release-guards.log')
if 'Native integration: 28 checks passed' not in native or 'Release publication: 9 checks passed' not in guards:
    raise RuntimeError('Native or publication check totals were not confirmed')
evidence = {
    'baseline_commit': BASE,
    'baseline_engine_blob': ENGINE_BLOB,
    'baseline_regressions': {'passed': 2, 'failed': 3},
    'after_full_tests_passed': int(counts.group(1)),
    'after_new_regressions_passed': 5,
    'native_checks_passed': 28,
    'publication_guards_passed': 9,
    'format_clippy_build': 'passed',
    'engine_blob_after': command(['git', 'hash-object', 'src/engine.rs']).strip(),
    'workflow_run': f"https://github.com/{os.environ['GITHUB_REPOSITORY']}/actions/runs/{os.environ['GITHUB_RUN_ID']}",
    'environment': 'GitHub-hosted windows-2022; Rust 1.93.1',
    'limits': ['No physical Windows 11 desktop or real cloud sync client was exercised', 'No measurement of human time savings', 'Final PR and main release validation are separate']
}
write('docs/research/2026-10-26-alias-recovery-validation.json', json.dumps(evidence, ensure_ascii=False, indent=2) + '\n')
research = Path('docs/research/2026-10-26-alias-recovery.md')
write(research, research.read_text(encoding='utf-8') + '\n## Executed preparation checks\n\n' + f"The unchanged baseline produced three intended assertion failures and two passing boundary checks. After the patch all {counts.group(1)} Rust tests passed, including the five new regressions, as did 28 native checks, nine release guards, formatting, strict Clippy and the debug build. Run: {evidence['workflow_run']}. Machine-readable details are in `2026-10-26-alias-recovery-validation.json`. Final PR/main CI and release publication must still be checked independently.\n")
# Preparation files are development-only, not part of the product or release.
Path('.github/workflows/prepare-shortcut-alias-recovery.yml').unlink()
Path('scripts/prepare-shortcut-alias-recovery.py').unlink()
command(['git', 'diff', '--check'])
remote = command(['git', 'ls-remote', 'origin', 'refs/heads/' + BRANCH]).split()[0]
if remote != os.environ['GITHUB_SHA']:
    raise RuntimeError('Development branch advanced concurrently; refusing to overwrite it')
command(['git', 'config', 'user.name', 'github-actions[bot]'])
command(['git', 'config', 'user.email', '41898282+github-actions[bot]@users.noreply.github.com'])
command(['git', 'add', '--all'])
command(['git', 'diff', '--cached', '--stat'])
command(['git', 'commit', '-m', 'fix: recover unreadable shortcut sources through verified launch aliases'])
command(['git', 'push', 'origin', 'HEAD:refs/heads/' + BRANCH])
print('VERIFIED_CANDIDATE=' + command(['git', 'rev-parse', 'HEAD']).strip(), flush=True)
