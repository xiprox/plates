# Counts the PC's input by kind while you go through a few activities, to see whether Plates could
# tell working at the PC (typing, clicking) from idling at it (scrolling, watching), from whatever
# app is in front. It listens through Windows' raw input with RIDEV_INPUTSINK, which hands a
# background window every keyboard and mouse event
# (documented, https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-rawinputdevice).
# It counts each event by its kind and never records which key you pressed.
#   powershell -ExecutionPolicy Bypass -File probe.ps1 [-Out <report file>]
param([string]$Out = (Join-Path $PSScriptRoot "results\$(Get-Date -Format yyyy-MM-dd)-windows.txt"))
# A second run the same day keeps the first one's report.
if (Test-Path $Out) { $Out = $Out -replace '\.txt$', "-$(Get-Date -Format HHmm).txt" }

Add-Type -ReferencedAssemblies System.Windows.Forms -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Windows.Forms;

public class Counts {
    public int Typing, Navigation, Modifier, OtherKey, Wheel, Move, Button, Hid;
    public HashSet<long> TypingSeconds = new HashSet<long>();
    public HashSet<long> ButtonSeconds = new HashSet<long>();
    public HashSet<long> WheelSeconds = new HashSet<long>();
    // The pauses between one typing key and the next, which decide how long a quiet spell must
    // last before Plates could take you as done typing.
    public DateTime? LastTyping;
    public double LongestGap;
    public int GapsOver2s, GapsOver5s;

    public void Typed(DateTime at) {
        if (LastTyping.HasValue) {
            double gap = (at - LastTyping.Value).TotalSeconds;
            if (gap > LongestGap) LongestGap = gap;
            if (gap > 2) GapsOver2s++;
            if (gap > 5) GapsOver5s++;
        }
        LastTyping = at;
    }
}

public class Listener : NativeWindow {
    [StructLayout(LayoutKind.Sequential)]
    struct RAWINPUTDEVICE { public ushort UsagePage; public ushort Usage; public uint Flags; public IntPtr Target; }

    [DllImport("user32.dll", SetLastError = true)]
    static extern bool RegisterRawInputDevices(RAWINPUTDEVICE[] devices, uint count, uint size);

    [DllImport("user32.dll")]
    static extern uint GetRawInputData(IntPtr input, uint command, IntPtr data, ref uint size, uint headerSize);

    const int WM_INPUT = 0x00FF;
    const uint RID_INPUT = 0x10000003;
    const uint RIDEV_INPUTSINK = 0x00000100;

    public Counts Current;
    public DateTime Start = DateTime.Now;

    public Listener() {
        CreateParams cp = new CreateParams();
        cp.Parent = new IntPtr(-3); // HWND_MESSAGE: a window that only receives messages.
        CreateHandle(cp);
        RAWINPUTDEVICE[] devices = new RAWINPUTDEVICE[] {
            Device(1, 6),    // keyboard
            Device(1, 2),    // mouse
            Device(0x0D, 5), // precision touchpad, whose scrolling may not come as mouse wheel
        };
        if (!RegisterRawInputDevices(devices, (uint)devices.Length, (uint)Marshal.SizeOf(typeof(RAWINPUTDEVICE)))) {
            // The touchpad entry can fail on a PC without one, so try keyboard and mouse alone.
            RAWINPUTDEVICE[] two = new RAWINPUTDEVICE[] { devices[0], devices[1] };
            if (!RegisterRawInputDevices(two, 2, (uint)Marshal.SizeOf(typeof(RAWINPUTDEVICE)))) {
                throw new Exception("RegisterRawInputDevices failed: " + Marshal.GetLastWin32Error());
            }
        }
    }

    RAWINPUTDEVICE Device(ushort page, ushort usage) {
        RAWINPUTDEVICE d = new RAWINPUTDEVICE();
        d.UsagePage = page; d.Usage = usage; d.Flags = RIDEV_INPUTSINK; d.Target = Handle;
        return d;
    }

    // Letters, digits, the number pad, punctuation, space, Backspace, Enter, Tab, and Delete type;
    // arrows, Page Up and Down, Home, and End move around a page; Shift, Ctrl, Alt, and Windows
    // modify. Space also scrolls a page, which the report should be read with in mind.
    static string KindOf(int vk) {
        if ((vk >= 0x41 && vk <= 0x5A) || (vk >= 0x30 && vk <= 0x39) || (vk >= 0x60 && vk <= 0x6F)
            || (vk >= 0xBA && vk <= 0xC0) || (vk >= 0xDB && vk <= 0xDF) || vk == 0xE2
            || vk == 0x20 || vk == 0x08 || vk == 0x0D || vk == 0x09 || vk == 0x2E) return "typing";
        if (vk >= 0x21 && vk <= 0x28) return "navigation";
        if ((vk >= 0x10 && vk <= 0x12) || (vk >= 0xA0 && vk <= 0xA5) || vk == 0x5B || vk == 0x5C) return "modifier";
        return "other";
    }

    protected override void WndProc(ref Message m) {
        if (m.Msg == WM_INPUT && Current != null) {
            Read(m.LParam);
        }
        base.WndProc(ref m);
    }

    void Read(IntPtr handle) {
        uint headerSize = (uint)(8 + 2 * IntPtr.Size);
        uint size = 0;
        GetRawInputData(handle, RID_INPUT, IntPtr.Zero, ref size, headerSize);
        if (size == 0) return;
        IntPtr buffer = Marshal.AllocHGlobal((int)size);
        try {
            if (GetRawInputData(handle, RID_INPUT, buffer, ref size, headerSize) != size) return;
            int type = Marshal.ReadInt32(buffer, 0);
            long second = (long)(DateTime.Now - Start).TotalSeconds;
            IntPtr body = new IntPtr(buffer.ToInt64() + headerSize);
            if (type == 1) {
                // RAWKEYBOARD: MakeCode, Flags, Reserved, VKey. Flags bit 0 is a key's release.
                int flags = Marshal.ReadInt16(body, 2);
                int vk = Marshal.ReadInt16(body, 6) & 0xFFFF;
                if ((flags & 1) != 0) return;
                switch (KindOf(vk)) {
                    case "typing": Current.Typing++; Current.TypingSeconds.Add(second); Current.Typed(DateTime.Now); break;
                    case "navigation": Current.Navigation++; break;
                    case "modifier": Current.Modifier++; break;
                    default: Current.OtherKey++; break;
                }
            } else if (type == 0) {
                // RAWMOUSE: usFlags, then usButtonFlags at 4, then lLastX at 12 and lLastY at 16.
                int buttons = Marshal.ReadInt16(body, 4) & 0xFFFF;
                int dx = Marshal.ReadInt32(body, 12), dy = Marshal.ReadInt32(body, 16);
                if ((buttons & 0x0C00) != 0) { Current.Wheel++; Current.WheelSeconds.Add(second); }
                if ((buttons & (0x01 | 0x04 | 0x10 | 0x40 | 0x100)) != 0) { Current.Button++; Current.ButtonSeconds.Add(second); }
                if (dx != 0 || dy != 0) Current.Move++;
            } else {
                Current.Hid++;
            }
        } finally {
            Marshal.FreeHGlobal(buffer);
        }
    }
}
"@

$phases = @(
    @{ Name = 'Rest'; Seconds = 10; Ask = "Don't touch the keyboard or mouse, as while watching a video." },
    @{ Name = 'Typing'; Seconds = 20; Ask = 'Click into another app, such as a Claude message box or Notepad, and type as you normally would.' },
    @{ Name = 'Wheel scrolling'; Seconds = 20; Ask = 'Scroll a web page or a long document with the mouse wheel or the touchpad, reading as you go.' },
    @{ Name = 'Key scrolling'; Seconds = 15; Ask = 'Scroll a page with the keyboard: arrows, Page Down, or the space bar.' },
    @{ Name = 'Clicking'; Seconds = 15; Ask = 'Click around: select some text, click links or buttons, drag something.' },
    @{ Name = 'Moving'; Seconds = 10; Ask = 'Move the mouse around without clicking or scrolling.' }
)

Write-Host 'This counts your keyboard and mouse input by kind for about 90 seconds, in six parts.'
Write-Host 'It never records which keys you press, only how many of each kind.'
Write-Host ''
foreach ($p in $phases) { Write-Host ("  {0,-16} {1,3}s  {2}" -f $p.Name, $p.Seconds, $p.Ask) }
Write-Host ''
Read-Host 'Press Enter here to start, then switch to the apps as each part asks'

$listener = New-Object Listener
$results = @()
foreach ($p in $phases) {
    Write-Host ''
    Write-Host ("== {0}: {1}" -f $p.Name, $p.Ask)
    for ($i = 3; $i -ge 1; $i--) { Write-Host "   starting in $i"; $end = (Get-Date).AddSeconds(1); while ((Get-Date) -lt $end) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 10 } }
    [console]::Beep(880, 120)
    $listener.Start = Get-Date
    $listener.Current = New-Object Counts
    $end = (Get-Date).AddSeconds($p.Seconds)
    while ((Get-Date) -lt $end) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 5 }
    $c = $listener.Current
    $listener.Current = $null
    [console]::Beep(660, 120)
    # One line per part, written out by hand, since a table this wide loses its last columns to
    # Format-Table's fit to the window.
    $results += ("{0,-16} {1,3}s | typing {2,4} in {3,2}s, longest pause {4,5:N1}s, pauses over 2s {5,2}, over 5s {6,2} | navigation {7,4} | modifier {8,4} | other key {9,3} | button {10,3} in {11,2}s | wheel {12,4} in {13,2}s | move {14,5} | touchpad {15,4}" -f `
        $p.Name, $p.Seconds, $c.Typing, $c.TypingSeconds.Count, $c.LongestGap, $c.GapsOver2s, $c.GapsOver5s,
        $c.Navigation, $c.Modifier, $c.OtherKey, $c.Button, $c.ButtonSeconds.Count,
        $c.Wheel, $c.WheelSeconds.Count, $c.Move, $c.Hid)
    Write-Host '   done'
}
$listener.DestroyHandle()

$os = (Get-CimInstance Win32_OperatingSystem)
$header = @(
    'Input counted by kind through raw input, one listener in the background.',
    ("Windows: {0} build {1}, {2}" -f $os.Caption, $os.BuildNumber, (Get-Date -Format 'yyyy-MM-dd')),
    '"in Ns" counts the seconds of the part that held at least one event of that kind.',
    'A pause is the time between one typing key and the next.',
    'Touchpad counts reports from a precision touchpad, if one is registered.',
    ''
)
$report = (($header + $results) -join "`r`n") + "`r`n"
New-Item -ItemType Directory -Force (Split-Path $Out) | Out-Null
Set-Content -Path $Out -Value $report -Encoding utf8
Write-Host $report
Write-Host "Saved to $Out"
Read-Host 'Press Enter to close'
