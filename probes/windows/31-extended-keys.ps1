#Requires -Version 5.1
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

<#
    31-extended-keys.ps1 - how SendInput delivers the navigation keys `press`
    synthesizes, with and without KEYEVENTF_EXTENDEDKEY.

    Issue 513 reported that `press shift+down` moves the caret without
    selecting in a Java editor, and named the missing extended flag as the
    likely cause without confirming it. This probe measures the two halves
    that can be measured without a Java runtime:

      hook.json      what a low-level keyboard hook sees for each navigation
                     key under four input shapes: today's (no flag, scan 0),
                     the flag with scan 0, the flag decided by
                     MapVirtualKeyW(MAPVK_VK_TO_VSC_EX), and the flag forced
                     with the mapped scan code. Every injected event carries a
                     marker in dwExtraInfo and the hook swallows it, so
                     nothing reaches any window.
      delivery.json  what a stock Win32 Edit control receives and selects for
                     Shift+Down twice and Shift+End, without and with the flag.
                     Injection targets the probe's own window only: it is
                     brought forward by minimize/restore (never
                     SetForegroundWindow), the foreground and focus are read
                     before and after every batch, and a case is skipped
                     rather than injected when either check fails.

    Captures carry virtual-key codes, flags and counts only - no text.
#>

. (Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) 'common.ps1')

$probe = '31-extended-keys'
[void](Get-CaptureDir -Probe $probe)

$source = @'
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Windows.Forms;

public sealed class ExtendedKeyHookRow
{
    public string Key; public string Shape; public int MapvkVscEx;
    public int Events; public int SeenScan; public bool SeenExtended; public bool SeenInjected;
}

public sealed class ExtendedKeyDeliveryRow
{
    public string Case; public bool ExtendedFlagSent; public bool Skipped; public string SkipReason;
    public int SelectionLength; public int ArrowKeyDowns; public int ArrowKeyDownsMarkedExtended;
    public bool ShiftHeldAtEveryArrow; public int UnmarkedShiftEvents; public bool ForegroundAfter;
}

public sealed class ExtendedKeyOutcome
{
    public int NumLockToggled;
    public List<ExtendedKeyHookRow> Hook = new List<ExtendedKeyHookRow>();
    public List<ExtendedKeyDeliveryRow> Delivery = new List<ExtendedKeyDeliveryRow>();
    public List<int> ModifiersLeftDown = new List<int>();
}

public static class ExtendedKeyProbe
{
    const uint KEYEVENTF_EXTENDEDKEY = 0x0001, KEYEVENTF_KEYUP = 0x0002;
    const uint LLKHF_EXTENDED = 0x01, LLKHF_INJECTED = 0x10;
    static readonly IntPtr Marker = new IntPtr(0x4144534B);

    delegate IntPtr HookProc(int code, IntPtr wParam, IntPtr lParam);

    [StructLayout(LayoutKind.Sequential)]
    struct KBDLLHOOKSTRUCT { public uint vkCode; public uint scanCode; public uint flags; public uint time; public IntPtr dwExtraInfo; }
    [StructLayout(LayoutKind.Sequential)]
    struct KEYBDINPUT { public ushort wVk; public ushort wScan; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }
    [StructLayout(LayoutKind.Sequential)]
    struct MOUSEINPUT { public int dx; public int dy; public uint mouseData; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }
    [StructLayout(LayoutKind.Explicit)]
    struct InputUnion { [FieldOffset(0)] public MOUSEINPUT mi; [FieldOffset(0)] public KEYBDINPUT ki; }
    [StructLayout(LayoutKind.Sequential)]
    struct INPUT { public uint type; public InputUnion u; }

    [DllImport("user32.dll", SetLastError = true)] static extern IntPtr SetWindowsHookEx(int id, HookProc fn, IntPtr mod, uint thread);
    [DllImport("user32.dll")] static extern bool UnhookWindowsHookEx(IntPtr hook);
    [DllImport("user32.dll")] static extern IntPtr CallNextHookEx(IntPtr hook, int code, IntPtr wParam, IntPtr lParam);
    [DllImport("kernel32.dll")] static extern IntPtr GetModuleHandle(string name);
    [DllImport("user32.dll")] static extern uint SendInput(uint count, INPUT[] inputs, int size);
    [DllImport("user32.dll")] static extern uint MapVirtualKey(uint code, uint mapType);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern IntPtr GetFocus();
    [DllImport("user32.dll")] static extern short GetKeyState(int vk);
    [DllImport("user32.dll")] static extern short GetAsyncKeyState(int vk);
    [DllImport("user32.dll")] static extern bool ShowWindow(IntPtr hwnd, int cmd);

    sealed class Seen { public uint Vk; public uint Scan; public uint Flags; public bool Ours; }

    sealed class LoggingEdit : TextBox
    {
        public readonly List<long> ArrowDowns = new List<long>();
        public readonly List<bool> ShiftAtArrow = new List<bool>();
        public int ArrowVk;
        protected override void WndProc(ref Message m)
        {
            if (m.Msg == 0x0100 && m.WParam.ToInt64() == ArrowVk)
            {
                ArrowDowns.Add(m.LParam.ToInt64());
                ShiftAtArrow.Add((GetKeyState(0x10) & 0x8000) != 0);
            }
            base.WndProc(ref m);
        }
    }

    static readonly List<Seen> seen = new List<Seen>();
    static bool swallow;
    static IntPtr hook;
    static HookProc keepAlive;

    static IntPtr OnHook(int code, IntPtr wParam, IntPtr lParam)
    {
        if (code >= 0)
        {
            KBDLLHOOKSTRUCT s = (KBDLLHOOKSTRUCT)Marshal.PtrToStructure(lParam, typeof(KBDLLHOOKSTRUCT));
            Seen e = new Seen { Vk = s.vkCode, Scan = s.scanCode, Flags = s.flags, Ours = s.dwExtraInfo == Marker };
            seen.Add(e);
            if (swallow && e.Ours) return new IntPtr(1);
        }
        return CallNextHookEx(hook, code, wParam, lParam);
    }

    static INPUT Key(ushort vk, ushort scan, uint flags)
    {
        INPUT i = new INPUT { type = 1 };
        i.u.ki = new KEYBDINPUT { wVk = vk, wScan = scan, dwFlags = flags, dwExtraInfo = Marker };
        return i;
    }

    static void Send(List<INPUT> inputs)
    {
        SendInput((uint)inputs.Count, inputs.ToArray(), Marshal.SizeOf(typeof(INPUT)));
    }

    static void Pump(int ms)
    {
        Stopwatch clock = Stopwatch.StartNew();
        while (clock.ElapsedMilliseconds < ms) { Application.DoEvents(); System.Threading.Thread.Sleep(5); }
    }

    public static ExtendedKeyOutcome Run()
    {
        ExtendedKeyOutcome outcome = new ExtendedKeyOutcome();
        outcome.NumLockToggled = GetKeyState(0x90) & 1;
        keepAlive = OnHook;
        hook = SetWindowsHookEx(13, keepAlive, GetModuleHandle(null), 0);
        if (hook == IntPtr.Zero) throw new InvalidOperationException("WH_KEYBOARD_LL install failed: " + Marshal.GetLastWin32Error());
        try
        {
            MeasureHook(outcome);
            MeasureDelivery(outcome);
        }
        finally
        {
            UnhookWindowsHookEx(hook);
            foreach (int vk in new[] { 0x10, 0x11, 0x12, 0x28, 0x23 })
                if ((GetAsyncKeyState(vk) & 0x8000) != 0) outcome.ModifiersLeftDown.Add(vk);
        }
        return outcome;
    }

    static void MeasureHook(ExtendedKeyOutcome outcome)
    {
        string[] names = { "up", "down", "left", "right", "home", "end", "pageup", "pagedown", "forwarddelete", "a" };
        ushort[] vks = { 0x26, 0x28, 0x25, 0x27, 0x24, 0x23, 0x21, 0x22, 0x2E, 0x41 };
        swallow = true;
        for (int k = 0; k < vks.Length; k++)
        {
            uint vsc = MapVirtualKey(vks[k], 4);
            ushort mapped = (ushort)(vsc & 0xFF);
            bool mappedSaysExtended = (vsc & 0xFF00) == 0xE000;
            string[] shapes = { "no_flag_scan_0", "flag_scan_0", "flag_from_vsc_ex", "flag_forced_mapped_scan" };
            ushort[] scans = { 0, 0, mapped, mapped };
            uint[] flags = { 0, KEYEVENTF_EXTENDEDKEY, mappedSaysExtended ? KEYEVENTF_EXTENDEDKEY : 0, KEYEVENTF_EXTENDEDKEY };
            for (int s = 0; s < shapes.Length; s++)
            {
                seen.Clear();
                Send(new List<INPUT> { Key(vks[k], scans[s], flags[s]), Key(vks[k], scans[s], flags[s] | KEYEVENTF_KEYUP) });
                Pump(60);
                List<Seen> ours = seen.FindAll(e => e.Ours);
                ExtendedKeyHookRow row = new ExtendedKeyHookRow { Key = names[k], Shape = shapes[s], MapvkVscEx = (int)vsc, Events = ours.Count };
                if (ours.Count > 0)
                {
                    row.SeenScan = (int)ours[0].Scan;
                    row.SeenExtended = ours.TrueForAll(e => (e.Flags & LLKHF_EXTENDED) != 0);
                    row.SeenInjected = ours.TrueForAll(e => (e.Flags & LLKHF_INJECTED) != 0);
                }
                outcome.Hook.Add(row);
            }
        }
        swallow = false;
    }

    static void MeasureDelivery(ExtendedKeyOutcome outcome)
    {
        Form form = new Form { Text = "agent-desktop probe 31", Width = 420, Height = 260 };
        LoggingEdit edit = new LoggingEdit { Multiline = true, Dock = DockStyle.Fill, HideSelection = false };
        form.Controls.Add(edit);
        form.Show();
        ShowWindow(form.Handle, 6);
        Pump(150);
        ShowWindow(form.Handle, 9);
        Pump(300);
        string[] cases = { "shift_down_twice", "shift_down_twice", "shift_end", "shift_end" };
        ushort[] arrows = { 0x28, 0x28, 0x23, 0x23 };
        int[] repeats = { 2, 2, 1, 1 };
        bool[] extended = { false, true, false, true };
        try
        {
            for (int c = 0; c < cases.Length; c++)
            {
                ExtendedKeyDeliveryRow row = new ExtendedKeyDeliveryRow { Case = cases[c], ExtendedFlagSent = extended[c] };
                outcome.Delivery.Add(row);
                edit.Text = "aaaa\r\nbbbb\r\ncccc\r\ndddd";
                form.ActiveControl = edit;
                edit.Focus();
                edit.Select(0, 0);
                edit.ArrowVk = arrows[c];
                edit.ArrowDowns.Clear();
                edit.ShiftAtArrow.Clear();
                Pump(80);
                if (GetForegroundWindow() != form.Handle) { row.Skipped = true; row.SkipReason = "probe window not foreground"; continue; }
                if (GetFocus() != edit.Handle) { row.Skipped = true; row.SkipReason = "edit control not focused"; continue; }
                uint flag = extended[c] ? KEYEVENTF_EXTENDEDKEY : 0;
                List<INPUT> batch = new List<INPUT> { Key(0x10, 0, 0) };
                for (int r = 0; r < repeats[c]; r++)
                {
                    batch.Add(Key(arrows[c], 0, flag));
                    batch.Add(Key(arrows[c], 0, flag | KEYEVENTF_KEYUP));
                }
                batch.Add(Key(0x10, 0, KEYEVENTF_KEYUP));
                seen.Clear();
                Send(batch);
                Pump(250);
                row.ForegroundAfter = GetForegroundWindow() == form.Handle;
                row.SelectionLength = edit.SelectionLength;
                row.ArrowKeyDowns = edit.ArrowDowns.Count;
                row.ArrowKeyDownsMarkedExtended = edit.ArrowDowns.FindAll(lp => ((lp >> 24) & 1) == 1).Count;
                row.ShiftHeldAtEveryArrow = edit.ShiftAtArrow.Count > 0 && edit.ShiftAtArrow.TrueForAll(held => held);
                row.UnmarkedShiftEvents = seen.FindAll(e => !e.Ours && (e.Vk == 0x10 || e.Vk == 0xA0 || e.Vk == 0xA1)).Count;
            }
        }
        finally
        {
            form.Close();
            form.Dispose();
        }
    }
}
'@

Add-Type -AssemblyName System.Windows.Forms | Out-Null
Add-Type -TypeDefinition $source -Language CSharp -ReferencedAssemblies @('System.Windows.Forms') -ErrorAction Stop | Out-Null
$outcome = [ExtendedKeyProbe]::Run()

$hook = [ordered]@{
    numlock_toggled = $outcome.NumLockToggled
    rows            = @($outcome.Hook | ForEach-Object {
            [ordered]@{
                key           = $_.Key
                shape         = $_.Shape
                mapvk_vsc_ex  = ('0x{0:X4}' -f $_.MapvkVscEx)
                events        = $_.Events
                seen_scan     = ('0x{0:X2}' -f $_.SeenScan)
                seen_extended = $_.SeenExtended
                seen_injected = $_.SeenInjected
            }
        })
}
$delivery = [ordered]@{
    numlock_toggled     = $outcome.NumLockToggled
    modifiers_left_down = @($outcome.ModifiersLeftDown)
    rows                = @($outcome.Delivery | ForEach-Object {
            [ordered]@{
                case                            = $_.Case
                extended_flag_sent              = $_.ExtendedFlagSent
                skipped                         = $_.Skipped
                skip_reason                     = $_.SkipReason
                selection_length                = $_.SelectionLength
                arrow_keydowns                  = $_.ArrowKeyDowns
                arrow_keydowns_marked_extended  = $_.ArrowKeyDownsMarkedExtended
                shift_held_at_every_arrow       = $_.ShiftHeldAtEveryArrow
                unmarked_shift_events           = $_.UnmarkedShiftEvents
                foreground_after                = $_.ForegroundAfter
            }
        })
}

[void](Write-ProbeJson -Probe $probe -Name 'hook.json' -InputObject $hook)
[void](Write-ProbeJson -Probe $probe -Name 'delivery.json' -InputObject $delivery)

$skipped = @($outcome.Delivery | Where-Object { $_.Skipped }).Count
$status = if ($outcome.ModifiersLeftDown.Count -gt 0) { 'fail' } elseif ($skipped -gt 0) { 'skip' } else { 'ok' }
Write-ProbeResult -Probe $probe -Status $status -Message ('navigation-key delivery measured; ' + $skipped + ' delivery case(s) skipped; ' + $outcome.ModifiersLeftDown.Count + ' key(s) left down') -Data ([ordered]@{ hook = $hook; delivery = $delivery })
