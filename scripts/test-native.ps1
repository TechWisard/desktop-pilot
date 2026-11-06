param(
    [string]$Executable = 'target/debug/desktop-pilot.exe',
    [string]$ReportDirectory = 'target/native-checks'
)
$ErrorActionPreference = 'Stop'
$exe = (Resolve-Path -LiteralPath $Executable).Path
$reports = [System.IO.Path]::GetFullPath($ReportDirectory)
[System.IO.Directory]::CreateDirectory($reports) | Out-Null
$profile = Join-Path ([System.IO.Path]::GetTempPath()) ('test-desktop-pilot-' + [guid]::NewGuid().ToString('N'))
$report = Join-Path $reports 'native-integration.json'
# Automatic-review fixtures deliberately retain the production idle/fullscreen
# checks. A hosted Windows image can start without any foreground window.
# Give only that disposable session a real, non-fullscreen fixture window;
# never bypass the guard or activate a window on an ordinary user's desktop.
$desktopHost = $null
$hostRoot = Join-Path ([IO.Path]::GetTempPath()) ('test-pilot-idle-host-' + [guid]::NewGuid().ToString('N'))
try {
    if ($env:GITHUB_ACTIONS -eq 'true' -and $env:RUNNER_ENVIRONMENT -eq 'github-hosted') {
        [IO.Directory]::CreateDirectory($hostRoot) | Out-Null
        $ready = Join-Path $hostRoot 'ready.json'
        $stop = Join-Path $hostRoot 'stop'
        $hostInfo = [Diagnostics.ProcessStartInfo]::new((Get-Process -Id $PID).Path)
        $hostInfo.UseShellExecute = $false
        foreach ($argument in @('-NoProfile', '-STA', '-File', (Join-Path $PSScriptRoot 'native-idle-host.ps1'), '-ReadyPath', $ready, '-StopPath', $stop)) {
            $hostInfo.ArgumentList.Add($argument)
        }
        $desktopHost = [Diagnostics.Process]::Start($hostInfo)
        $deadline = [DateTime]::UtcNow.AddSeconds(60)
        while (-not (Test-Path -LiteralPath $ready)) {
            if ($desktopHost.HasExited -or [DateTime]::UtcNow -gt $deadline) { throw 'Disposable desktop did not reach the real idle condition' }
            Start-Sleep -Milliseconds 200
        }
        Copy-Item -LiteralPath $ready -Destination (Join-Path $reports 'idle-desktop.json')
    }
    $info = [System.Diagnostics.ProcessStartInfo]::new($exe)
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    foreach ($argument in @('--self-test', '--home', $profile, '--out', $report)) {
        $info.ArgumentList.Add($argument)
    }
    $process = [System.Diagnostics.Process]::Start($info)
    $output = $process.StandardOutput.ReadToEndAsync()
    $errors = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit(120000)) {
        $process.Kill($true)
        throw 'Native integration checks exceeded 120 seconds'
    }
    $stdout = $output.GetAwaiter().GetResult()
    $stderr = $errors.GetAwaiter().GetResult()
    [System.IO.File]::WriteAllText((Join-Path $reports 'stdout.txt'), $stdout)
    [System.IO.File]::WriteAllText((Join-Path $reports 'stderr.txt'), $stderr)
    if ($process.ExitCode -ne 0) { throw "Native integration checks failed: $stderr" }
    if (-not (Test-Path -LiteralPath $report)) { throw 'Native integration report is missing' }
    $result = Get-Content -Raw -LiteralPath $report | ConvertFrom-Json
    if ($result.passed -ne $true -or $result.originals_unchanged -ne $true) {
        throw 'Native integration report did not confirm success'
    }
    Write-Host ("Native integration: {0} checks passed" -f $result.checks.Count)
    $result.checks | ForEach-Object { Write-Host "  $_" }
} finally {
    if ($desktopHost) {
        [IO.File]::WriteAllText((Join-Path $hostRoot 'stop'), 'stop')
        if (-not $desktopHost.WaitForExit(5000)) { $desktopHost.Kill($true) }
        $desktopHost.Dispose()
    }
    if (Test-Path -LiteralPath $hostRoot) { Remove-Item -LiteralPath $hostRoot -Recurse -Force }
}
