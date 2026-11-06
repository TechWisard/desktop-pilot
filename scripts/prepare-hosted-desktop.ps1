$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
    throw 'Desktop fixture preparation is restricted to a disposable GitHub-hosted runner'
}
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class HostedWelcome {
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, StringBuilder text, int capacity);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr window, StringBuilder text, int capacity);
    [DllImport("user32.dll", SetLastError=true)] public static extern bool PostMessage(IntPtr window, uint message, UIntPtr wparam, IntPtr lparam);
}
'@
$window = [HostedWelcome]::GetForegroundWindow()
$caption = [Text.StringBuilder]::new(256)
$kind = [Text.StringBuilder]::new(256)
[void][HostedWelcome]::GetWindowText($window, $caption, 256)
[void][HostedWelcome]::GetClassName($window, $kind, 256)
[uint32]$ownerId = 0
[void][HostedWelcome]::GetWindowThreadProcessId($window, [ref]$ownerId)
if ($ownerId -eq 0) { Write-Host 'No pre-existing foreground window'; return }
$owner = Get-Process -Id $ownerId
Write-Host "Hosted foreground: class=$kind caption=$caption process=$($owner.ProcessName) session=$($owner.SessionId) path=$($owner.Path)"
# Some fresh hosted Windows 11 images present the optional Microsoft-account
# welcome screen. Close only that verified Windows-owned welcome window.
# Never change an account, sign-in policy, lock state, or desktop settings.
$allowed = @('WWAHost', 'CloudExperienceHostBroker', 'UserOOBEBroker', 'ApplicationFrameHost', 'msoobe')
if ($caption.ToString() -eq 'Microsoft account' -and
    $kind.ToString() -eq 'Windows.UI.Core.CoreWindow' -and
    $owner.ProcessName -in $allowed -and
    $owner.SessionId -eq (Get-Process -Id $PID).SessionId -and
    $owner.Path.StartsWith($env:SystemRoot + '\', [StringComparison]::OrdinalIgnoreCase)) {
    if (-not [HostedWelcome]::PostMessage($window, 0x0010, [UIntPtr]::Zero, [IntPtr]::Zero)) {
        throw "The hosted welcome window could not be closed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    Start-Sleep -Seconds 2
    Write-Host 'Requested normal close of the optional hosted welcome window'
}
