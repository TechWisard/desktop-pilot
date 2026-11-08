param(
    [string]$Executable = 'target/debug/desktop-pilot.exe',
    [string]$ReportDirectory = 'target/ui-review',
    [switch]$SoftwareRenderer = ($env:GITHUB_ACTIONS -eq 'true')
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'ui-review-input.ps1')
$sourceExecutable = (Resolve-Path -LiteralPath $Executable).Path
$exe = $sourceExecutable
$report = [IO.Path]::GetFullPath($ReportDirectory)
[IO.Directory]::CreateDirectory($report) | Out-Null
$fixtureRoot = Join-Path ([IO.Path]::GetTempPath()) ('test-pilot-ui-review-' + [guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($fixtureRoot) | Out-Null
$records = [Collections.Generic.List[object]]::new()
$renderer = 'native OpenGL'
$primaryFailure = $null
$emulatedX64 = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq [Runtime.InteropServices.Architecture]::Arm64
function Invoke-Fixture([string[]]$Arguments, [string]$Label, [int]$Timeout = 45000, [switch]$ScrollBottom) {
    $info = [Diagnostics.ProcessStartInfo]::new($exe)
    $info.UseShellExecute = $false
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    if ($SoftwareRenderer) {
        $info.Environment['GALLIUM_DRIVER'] = 'llvmpipe'
        $info.Environment['LP_NUM_THREADS'] = '2'
        if ($emulatedX64) {
            # LLVM issue 214488: Windows x64 emulation can report a CPU name
            # that implies unsupported FMA4 to the test driver's shader JIT.
            # Scope the documented Mesa cap to this child only, not the OS
            # or application. The native renderer still draws every frame.
            $info.Environment['GALLIUM_OVERRIDE_CPU_CAPS'] = 'sse4.1'
        }
    }
    foreach ($argument in $Arguments) { $info.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::Start($info)
    $output = $process.StandardOutput.ReadToEndAsync()
    $errors = $process.StandardError.ReadToEndAsync()
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $scrollMessages = 0
    try {
        while (-not $process.WaitForExit(100)) {
            if ($watch.ElapsedMilliseconds -gt $Timeout) { $process.Kill($true); throw "Native UI timed out: $Label" }
            if ($ScrollBottom -and $watch.ElapsedMilliseconds -ge 1000) {
                $process.Refresh()
                if ([PilotUiReviewInput]::ScrollDown($process.MainWindowHandle, [uint32]$process.Id)) { $scrollMessages++ }
            }
        }
        $stdout = $output.GetAwaiter().GetResult()
        $stderr = $errors.GetAwaiter().GetResult()
        [IO.File]::WriteAllText((Join-Path $report "$Label-stdout.txt"), $stdout)
        [IO.File]::WriteAllText((Join-Path $report "$Label-stderr.txt"), $stderr)
        @{exit_code=$process.ExitCode; elapsed_ms=$watch.ElapsedMilliseconds; scroll_messages=$scrollMessages} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $report "$Label-process.json") -Encoding utf8
        if ($process.ExitCode -ne 0) { throw "Native UI failed: $Label ($($process.ExitCode)): $stderr" }
        if ($ScrollBottom -and $scrollMessages -eq 0) { throw "No window-scoped scroll input was delivered: $Label" }
        return $scrollMessages
    } finally {
        if (-not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
        $process.Dispose()
    }
}
try {
    if ($SoftwareRenderer) {
        # App-local test deployment only: no system installation or release DLL changes.
        $archive = Join-Path $fixtureRoot 'mesa.7z'
        $mesaUrl = 'https://github.com/pal1000/mesa-dist-win/releases/download/26.2.4/mesa3d-26.2.4-release-msvc.7z'
        $mesaSha = '351fc8c8b695878ffb3eaa044b3ead08672a48b1a045e3c3e3975811df0f6695'
        . (Join-Path $PSScriptRoot 'ui-download.ps1')
        Invoke-VerifiedUiRendererDownload -Uri $mesaUrl -Destination $archive -ExpectedSha256 $mesaSha
        $mesa = Join-Path $fixtureRoot 'mesa'
        & 7z x $archive "-o$mesa" -y | Out-Null
        if ($LASTEXITCODE) { throw 'Could not extract the verified software renderer' }
        $application = Join-Path $fixtureRoot 'application'
        [IO.Directory]::CreateDirectory($application) | Out-Null
        $exe = Join-Path $application 'desktop-pilot.exe'
        Copy-Item -LiteralPath $sourceExecutable -Destination $exe
        $driver = Get-ChildItem -LiteralPath $mesa -Recurse -Filter opengl32.dll | Where-Object { $_.Directory.Name -eq 'x64' }
        if (@($driver).Count -ne 1) { throw 'Expected one x64 software OpenGL driver' }
        Get-ChildItem -LiteralPath $driver.Directory.FullName -Filter '*.dll' | Copy-Item -Destination $application
        # Retain upstream licenses with the complete temporary distribution.
        # No driver files are included in the product or uploaded as evidence.
        $renderer = 'Mesa 26.2.4 llvmpipe; app-local test driver; not a hardware GPU test'
        if ($emulatedX64) { $renderer += '; emulated x64, GALLIUM_OVERRIDE_CPU_CAPS=sse4.1' }
        [IO.File]::WriteAllText((Join-Path $report 'renderer.txt'), "$renderer`n$mesaUrl`nSHA256 $mesaSha`n")
    }
    foreach ($language in @('ru', 'en')) {
        foreach ($scale in @(1, 2)) {
            foreach ($view in @('organization', 'organization-filtered', 'organization-files', 'organization-warning')) {
                foreach ($bottom in @($false, $true)) {
                    if ($bottom -and $scale -ne 2) { continue }
                    $suffix = if ($bottom) { '-bottom' } else { '' }
                    $name = "$language-$($scale * 100)-$view$suffix"
                    $profileHome = Join-Path $fixtureRoot "test-$name"
                    [IO.Directory]::CreateDirectory($profileHome) | Out-Null
                    $seed = Join-Path $profileHome 'seed.json'
                    [IO.File]::WriteAllText($seed, '[]')
                    Invoke-Fixture -Label "$name-init" -Arguments @('--init', $seed, '--home', $profileHome, '--out', (Join-Path $profileHome 'initialized.json')) | Out-Null
                    @'
import json, pathlib, sqlite3, sys
home = pathlib.Path(sys.argv[1])
language, scale, view = sys.argv[2], float(sys.argv[3]), sys.argv[4]
bottom = sys.argv[5].lower() == 'true'
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
              'test_scroll_bottom': bottom,
              'ui_scale': scale, 'test_viewport': [1120.0, 820.0],
              'test_layout': {'items': [], 'spacing': {'x': 80, 'y': 100}, 'origin': {'x': 0, 'y': 0},
                              'work': [0, 0, 1920, 1040], 'flags': 0, 'topology': 'isolated-fixture', 'icon_size': 32}}
    db.executemany('INSERT OR REPLACE INTO kv(k,v) VALUES (?,?)', [(k, json.dumps(v)) for k, v in values.items()])
'@ | python - $profileHome $language $scale $view $bottom
                    if ($LASTEXITCODE) { throw "Fixture setup failed: $name" }
                    $destination = Join-Path $report $name
                    $scrollMessages = Invoke-Fixture -Label $name -ScrollBottom:$bottom -Arguments @('--ui-smoke', $destination, '--home', $profileHome)
                    $image = Join-Path $destination "dialog-$view.png"
                    if (-not (Test-Path -LiteralPath $image) -or (Get-Item -LiteralPath $image).Length -lt 4096) { throw "Native screenshot is missing or empty: $name" }
                    foreach ($leaf in @('Project Alpha.url', 'Project Beta.url', 'Reference notes.url', 'Notes one.txt', 'Notes two.txt', 'Existing project')) {
                        if (-not (Test-Path -LiteralPath (Join-Path (Join-Path $profileHome 'Desktop') $leaf))) { throw "UI capture changed a fixture: $leaf" }
                    }
                    if (@(Get-ChildItem -LiteralPath (Join-Path $profileHome 'Desktop')).Count -ne 6) { throw 'UI capture created an unexpected desktop object' }
                    $records.Add(@{view=$view; language=$language; scale=$scale; scrolled=$bottom; scroll_messages=$scrollMessages; image="$name/dialog-$view.png"; sha256=(Get-FileHash -LiteralPath $image -Algorithm SHA256).Hash.ToLowerInvariant(); exit_code=0})
                    Write-Host "PASS native UI: $name (scroll messages: $scrollMessages)"
                }
            }
        }
    }
    @{os=(Get-CimInstance Win32_OperatingSystem).Caption; renderer=$renderer; executable_sha256=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant(); captures=@($records)} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $report 'manifest.json') -Encoding utf8
    Write-Host "Native UI: $($records.Count) captures verified; no desktop fixtures moved"
} catch {
    $primaryFailure = $_
    [IO.File]::WriteAllText((Join-Path $report 'failure.txt'), $_.ToString() + "`n" + $_.ScriptStackTrace)
    throw
} finally {
    # Windows image teardown can briefly retain the exited EXE. Retry only
    # deletion of this unique fixture. Never mask the original test failure.
    $cleanupFailure = $null
    for ($attempt = 0; $attempt -lt 20; $attempt++) {
        if (-not (Test-Path -LiteralPath $fixtureRoot)) { $cleanupFailure = $null; break }
        try {
            Remove-Item -LiteralPath $fixtureRoot -Recurse -Force
            $cleanupFailure = $null
            break
        } catch {
            $cleanupFailure = $_
            Start-Sleep -Milliseconds 250
        }
    }
    if ($cleanupFailure) {
        [IO.File]::WriteAllText((Join-Path $report 'cleanup-failure.txt'), $cleanupFailure.ToString())
        if (-not $primaryFailure) { throw $cleanupFailure }
        Write-Warning "Fixture cleanup also failed: $cleanupFailure"
    }
}
