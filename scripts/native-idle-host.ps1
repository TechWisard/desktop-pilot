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
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct MouseInput { public int X, Y; public uint Data, Flags, Time; public UIntPtr Extra; }
    [StructLayout(LayoutKind.Sequential)] public struct Input { public uint Type; public MouseInput Mouse; }
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool GetLastInputInfo(ref LastInput input);
    [DllImport("user32.dll")] public static extern short GetAsyncKeyState(int key);
    [DllImport("kernel32.dll")] public static extern uint GetTickCount();
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr window, uint flags);
    [DllImport("user32.dll", SetLastError=true)] public static extern uint SendInput(uint count, Input[] inputs, int size);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, StringBuilder text, int capacity);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr window, StringBuilder text, int capacity);
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
    public static void ClickOwnWindow(IntPtr window) {
        Rect rect;
        if (!GetWindowRect(window, out rect)) throw new InvalidOperationException("Fixture window has no rectangle");
        var point = new Point { X = (rect.Left + rect.Right) / 2, Y = (rect.Top + rect.Bottom) / 2 };
        if (!SetCursorPos(point.X, point.Y) || GetAncestor(WindowFromPoint(point), 2) != window)
            throw new InvalidOperationException("Fixture is not the actual input target; refusing to click another window");
        var inputs = new[] {
            new Input { Type = 0, Mouse = new MouseInput { Flags = 0x0002 } },
            new Input { Type = 0, Mouse = new MouseInput { Flags = 0x0004 } }
        };
        if (SendInput(2, inputs, Marshal.SizeOf(typeof(Input))) != 2)
            throw new InvalidOperationException("Fixture activation input was rejected: " + Marshal.GetLastWin32Error());
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
    $form.TopMost = $true
    $form.Activate()
    $activated = [IdleDesktopFixture]::SetForegroundWindow($form.Handle)
    Write-Host "Fixture activation: $activated; handle=$($form.Handle)"
    if ([IdleDesktopFixture]::GetForegroundWindow() -ne $form.Handle) {
        # Focus the fixture exactly as a UI test would. This is not idle
        # injection: readiness still waits for 16 seconds of actual inactivity.
        [IdleDesktopFixture]::ClickOwnWindow($form.Handle)
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
