# Window-scoped input for an explicitly isolated --ui-smoke process only.
# No global mouse/keyboard injection and no messages to another process.
if (-not ('PilotUiReviewInput' -as [type])) {
    Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class PilotUiReviewInput {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint owner);
    [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr window, ref Point point);
    [DllImport("user32.dll", SetLastError=true)] static extern bool PostMessage(IntPtr window, uint message, UIntPtr wparam, IntPtr lparam);
    static IntPtr Coordinates(int x, int y) {
        return new IntPtr(unchecked((int)(((uint)(ushort)y << 16) | (ushort)x)));
    }
    public static bool ScrollDown(IntPtr window, uint expectedOwner) {
        uint owner; Rect rect;
        if (window == IntPtr.Zero || GetWindowThreadProcessId(window, out owner) == 0 || owner != expectedOwner || !GetClientRect(window, out rect)) return false;
        var point = new Point { X = (rect.Right - rect.Left) * 7 / 10, Y = (rect.Bottom - rect.Top) * 6 / 10 };
        if (point.X <= 0 || point.Y <= 0) return false;
        if (!PostMessage(window, 0x0200, UIntPtr.Zero, Coordinates(point.X, point.Y))) return false;
        if (!ClientToScreen(window, ref point)) return false;
        // WM_MOUSEMOVE uses client coordinates; WM_MOUSEWHEEL uses signed
        // screen coordinates. Twenty downward notches stay within int16.
        uint wheel = unchecked((uint)(ushort)(short)-2400 << 16);
        return PostMessage(window, 0x020A, new UIntPtr(wheel), Coordinates(point.X, point.Y));
    }
}
'@
}
