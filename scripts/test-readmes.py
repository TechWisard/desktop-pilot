"""Guard the first installation instructions against stale pinned release names."""
from pathlib import Path
import re

for path, heading in (
    ("README.md", "## Get started"),
    ("README.ru.md", "## Начало работы"),
):
    text = Path(path).read_text(encoding="utf-8")
    assert text.count(heading) == 1, path
    section = text.split(heading, 1)[1].split("\n## ", 1)[0]
    match = re.search(r"(?m)^1\. (.+)$", section)
    assert match, f"Missing first installation step in {path}"
    step = match.group(1)
    assert "https://github.com/TechWisard/desktop-pilot/releases/latest" in step, path
    assert "DesktopPilot-*-windows-x64.zip" in step, path
    assert not re.search(r"DesktopPilot-\d+\.\d+\.\d+-windows-x64\.zip", step), path
    assert "`desktop-pilot.exe`" in section, path
    assert "SHA-256" in section, path

print("README installation instructions: English and Russian use latest-release archives")

# Keep the public catalog limits and post-import instructions synchronized with
# the actual scanner and Settings controls, instead of freezing old release values.
scanner = Path("src/engine.rs").read_text(encoding="utf-8")
limit_match = re.search(r"MAX_DISCOVERED_SHORTCUTS:\s*usize\s*=\s*([0-9_]+)", scanner)
assert limit_match, "Missing scanner discovery limit"
limit = int(limit_match.group(1).replace("_", ""))
for path, opening, numeral, heading in (
    ("docs/guide.md", "The catalog reads personal", f"{limit:,}", "Last import"),
    ("docs/guide.ru.md", "Каталог собирается", str(limit), "Последний импорт"),
):
    content = Path(path).read_text(encoding="utf-8")
    introduction = next((line for line in content.splitlines() if line.startswith(opening)), "")
    assert introduction, f"Missing catalog instructions in {path}"
    assert numeral in introduction, f"Outdated shortcut scan limit in {path}"
    assert heading in introduction, f"Outdated import filter wording in {path}"

print("Catalog guides match the scanner limit and Last import behavior")
