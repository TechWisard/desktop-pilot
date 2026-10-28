"""Bounded failure-first Windows verification for transaction-local import indexing."""
import json
import os
from pathlib import Path
import re
import subprocess

BASE = '6c27fe636ad4b727a935763ae9ab04463d5662b5'
BRANCH = 'perf/bulk-import-identity-index-20261028'
assert os.environ['GITHUB_REF'] == 'refs/heads/' + BRANCH
START = os.environ['GITHUB_SHA']
LOG = Path('target/import-scaling')
LOG.mkdir(parents=True, exist_ok=True)


def run(args, name=None, check=True):
    result = subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, encoding='utf-8', errors='replace', timeout=600, env={**os.environ, 'CARGO_TERM_COLOR': 'never'})
    print(result.stdout, flush=True)
    if name:
        (LOG / (name + '.txt')).write_text(result.stdout, encoding='utf-8')
    if check and result.returncode:
        raise RuntimeError('Command failed: ' + ' '.join(args))
    return result


def replace_once(text, old, new):
    assert text.count(old) == 1, 'Unexpected source context: ' + old[:100]
    return text.replace(old, new)


def write(path, text):
    Path(path).write_text(text, encoding='utf-8', newline='\n')


run(['git', 'merge-base', '--is-ancestor', BASE, 'HEAD'])
for path, sha in {
    'src/ui.rs': '2d9813022d0de13316c48bfae0e3c36ab6fb45f3',
    'src/ui/import_ownership_tests.rs': '262baae7f9e9523c60709212b7a931cb81470428',
}.items():
    assert run(['git', 'rev-parse', 'HEAD:' + path]).stdout.strip() == sha
ui = Path('src/ui.rs').read_text(encoding='utf-8')
start = ui.index('    fn add_shortcuts(')
end = ui.index('    fn refresh_layout(', start)
body = ui[start:end]
body = replace_once(body, '            let _com = native::Com::new()?;', '''            let _com = native::Com::new()?;
            #[cfg(test)]
            let import_started = Instant::now();
            #[cfg(test)]
            IDENTITY_EVALUATIONS.with(|calls| calls.set(0));''')
body = replace_once(body, '            store.transaction(|| {', '''            #[cfg(test)]
            let transaction_started = Instant::now();
            store.transaction(|| {''')
body = replace_once(body, '            Ok(JobResult::Added(result))', '''            #[cfg(test)]
            import_scaling_tests::record(
                &home,
                IDENTITY_EVALUATIONS.with(|calls| calls.get()),
                import_started.elapsed(),
                transaction_started.elapsed(),
            );
            Ok(JobResult::Added(result))''')
ui = ui[:start] + body + ui[end:] + '\n#[cfg(test)]\nmod import_scaling_tests;\n'
write('src/ui.rs', ui)
fixture = Path('src/ui/import_ownership_tests.rs').read_text(encoding='utf-8')
write('src/ui/import_ownership_tests.rs', replace_once(fixture, 'fn with_profile(', 'pub(super) fn with_profile('))
run(['cargo', 'fmt'])
command = ['cargo', 'test', '--locked', 'ui::import_scaling_tests', '--', '--test-threads=1', '--nocapture']
before = run(command, 'before', check=False)
assert before.returncode != 0 and 'test result: FAILED.' in before.stdout
assert 'error[E' not in before.stdout, 'Compilation failure is not behavioral evidence'
assert 'bulk import must not compare every new shortcut' in before.stdout
assert re.search(r'test result: FAILED\. 3 passed; 1 failed;', before.stdout)

ui = Path('src/ui.rs').read_text(encoding='utf-8')
start = ui.index('fn prepare_imported_app(')
end = ui.index('fn should_resume_background(', start)
ui = ui[:start] + '''#[cfg(test)]
fn prepare_imported_app(app: &mut App, existing: &HashMap<String, App>) -> Result<()> {
    ImportCatalog::new(existing.values().cloned()).prepare(app)
}

''' + ui[end:]
ui = replace_once(ui, '''                let mut existing: HashMap<_, _> = store
                    .apps()?
                    .into_iter()
                    .map(|app| (app.id.clone(), app))
                    .collect();''', '''                let mut existing = ImportCatalog::new(store.apps()?);''')
ui = replace_once(ui, 'if let Err(error) = prepare_imported_app(&mut app, &existing) {', 'if let Err(error) = existing.prepare(&mut app) {')
ui += '\nmod import_catalog;\nuse import_catalog::ImportCatalog;\n'
write('src/ui.rs', ui)
for path, old, new in [
    ('Cargo.toml', 'version = "0.2.37"', 'version = "0.2.38"'),
    ('Cargo.lock', 'name = "desktop-pilot"\nversion = "0.2.37"', 'name = "desktop-pilot"\nversion = "0.2.38"'),
]:
    write(path, replace_once(Path(path).read_text(encoding='utf-8'), old, new))
run(['cargo', 'fmt'])
after = run(command, 'after-targeted')
run(['cargo', 'fmt', '--check'], 'format')
full = run(['cargo', 'test', '--locked'], 'full-tests')
run(['cargo', 'clippy', '--locked', '--all-targets', '--', '-D', 'warnings'], 'clippy')
run(['cargo', 'build', '--locked'], 'build')
native = run(['pwsh', '-NoProfile', '-File', 'scripts/test-native.ps1'], 'native')
assert 'Native integration: 28 checks passed' in native.stdout
guards = run(['pwsh', '-NoProfile', '-File', 'scripts/test-release-workflow.ps1'], 'publication')
assert 'Release publication: 9 checks passed' in guards.stdout
run(['python', 'scripts/test-readmes.py'], 'readme')


def samples(output):
    return [json.loads(line.split('IMPORT_SCALING_METRIC ', 1)[1]) for line in output.splitlines() if 'IMPORT_SCALING_METRIC ' in line]


baseline = samples(before.stdout)
corrected = samples(after.stdout)
assert [r['entries'] for r in baseline] == [100, 1000, 5000]
assert [r['entries'] for r in corrected] == [100, 1000, 5000]
validation = {
    'baseline_commit': BASE,
    'baseline_production_ui_blob': '2d9813022d0de13316c48bfae0e3c36ab6fb45f3',
    'before': baseline, 'after': corrected,
    'baseline_tests': {'passed': 3, 'failed': 1},
    'after_targeted_tests_passed': 4,
    'after_full_tests_passed': int(re.search(r'test result: ok\. (\d+) passed; 0 failed;', full.stdout)[1]),
    'native_checks_passed': 28, 'publication_guards_passed': 9,
    'format_clippy_build_readme': 'passed',
    'workflow_run': 'https://github.com/TechWisard/desktop-pilot/actions/runs/' + os.environ['GITHUB_RUN_ID'],
    'environment': 'GitHub-hosted windows-2022; Rust 1.93.1; debug tests; serial targeted comparison',
    'limits': ['Single paired CI run, timings are diagnostic, not a human productivity study', 'Reopened Settings connection is not a cold OS disk-cache or full process-start measurement', 'No physical Windows 11 desktop or real network disconnect', 'Final PR/main/release checks are separate'],
}
write('docs/research/2026-10-28-bulk-import-scaling-validation.json', json.dumps(validation, ensure_ascii=False, indent=2) + '\n')
print(json.dumps(validation, ensure_ascii=False, indent=2), flush=True)
# Keep reproducible tests and evidence; preparation machinery is not shipped.
Path('.github/workflows/prepare-import-scaling.yml').unlink()
Path('scripts/prepare-import-scaling.py').unlink()
run(['git', 'diff', '--check'])
assert run(['git', 'ls-remote', 'origin', 'refs/heads/' + BRANCH]).stdout.split()[0] == START, 'Branch changed concurrently'
run(['git', 'config', 'user.name', 'TechWisard'])
run(['git', 'config', 'user.email', '152439417+TechWisard@users.noreply.github.com'])
run(['git', 'add', 'src/ui.rs', 'src/ui/import_ownership_tests.rs', 'src/ui/import_catalog.rs', 'src/ui/import_scaling_tests.rs', 'Cargo.toml', 'Cargo.lock', 'docs', 'scripts/prepare-import-scaling.py', '.github/workflows/prepare-import-scaling.yml'])
run(['git', 'commit', '-m', 'perf: keep bulk shortcut import identity matching linear'])
run(['git', 'push', 'origin', 'HEAD:refs/heads/' + BRANCH])
print('VERIFIED_CANDIDATE=' + run(['git', 'rev-parse', 'HEAD']).stdout.strip(), flush=True)
