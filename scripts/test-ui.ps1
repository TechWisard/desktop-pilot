param([string]$Executable = 'target/debug/desktop-pilot.exe', [string]$ReportDirectory = 'target/ui-review')
$ErrorActionPreference = 'Stop'
$exe = (Resolve-Path -LiteralPath $Executable).Path
$report = [IO.Path]::GetFullPath($ReportDirectory)
[IO.Directory]::CreateDirectory($report) | Out-Null
$fixtureRoot = Join-Path ([IO.Path]::GetTempPath()) ('test-pilot-ui-review-' + [guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($fixtureRoot) | Out-Null
$records = [Collections.Generic.List[object]]::new()
try {
    foreach ($language in @('ru', 'en')) {
        foreach ($scale in @(1, 2)) {
            foreach ($view in @('organization', 'organization-filtered', 'organization-files', 'organization-warning')) {
                $name = "$language-$($scale * 100)-$view"
                $home = Join-Path $fixtureRoot "test-$name"
                [IO.Directory]::CreateDirectory($home) | Out-Null
                $seed = Join-Path $home 'seed.json'
                [IO.File]::WriteAllText($seed, '[]')
                $init = Start-Process -FilePath $exe -ArgumentList @('--init', ('"' + $seed + '"'), '--home', ('"' + $home + '"'), '--out', ('"' + (Join-Path $home 'initialized.json') + '"')) -PassThru
                if (-not $init.WaitForExit(30000)) { $init.Kill(); throw "Profile initialization timed out: $name" }
                if ($init.ExitCode -ne 0) { throw "Profile initialization failed: $name" }
                @'
import json, pathlib, sqlite3, sys
home = pathlib.Path(sys.argv[1])
language, scale, view = sys.argv[2], float(sys.argv[3]), sys.argv[4]
desktop = home / 'Desktop'
desktop.mkdir()
for name, address in [('Project Alpha', 'alpha'), ('Project Beta', 'beta'), ('Reference notes', 'reference')]:
    (desktop / (name + '.url')).write_text('[InternetShortcut]\r\nURL=https://example.invalid/' + address + '\r\n', encoding='utf-8')
for name in ['Notes one.txt', 'Notes two.txt']:
    (desktop / name).write_text('Synthetic review document; never a user file.', encoding='utf-8')
(desktop / 'Existing project').mkdir()
databases = [p for p in home.iterdir() if p.is_file() and p.read_bytes()[:16] == b'SQLite format 3\x00']
assert len(databases) == 1, databases
with sqlite3.connect(str(databases[0])) as db:
    config = json.loads(db.execute("SELECT v FROM kv WHERE k='config'").fetchone()[0])
    config.update(language=language, collect=False, autostart=False)
    values = {'config': config, 'setup_complete': True, 'background_disabled': True,
              'test_desktop': str(desktop), 'test_view': view, 'test_search': 'Project Alpha',
              'ui_scale': scale, 'test_viewport': [1120.0, 820.0],
              'test_layout': {'items': [], 'spacing': {'x': 80, 'y': 100}, 'origin': {'x': 0, 'y': 0},
                              'work': [0, 0, 1920, 1040], 'flags': 0, 'topology': 'isolated-fixture', 'icon_size': 32}}
    db.executemany('INSERT OR REPLACE INTO kv(k,v) VALUES (?,?)', [(k, json.dumps(v)) for k, v in values.items()])
'@ | python - $home $language $scale $view
                if ($LASTEXITCODE) { throw "Fixture setup failed: $name" }
                $destination = Join-Path $report $name
                $process = Start-Process -FilePath $exe -ArgumentList @('--ui-smoke', ('"' + $destination + '"'), '--home', ('"' + $home + '"')) -PassThru
                if (-not $process.WaitForExit(45000)) { $process.Kill(); throw "Native UI screenshot timed out: $name" }
                if ($process.ExitCode -ne 0) { throw "Native UI failed: $name" }
                $image = Join-Path $destination "dialog-$view.png"
                if (-not (Test-Path -LiteralPath $image) -or (Get-Item -LiteralPath $image).Length -lt 4096) { throw "Native screenshot is missing or empty: $name" }
                foreach ($leaf in @('Project Alpha.url', 'Project Beta.url', 'Reference notes.url', 'Notes one.txt', 'Notes two.txt', 'Existing project')) {
                    if (-not (Test-Path -LiteralPath (Join-Path (Join-Path $home 'Desktop') $leaf))) { throw "UI capture changed a fixture: $leaf" }
                }
                if (@(Get-ChildItem -LiteralPath (Join-Path $home 'Desktop')).Count -ne 6) { throw 'UI capture created an unexpected desktop object' }
                $records.Add(@{view=$view; language=$language; scale=$scale; image="$name/dialog-$view.png"; sha256=(Get-FileHash -LiteralPath $image -Algorithm SHA256).Hash.ToLowerInvariant(); exit_code=$process.ExitCode})
                Write-Host "PASS native UI: $name"
            }
        }
    }
    @{os=(Get-CimInstance Win32_OperatingSystem).Caption; executable_sha256=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant(); captures=@($records)} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $report 'manifest.json') -Encoding utf8
    Write-Host "Native UI: $($records.Count) captures verified; no desktop fixtures moved"
} finally {
    if (Test-Path -LiteralPath $fixtureRoot) { Remove-Item -LiteralPath $fixtureRoot -Recurse -Force }
}
