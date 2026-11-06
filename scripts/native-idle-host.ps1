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
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("kernel32.dll")] public static extern uint GetTickCount();
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
    [DllImport("user32.dll")] public static extern IntPtr GetThreadDesktop(uint threadId);
    [DllImport("user32.dll", SetLastError=true)] public static extern bool AttachThreadInput(uint from, uint to, bool attach);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr window);
    [DllImport("user32.dll")] public static extern bool GetLastInputInfo(ref LastInput input);
    [DllImport("user32.dll")] public static extern short GetAsyncKeyState(int key);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, StringBuilder text, int capacity);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr window, StringBuilder text, int capacity);
    [DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)] public static extern bool GetUserObjectInformation(IntPtr obj, int index, out int value, uint size, out uint needed);
    public static bool ActivateOwnWindow(IntPtr window) {
        uint own = GetCurrentThreadId(); uint ignored;
        uint other = GetWindowThreadProcessId(GetForegroundWindow(), out ignored);
        int receivesInput; uint needed;
        if (!GetUserObjectInformation(GetThreadDesktop(own), 6, out receivesInput, 4, out needed) || receivesInput == 0)
            throw new InvalidOperationException("The fixture is not on the active input desktop");
        bool attached = false;
        try {
            if (other != 0 && other != own) {
                attached = AttachThreadInput(own, other, true);
                if (!attached) throw new InvalidOperationException("Cannot attach the fixture input queue: " + Marshal.GetLastWin32Error());
            }
            BringWindowToTop(window);
            return SetForegroundWindow(window);
        } finally {
            if (attached) AttachThreadInput(own, other, false);
        }
    }
    public static string Describe(IntPtr window) {
        var text = new StringBuilder(256); var kind = new StringBuilder(256);
        GetWindowText(window, text, text.Capacity); GetClassName(window, kind, kind.Capacity);
        return kind.ToString() + ": " + text.ToString();
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
Write-Host "Disposable desktop: session=$((Get-Process -Id $PID).SessionId) interactive=$([Environment]::UserInteractive)"
Write-Host ("Initial foreground: " + [IdleDesktopFixture]::Describe([IdleDesktopFixture]::GetForegroundWindow()))
$form = [Windows.Forms.Form]::new()
$form.Text = 'Desktop Pilot native acceptance fixture'
$form.ClientSize = [Drawing.Size]::new(460, 160)
$form.StartPosition = [Windows.Forms.FormStartPosition]::CenterScreen
$form.FormBorderStyle = [Windows.Forms.FormBorderStyle]::FixedDialog
$form.MaximizeBox = $false
$label = [Windows.Forms.Label]::new()
$label.Text = 'Isolated acceptance session. No user files or settings are changed.'
$label.AutoSize = $false
$label.Dock = [Windows.Forms.DockStyle]::Fill
$form.Controls.Add($label)
$form.Add_Shown({
    try {
        $form.TopMost = $true
        $activated = [IdleDesktopFixture]::ActivateOwnWindow($form.Handle)
        Write-Host "Fixture activation: $activated; handle=$($form.Handle)"
    } catch {
        [IO.File]::WriteAllText("$ReadyPath.error.txt", $_.ToString())
        Write-Host "Fixture activation failed: $_"
        $form.Close()
    }
})
$started = [Diagnostics.Stopwatch]::StartNew()
$timer = [Windows.Forms.Timer]::new()
$timer.Interval = 1000
$timer.Add_Tick({
    if ((Test-Path -LiteralPath $StopPath) -or $started.Elapsed.TotalSeconds -gt 180) {
        $form.Close()
        return
    }
    $idle = [IdleDesktopFixture]::IdleMilliseconds()
    $foreground = [IdleDesktopFixture]::GetForegroundWindow()
    $key = [IdleDesktopFixture]::HeldKey()
    if ($foreground -eq $form.Handle) { $form.TopMost = $false }
    $state = @{session=(Get-Process -Id $PID).SessionId; foreground=$foreground.ToInt64(); fixture=$form.Handle.ToInt64(); idle_milliseconds=$idle; held_key=$key; elapsed_seconds=[int]$started.Elapsed.TotalSeconds}
    $json = $state | ConvertTo-Json -Compress
    [IO.File]::WriteAllText("$ReadyPath.status.json", $json)
    if ([int]$started.Elapsed.TotalSeconds % 5 -eq 0) { Write-Host $json }
    # Readiness is still the actual OS foreground and inactivity; never an
    # injected quiet result or a substitute for the application's guards.
    if (-not (Test-Path -LiteralPath $ReadyPath) -and $foreground -eq $form.Handle -and $idle -ge 16000 -and $key -eq 0) {
        [IO.File]::WriteAllText($ReadyPath, $json)
    }
})
try {
    $timer.Start()
    [Windows.Forms.Application]::Run($form)
} finally {
    $timer.Stop()
    $timer.Dispose()
    $form.Dispose()
}
