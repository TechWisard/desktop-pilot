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
