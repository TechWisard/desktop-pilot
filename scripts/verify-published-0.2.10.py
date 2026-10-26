"""Verify one public Desktop Pilot release without executing it or changing GitHub."""
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import struct
import subprocess
import tomllib
import urllib.request
import zipfile

REPO = 'TechWisard/desktop-pilot'
VERSION = '0.2.10'
EXPECTED_COMMIT = os.environ['EXPECTED_RELEASE_COMMIT']
assert re.fullmatch('[0-9a-f]{40}', EXPECTED_COMMIT)
TAG = 'v' + VERSION
NAMES = [f'DesktopPilot-{VERSION}-windows-x64.zip', f'DesktopPilot-{VERSION}-source.zip', 'SHA256SUMS.txt']


def api(path):
    headers = {'User-Agent': 'DesktopPilot-public-release-verification', 'Accept': 'application/vnd.github+json'}
    if os.environ.get('GH_TOKEN'):
        headers['Authorization'] = 'Bearer ' + os.environ['GH_TOKEN']
    req = urllib.request.Request('https://api.github.com/repos/' + REPO + '/' + path, headers=headers)
    with urllib.request.urlopen(req, timeout=40) as response:
        return json.load(response)


def public_bytes(url):
    # Deliberately no credentials: verify the actual links users receive.
    assert url.startswith('https://github.com/' + REPO + '/releases/download/' + TAG + '/')
    req = urllib.request.Request(url, headers={'User-Agent': 'DesktopPilot-public-release-verification'})
    with urllib.request.urlopen(req, timeout=60) as response:
        assert response.status == 200
        data = response.read(32 * 1024 * 1024 + 1)
        assert len(data) <= 32 * 1024 * 1024, 'Unexpectedly large asset'
        return data


release = api('releases/tags/' + TAG)
assert release['tag_name'] == TAG
assert release['draft'] is False and release['prerelease'] is False
assert release['target_commitish'] == EXPECTED_COMMIT
ref = api('git/ref/tags/' + TAG)['object']
for _ in range(5):
    if ref['type'] != 'tag':
        break
    ref = api('git/tags/' + ref['sha'])['object']
assert ref['type'] == 'commit' and ref['sha'] == EXPECTED_COMMIT
assets = release['assets']
assert len(assets) == 3 and sorted(a['name'] for a in assets) == sorted(NAMES)
contents = {}
result = {
    'release': release['html_url'], 'release_id': release['id'],
    'version': VERSION, 'commit': EXPECTED_COMMIT,
    'public_downloads_without_authentication': True,
    'assets': [], 'checks': [],
}
for name in NAMES:
    asset = next(a for a in assets if a['name'] == name)
    assert asset['state'] == 'uploaded' and asset['size'] > 0
    data = public_bytes(asset['browser_download_url'])
    digest = hashlib.sha256(data).hexdigest()
    assert len(data) == asset['size']
    assert asset['digest'] == 'sha256:' + digest, name
    contents[name] = data
    result['assets'].append({'name': name, 'bytes': len(data), 'sha256': digest, 'download_url': asset['browser_download_url']})
sums = {}
for line in contents['SHA256SUMS.txt'].decode('utf-8-sig').splitlines():
    match = re.fullmatch(r'([0-9a-f]{64})  (\S+)', line)
    assert match and match[2] not in sums
    sums[match[2]] = match[1]
assert set(sums) == set(NAMES[:2])
for name, digest in sums.items():
    assert hashlib.sha256(contents[name]).hexdigest() == digest
result['checks'].append('Public assets, uploaded digests and SHA256SUMS agree')


def archive(name):
    z = zipfile.ZipFile(io.BytesIO(contents[name]))
    assert z.testzip() is None
    names = z.namelist()
    assert len(set(names)) == len(names)
    assert sum(i.file_size for i in z.infolist()) < 100 * 1024 * 1024
    for path in names:
        assert not path.startswith(('/', '\\')) and '..' not in PurePosixPath(path).parts
    return z


source = archive(NAMES[1])
assert source.comment.decode('ascii') == EXPECTED_COMMIT
prefix = f'DesktopPilot-{VERSION}-source/'
source_files = {i.filename[len(prefix):]: source.read(i) for i in source.infolist() if not i.is_dir() and i.filename.startswith(prefix)}
assert len(source_files) == len([i for i in source.infolist() if not i.is_dir()])
tree_response = api('git/trees/' + EXPECTED_COMMIT + '?recursive=1')
assert tree_response.get('truncated') is False
blobs = {e['path']: e['sha'] for e in tree_response['tree'] if e['type'] == 'blob'}
assert set(source_files) == set(blobs), 'Source archive/tree file sets differ'
eol_normalized = []
for path, data in source_files.items():
    def git_hash(value):
        return hashlib.sha1(b'blob ' + str(len(value)).encode() + b'\0' + value).hexdigest()
    if git_hash(data) != blobs[path]:
        assert b'\0' not in data and b'\r\n' in data, 'Source mismatch: ' + path
        assert git_hash(data.replace(b'\r\n', b'\n')) == blobs[path], 'Source mismatch: ' + path
        eol_normalized.append(path)
result['source_eol_conversion_files'] = eol_normalized
assert tomllib.loads(source_files['Cargo.toml'].decode())['package']['version'] == VERSION
packages = tomllib.loads(source_files['Cargo.lock'].decode())['package']
assert [p['version'] for p in packages if p['name'] == 'desktop-pilot'] == [VERSION]
assert source_files[f'docs/releases/{VERSION}.md'].decode().strip().replace('\r\n', '\n') == release['body'].strip().replace('\r\n', '\n')
assert 'scripts/complete-0.2.10.py' not in source_files
assert '.github/workflows/finish-manual-absence.yml' not in source_files
result['checks'].append(f'Source ZIP comment and all {len(source_files)} files match the tag commit, allowing only verified Git CRLF conversion')
result['checks'].append('Cargo manifest, lockfile and release notes agree on version and contents')

binary = archive(NAMES[0])
bin_files = {i.filename: binary.read(i) for i in binary.infolist() if not i.is_dir()}
required = {'desktop-pilot.exe', 'README.md', 'README.ru.md', 'THIRD-PARTY-NOTICES.txt', 'BUILD-DEPENDENCY-NOTICES.txt', 'disable.ps1'}
expected_bin = required | {p for p in source_files if p.startswith('docs/')}
assert set(bin_files) == expected_bin, 'Unexpected or missing packaged files'
packaged_eol = []
for path in expected_bin - {'desktop-pilot.exe'}:
    src = 'scripts/disable.ps1' if path == 'disable.ps1' else path
    if bin_files[path] != source_files[src]:
        assert b'\0' not in bin_files[path] and b'\0' not in source_files[src]
        assert bin_files[path].replace(b'\r\n', b'\n') == source_files[src].replace(b'\r\n', b'\n'), 'Packaged content mismatch: ' + path
        packaged_eol.append(path)
result['packaged_eol_conversion_files'] = packaged_eol
exe = bin_files['desktop-pilot.exe']
assert exe[:2] == b'MZ'
pe = struct.unpack_from('<I', exe, 0x3c)[0]
assert exe[pe:pe + 4] == b'PE\0\0'
assert struct.unpack_from('<H', exe, pe + 4)[0] == 0x8664
assert struct.unpack_from('<H', exe, pe + 24)[0] == 0x20b
Path('downloaded-desktop-pilot.exe').write_bytes(exe)
product_version = subprocess.check_output(['pwsh', '-NoProfile', '-Command', "(Get-Item -LiteralPath './downloaded-desktop-pilot.exe').VersionInfo.ProductVersion"], text=True).strip()
assert product_version == VERSION
result['executable_sha256'] = hashlib.sha256(exe).hexdigest()
result['checks'].append('Downloaded executable is Windows x64 with matching ProductVersion')
result['checks'].append('Packaged documents, licenses and disable script match source content; any CRLF-only differences are listed')
result['checks'].append('ZIP CRC validation passed; no duplicate or unsafe archive paths')
result['workflow_run'] = 'https://github.com/' + REPO + '/actions/runs/' + os.environ['GITHUB_RUN_ID']
result['executed_downloaded_binary'] = False
result['passed'] = True
report = json.dumps(result, ensure_ascii=False, indent=2)
Path('release-download-verification.json').write_text(report + '\n', encoding='utf-8')
print(report)
