param([string]$ReportDirectory = 'target/input-review')
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
    throw 'Input inspection is restricted to a disposable GitHub-hosted runner'
}
Add-Type -AssemblyName System.Windows.Forms
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class HostedInputInspection {
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] public static extern IntPtr GetThreadDesktop(uint thread);
    [DllImport("user32.dll")] public static extern IntPtr GetProcessWindowStation();
    [DllImport("user32.dll", SetLastError=true)] public static extern IntPtr OpenInputDesktop(uint flags, bool inherit, uint access);
    [DllImport("user32.dll")] public static extern bool CloseDesktop(IntPtr desktop);
    [DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)] public static extern bool GetUserObjectInformation(IntPtr obj, int index, IntPtr value, uint size, out uint needed);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, StringBuilder text, int capacity);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr window, StringBuilder text, int capacity);
    public static string Name(IntPtr obj) {
        IntPtr buffer = Marshal.AllocHGlobal(1024); uint needed;
        try { return GetUserObjectInformation(obj, 2, buffer, 1024, out needed) ? Marshal.PtrToStringUni(buffer) : "error:" + Marshal.GetLastWin32Error(); }
        finally { Marshal.FreeHGlobal(buffer); }
    }
    public static int IsInput(IntPtr obj) {
        IntPtr buffer = Marshal.AllocHGlobal(4); uint needed;
        try { return GetUserObjectInformation(obj, 6, buffer, 4, out needed) ? Marshal.ReadInt32(buffer) : -Marshal.GetLastWin32Error(); }
        finally { Marshal.FreeHGlobal(buffer); }
    }
}
'@
$threadDesktop = [HostedInputInspection]::GetThreadDesktop([HostedInputInspection]::GetCurrentThreadId())
$inputDesktop = [HostedInputInspection]::OpenInputDesktop(0, $false, 1)
$inputError = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
$window = [HostedInputInspection]::GetForegroundWindow()
[uint32]$ownerId = 0
[void][HostedInputInspection]::GetWindowThreadProcessId($window, [ref]$ownerId)
$caption = [Text.StringBuilder]::new(256)
$kind = [Text.StringBuilder]::new(256)
[void][HostedInputInspection]::GetWindowText($window, $caption, 256)
[void][HostedInputInspection]::GetClassName($window, $kind, 256)
$state = [ordered]@{
    session=(Get-Process -Id $PID).SessionId
    interactive=[Environment]::UserInteractive
    station=[HostedInputInspection]::Name([HostedInputInspection]::GetProcessWindowStation())
    thread_desktop=[HostedInputInspection]::Name($threadDesktop)
    thread_receives_input=[HostedInputInspection]::IsInput($threadDesktop)
    input_desktop=$(if ($inputDesktop -ne [IntPtr]::Zero) { [HostedInputInspection]::Name($inputDesktop) } else { "error:$inputError" })
    foreground_process=$ownerId
    foreground_class=$kind.ToString()
    foreground_title=$caption.ToString()
    screens=@([Windows.Forms.Screen]::AllScreens | ForEach-Object { @{device=$_.DeviceName; primary=$_.Primary; bounds=$_.Bounds.ToString()} })
}
if ($inputDesktop -ne [IntPtr]::Zero) { [void][HostedInputInspection]::CloseDesktop($inputDesktop) }
[IO.Directory]::CreateDirectory([IO.Path]::GetFullPath($ReportDirectory)) | Out-Null
$state | ConvertTo-Json -Depth 5 | Tee-Object -FilePath (Join-Path $ReportDirectory 'input-desktop.json')
# Actual pixels of the disposable session, not a reconstructed interface.
$bounds = [Windows.Forms.SystemInformation]::VirtualScreen
$bitmap = [Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
$graphics = [Drawing.Graphics]::FromImage($bitmap)
try {
    $graphics.CopyFromScreen($bounds.Location, [Drawing.Point]::Empty, $bounds.Size)
    $bitmap.Save([IO.Path]::GetFullPath((Join-Path $ReportDirectory 'desktop.png')), [Drawing.Imaging.ImageFormat]::Png)
} finally { $graphics.Dispose(); $bitmap.Dispose() }
