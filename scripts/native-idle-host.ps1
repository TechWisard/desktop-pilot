param([Parameter(Mandatory)][string]$ReadyPath, [Parameter(Mandatory)][string]$StopPath)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
    throw 'This foreground fixture is restricted to a disposable GitHub-hosted runner'
}
Add-Type -AssemblyName System.Windows.Forms
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class IdleDesktopFixture {
    [StructLayout(LayoutKind.Sequential)] public struct LastInput { public uint Size; public uint Tick; }
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct MonitorInfo { public uint Size; public Rect Monitor, Work; public uint Flags; }
    [DllImport("kernel32.dll")] public static extern uint GetTickCount();
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool GetLastInputInfo(ref LastInput input);
    [DllImport("user32.dll")] public static extern short GetAsyncKeyState(int key);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern IntPtr MonitorFromWindow(IntPtr window, uint flags);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern bool GetMonitorInfo(IntPtr monitor, ref MonitorInfo info);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, StringBuilder text, int capacity);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr window, StringBuilder text, int capacity);
    public static string Class(IntPtr window) {
        var kind = new StringBuilder(256); GetClassName(window, kind, kind.Capacity); return kind.ToString();
    }
    public static string Describe(IntPtr window) {
        var text = new StringBuilder(256); GetWindowText(window, text, text.Capacity); return Class(window) + ": " + text.ToString();
    }
    public static bool NonFullscreen(IntPtr window) {
        if (window == IntPtr.Zero) return false;
        Rect rect; var info = new MonitorInfo { Size = (uint)Marshal.SizeOf(typeof(MonitorInfo)) };
        if (!GetWindowRect(window, out rect) || !GetMonitorInfo(MonitorFromWindow(window, 2), ref info)) return false;
        string kind = Class(window);
        return kind == "Progman" || kind == "WorkerW" || !(rect.Left <= info.Monitor.Left && rect.Top <= info.Monitor.Top && rect.Right >= info.Monitor.Right && rect.Bottom >= info.Monitor.Bottom);
    }
    public static long IdleMilliseconds() {
        var input = new LastInput { Size = (uint)Marshal.SizeOf(typeof(LastInput)) };
        return GetLastInputInfo(ref input) ? (long)unchecked(GetTickCount() - input.Tick) : -1;
    }
    public static int HeldKey() {
        for (int key = 1; key <= 254; key++) if (GetAsyncKeyState(key) < 0) return key;
        return 0;
    }
}
'@
# Show Desktop is a normal shell action, not a policy, privilege or input-guard
# override. This only runs on a fresh disposable hosted VM, never a user PC.
$shell = New-Object -ComObject Shell.Application
try { $shell.MinimizeAll() } finally { [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell) }
Write-Host ("Hosted foreground after Show Desktop: " + [IdleDesktopFixture]::Describe([IdleDesktopFixture]::GetForegroundWindow()))
$form = [Windows.Forms.Form]::new()
$form.Text = 'Desktop Pilot native acceptance fixture'
$form.ClientSize = [Drawing.Size]::new(460, 160)
$form.StartPosition = [Windows.Forms.FormStartPosition]::CenterScreen
$form.FormBorderStyle = [Windows.Forms.FormBorderStyle]::FixedDialog
$form.MaximizeBox = $false
$label = [Windows.Forms.Label]::new()
$label.Text = 'Isolated acceptance session. No user files or settings are changed.'
$label.Dock = [Windows.Forms.DockStyle]::Fill
$form.Controls.Add($label)
$form.Add_Shown({
    $form.Activate()
    $activated = [IdleDesktopFixture]::SetForegroundWindow($form.Handle)
    Write-Host "Fixture foreground request: $activated; handle=$($form.Handle)"
})
$started = [Diagnostics.Stopwatch]::StartNew()
$timer = [Windows.Forms.Timer]::new()
$timer.Interval = 1000
$timer.Add_Tick({
    if ((Test-Path -LiteralPath $StopPath) -or $started.Elapsed.TotalSeconds -gt 180) { $form.Close(); return }
    $idle = [IdleDesktopFixture]::IdleMilliseconds()
    $foreground = [IdleDesktopFixture]::GetForegroundWindow()
    $key = [IdleDesktopFixture]::HeldKey()
    $nonFullscreen = [IdleDesktopFixture]::NonFullscreen($foreground)
    $state = @{session=(Get-Process -Id $PID).SessionId; foreground=$foreground.ToInt64(); foreground_class=[IdleDesktopFixture]::Class($foreground); fixture=$form.Handle.ToInt64(); idle_milliseconds=$idle; held_key=$key; non_fullscreen=$nonFullscreen; elapsed_seconds=[int]$started.Elapsed.TotalSeconds}
    $json = $state | ConvertTo-Json -Compress
    [IO.File]::WriteAllText("$ReadyPath.status.json", $json)
    if ([int]$started.Elapsed.TotalSeconds % 5 -eq 0) { Write-Host $json }
    # Any real, idle, non-fullscreen foreground (including Explorer's desktop)
    # is a valid environment. Requiring this helper itself to own focus was an
    # extra fixture assumption, not the production contract. The executable
    # independently rechecks its unchanged quiet_for_changes at every boundary.
    if (-not (Test-Path -LiteralPath $ReadyPath) -and $nonFullscreen -and $idle -ge 16000 -and $key -eq 0 -and [IdleDesktopFixture]::GetForegroundWindow() -eq $foreground) {
        [IO.File]::WriteAllText($ReadyPath, $json)
    }
})
try { $timer.Start(); [Windows.Forms.Application]::Run($form) }
finally { $timer.Stop(); $timer.Dispose(); $form.Dispose() }
