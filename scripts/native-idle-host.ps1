param([Parameter(Mandatory)][string]$ReadyPath, [Parameter(Mandatory)][string]$StopPath)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
    throw 'This foreground fixture is restricted to a disposable GitHub-hosted runner'
}
Add-Type -AssemblyName System.Windows.Forms
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class IdleDesktopFixture {
    [StructLayout(LayoutKind.Sequential)]
    public struct LastInput { public uint Size; public uint Tick; }
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool GetLastInputInfo(ref LastInput input);
    [DllImport("user32.dll")] public static extern short GetAsyncKeyState(int key);
    [DllImport("kernel32.dll")] public static extern uint GetTickCount();
    public static long IdleMilliseconds() {
        var input = new LastInput { Size = (uint)Marshal.SizeOf(typeof(LastInput)) };
        return GetLastInputInfo(ref input) ? (long)unchecked(GetTickCount() - input.Tick) : -1;
    }
    public static bool KeyHeld() {
        for (int key = 1; key <= 254; key++) if (GetAsyncKeyState(key) < 0) return true;
        return false;
    }
}
'@
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
$form.Add_Shown({ $form.Activate() })
$started = [Diagnostics.Stopwatch]::StartNew()
$timer = [Windows.Forms.Timer]::new()
$timer.Interval = 100
$timer.Add_Tick({
    if ((Test-Path -LiteralPath $StopPath) -or $started.Elapsed.TotalSeconds -gt 180) {
        $form.Close()
        return
    }
    $idle = [IdleDesktopFixture]::IdleMilliseconds()
    if (-not (Test-Path -LiteralPath $ReadyPath) -and
        [IdleDesktopFixture]::GetForegroundWindow() -eq $form.Handle -and
        $idle -ge 16000 -and -not [IdleDesktopFixture]::KeyHeld()) {
        @{foreground_fixture=$true; idle_milliseconds=$idle; held_keys=$false; fullscreen=$false} |
            ConvertTo-Json | Set-Content -LiteralPath $ReadyPath -Encoding utf8
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
