from pathlib import Path
import shutil
import subprocess

organizer = '9424d6f3eed20df8707c15e2858b340b6f46041f'
result = subprocess.run(['git', 'merge', '--no-commit', '--no-ff', organizer])
if result.returncode not in (0, 1):
    raise SystemExit('Merge failed before conflict review')
conflicts = subprocess.check_output(['git', 'diff', '--name-only', '--diff-filter=U'], text=True).splitlines()
if any(path != '.github/workflows/checks.yml' for path in conflicts):
    raise SystemExit('Unreviewed merge conflict: ' + repr(conflicts))
shutil.copyfile('scripts/review-checks.yml', '.github/workflows/checks.yml')
ui = Path('src/ui.rs')
text = ui.read_text()
assert text.startswith('mod organization;\n')
text = text.replace('mod organization;\n', 'mod organization;\nmod smoke;\n', 1)
needle = '            "import" => {\n                app.tab = 1;'
assert text.count(needle) == 1
text = text.replace(needle, '            "organization" | "organization-filtered" | "organization-files" | "organization-warning" => {\n                smoke::organization(&mut app, &view)?;\n            }\n' + needle, 1)
ui.write_text(text)
module = Path('src/organize/mod.rs')
text = module.read_text()
needle = '#[cfg(test)]\nmod acceptance_tests;'
assert text.count(needle) == 1
module.write_text(text.replace(needle, needle + '\n#[cfg(test)]\nmod live_tests;', 1))
for path in ['.github/workflows/integration-review.yml', 'scripts/review-checks.yml', 'scripts/integrate-organizer.py']:
    Path(path).unlink()
