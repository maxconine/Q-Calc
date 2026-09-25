// win32 helpers for smoke.ps1; kept to C# 5 because Windows PowerShell's Add-Type compiles with it
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

namespace QCalcE2E
{
    public static class Native
    {
        [StructLayout(LayoutKind.Sequential)]
        public struct RECT { public int Left, Top, Right, Bottom; }

        [StructLayout(LayoutKind.Sequential)]
        struct MOUSEINPUT { public int dx, dy; public uint mouseData, dwFlags, time; public IntPtr dwExtraInfo; }

        [StructLayout(LayoutKind.Sequential)]
        struct KEYBDINPUT { public ushort wVk, wScan; public uint dwFlags, time; public IntPtr dwExtraInfo; }

        [StructLayout(LayoutKind.Explicit)]
        struct INPUTUNION
        {
            [FieldOffset(0)] public MOUSEINPUT mi;
            [FieldOffset(0)] public KEYBDINPUT ki;
        }

        [StructLayout(LayoutKind.Sequential)]
        struct INPUT { public uint type; public INPUTUNION u; }

        delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

        [DllImport("user32.dll")] static extern uint SendInput(uint n, INPUT[] inputs, int size);
        [DllImport("user32.dll")] static extern short VkKeyScanW(char ch);
        [DllImport("user32.dll")] static extern uint MapVirtualKeyW(uint code, uint mapType);
        [DllImport("user32.dll")] static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll")] static extern bool EnumWindows(EnumWindowsProc cb, IntPtr lParam);
        [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowTextW(IntPtr hWnd, StringBuilder text, int max);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassNameW(IntPtr hWnd, StringBuilder text, int max);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindowW(string className, string title);
        [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
        [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
        [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hWnd);
        [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
        [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
        [DllImport("user32.dll")] static extern int GetSystemMetrics(int index);
        [DllImport("user32.dll", SetLastError = true)] static extern IntPtr OpenInputDesktop(uint flags, bool inherit, uint access);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern bool GetUserObjectInformationW(IntPtr obj, int index, StringBuilder info, int length, out int needed);
        [DllImport("user32.dll")] static extern bool CloseDesktop(IntPtr desktop);
        [DllImport("user32.dll")] static extern bool SystemParametersInfoW(uint action, uint param, out uint value, uint winIni);
        [DllImport("kernel32.dll")] static extern bool ProcessIdToSessionId(uint pid, out uint session);
        [DllImport("kernel32.dll")] static extern uint WTSGetActiveConsoleSessionId();

        const uint INPUT_MOUSE = 0, INPUT_KEYBOARD = 1;
        const uint KEYEVENTF_EXTENDEDKEY = 0x1, KEYEVENTF_KEYUP = 0x2, KEYEVENTF_UNICODE = 0x4;
        const uint MOUSEEVENTF_LEFTDOWN = 0x2, MOUSEEVENTF_LEFTUP = 0x4;
        public const ushort VK_BACK = 0x08, VK_RETURN = 0x0D, VK_SHIFT = 0x10, VK_CONTROL = 0x11, VK_MENU = 0x12,
            VK_ESCAPE = 0x1B, VK_SPACE = 0x20, VK_A = 0x41;

        static INPUT Key(ushort vk, bool up)
        {
            INPUT i = new INPUT();
            i.type = INPUT_KEYBOARD;
            i.u.ki.wVk = vk;
            i.u.ki.wScan = (ushort)MapVirtualKeyW(vk, 0);
            i.u.ki.dwFlags = up ? KEYEVENTF_KEYUP : 0;
            return i;
        }

        static void Send(List<INPUT> inputs)
        {
            INPUT[] arr = inputs.ToArray();
            uint sent = SendInput((uint)arr.Length, arr, Marshal.SizeOf(typeof(INPUT)));
            if (sent != arr.Length) throw new InvalidOperationException("SendInput sent " + sent + " of " + arr.Length + " (blocked by UIPI or no input desktop)");
        }

        // modifiers go down in order and come up in reverse, like a person pressing a chord
        public static void Chord(params ushort[] vks)
        {
            List<INPUT> inputs = new List<INPUT>();
            foreach (ushort vk in vks) inputs.Add(Key(vk, false));
            for (int k = vks.Length - 1; k >= 0; k--) inputs.Add(Key(vks[k], true));
            Send(inputs);
        }

        // real virtual keys where the layout has them, so the page sees ordinary keydowns rather than VK_PACKET
        public static void Type(string text, int delayMs)
        {
            foreach (char ch in text)
            {
                List<INPUT> inputs = new List<INPUT>();
                short scan = VkKeyScanW(ch);
                if (scan == -1)
                {
                    INPUT down = new INPUT(); down.type = INPUT_KEYBOARD; down.u.ki.wScan = ch; down.u.ki.dwFlags = KEYEVENTF_UNICODE;
                    INPUT up = down; up.u.ki.dwFlags = KEYEVENTF_UNICODE | KEYEVENTF_KEYUP;
                    inputs.Add(down); inputs.Add(up);
                }
                else
                {
                    ushort vk = (ushort)(scan & 0xff);
                    bool shift = (scan & 0x100) != 0;
                    if (shift) inputs.Add(Key(VK_SHIFT, false));
                    inputs.Add(Key(vk, false));
                    inputs.Add(Key(vk, true));
                    if (shift) inputs.Add(Key(VK_SHIFT, true));
                }
                Send(inputs);
                Thread.Sleep(delayMs);
            }
        }

        public static void Click(int x, int y)
        {
            SetCursorPos(x, y);
            Thread.Sleep(50);
            List<INPUT> inputs = new List<INPUT>();
            INPUT down = new INPUT(); down.type = INPUT_MOUSE; down.u.mi.dwFlags = MOUSEEVENTF_LEFTDOWN;
            INPUT up = new INPUT(); up.type = INPUT_MOUSE; up.u.mi.dwFlags = MOUSEEVENTF_LEFTUP;
            inputs.Add(down); inputs.Add(up);
            Send(inputs);
        }

        public static string Title(IntPtr hWnd)
        {
            StringBuilder sb = new StringBuilder(512);
            GetWindowTextW(hWnd, sb, sb.Capacity);
            return sb.ToString();
        }

        public static string ClassName(IntPtr hWnd)
        {
            StringBuilder sb = new StringBuilder(256);
            GetClassNameW(hWnd, sb, sb.Capacity);
            return sb.ToString();
        }

        public static uint ProcessOf(IntPtr hWnd)
        {
            uint pid;
            GetWindowThreadProcessId(hWnd, out pid);
            return pid;
        }

        // top-level windows of the given processes whose title starts with the prefix
        public static IntPtr[] Windows(int[] pids, string titlePrefix)
        {
            HashSet<uint> wanted = new HashSet<uint>();
            if (pids == null) return new IntPtr[0];
            foreach (int p in pids) wanted.Add((uint)p);
            List<IntPtr> found = new List<IntPtr>();
            EnumWindows(delegate (IntPtr h, IntPtr l)
            {
                if (wanted.Contains(ProcessOf(h)) && Title(h).StartsWith(titlePrefix, StringComparison.Ordinal)) found.Add(h);
                return true;
            }, IntPtr.Zero);
            return found.ToArray();
        }

        public static RECT Rect(IntPtr hWnd)
        {
            RECT r;
            GetWindowRect(hWnd, out r);
            return r;
        }

        public static Rectangle Screen()
        {
            return new Rectangle(GetSystemMetrics(76), GetSystemMetrics(77), GetSystemMetrics(78), GetSystemMetrics(79));
        }

        public static void Screenshot(string path)
        {
            Capture(path, Screen());
        }

        // a part of the screen, clipped to it
        public static void Screenshot(string path, int x, int y, int width, int height)
        {
            Capture(path, Rectangle.Intersect(new Rectangle(x, y, width, height), Screen()));
        }

        static void Capture(string path, Rectangle s)
        {
            using (Bitmap bmp = new Bitmap(Math.Max(1, s.Width), Math.Max(1, s.Height)))
            {
                using (Graphics g = Graphics.FromImage(bmp))
                {
                    g.CopyFromScreen(s.Left, s.Top, 0, 0, bmp.Size, CopyPixelOperation.SourceCopy);
                }
                bmp.Save(path, ImageFormat.Png);
            }
        }

        public static string InputDesktop()
        {
            IntPtr d = OpenInputDesktop(0, false, 0x0001);
            if (d == IntPtr.Zero) return "(none: OpenInputDesktop failed, error " + Marshal.GetLastWin32Error() + ")";
            StringBuilder sb = new StringBuilder(256);
            int needed;
            GetUserObjectInformationW(d, 2, sb, sb.Capacity * 2, out needed);
            CloseDesktop(d);
            return sb.ToString();
        }

        public static uint Session()
        {
            uint s;
            ProcessIdToSessionId((uint)System.Diagnostics.Process.GetCurrentProcess().Id, out s);
            return s;
        }

        public static uint ConsoleSession() { return WTSGetActiveConsoleSessionId(); }

        public static uint ForegroundLockTimeout()
        {
            uint v;
            SystemParametersInfoW(0x2000, 0, out v, 0);
            return v;
        }
    }
}
