using System.Runtime.InteropServices;

namespace OpenWhisper.Tray;

/// <summary>
/// Thin wrapper around Win32 <c>TrackPopupMenu</c> for the tray-icon context
/// menu. Uses the native menu control rather than a WinUI <c>MenuFlyout</c>
/// because the tray callback gives us screen coordinates with no XAML
/// anchor, and the native menu automatically follows the system theme +
/// accent color without extra styling.
///
/// Mirrors the macOS menubar menu in <c>apps/macos/App/OpenWhisperApp.swift</c>
/// — keep item text in sync so users recognize the same menu across shells.
/// </summary>
internal static class TrayMenu
{
    public sealed class Item
    {
        public required string Text { get; init; }
        public bool Enabled { get; init; } = true;
        public Action? Handler { get; init; }
        public bool IsSeparator { get; init; }

        public static Item Separator() => new() { Text = string.Empty, IsSeparator = true };
    }

    /// <summary>
    /// Show the menu at the given screen coordinates. Blocks the UI thread
    /// until the user picks an item or dismisses the menu. Returns after
    /// dispatching the selected item's <see cref="Item.Handler"/>.
    /// </summary>
    /// <param name="ownerHwnd">
    /// HWND that will own the menu. TrackPopupMenu requires an owning window
    /// that has been made foreground before the call, otherwise the menu
    /// doesn't dismiss on outside clicks (see MSDN's `TrackPopupMenu` remarks).
    /// </param>
    public static void Show(int screenX, int screenY, IntPtr ownerHwnd, IReadOnlyList<Item> items)
    {
        IntPtr hMenu = CreatePopupMenu();
        if (hMenu == IntPtr.Zero) return;

        try
        {
            for (int i = 0; i < items.Count; i++)
            {
                var item = items[i];
                if (item.IsSeparator)
                {
                    AppendMenu(hMenu, MF_SEPARATOR, 0, null);
                    continue;
                }
                uint flags = MF_STRING | (item.Enabled ? 0u : MF_GRAYED);
                // Menu item IDs start at 1 because 0 is reserved for "user dismissed".
                AppendMenu(hMenu, flags, (uint)(i + 1), item.Text);
            }

            // TrackPopupMenu dismissal-on-outside-click requires the owner
            // to be foreground. Without this, the menu stays open when the
            // user clicks somewhere else — a well-documented Win32 quirk.
            SetForegroundWindow(ownerHwnd);

            int selectedId = TrackPopupMenu(
                hMenu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_RIGHTALIGN,
                screenX, screenY,
                0, ownerHwnd, IntPtr.Zero);

            // Post a null message to unblock the message loop — another
            // TrackPopupMenu quirk, required for the owner to repaint cleanly.
            PostMessage(ownerHwnd, 0, IntPtr.Zero, IntPtr.Zero);

            if (selectedId > 0 && selectedId <= items.Count)
            {
                items[selectedId - 1].Handler?.Invoke();
            }
        }
        finally
        {
            DestroyMenu(hMenu);
        }
    }

    // --- P/Invoke ---

    private const uint MF_STRING = 0x00000000;
    private const uint MF_SEPARATOR = 0x00000800;
    private const uint MF_GRAYED = 0x00000001;

    private const uint TPM_RETURNCMD = 0x0100;
    private const uint TPM_RIGHTBUTTON = 0x0002;
    private const uint TPM_BOTTOMALIGN = 0x0020;
    private const uint TPM_RIGHTALIGN = 0x0008;

    [DllImport("user32.dll")]
    private static extern IntPtr CreatePopupMenu();

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool AppendMenu(IntPtr hMenu, uint uFlags, uint uIDNewItem, string? lpNewItem);

    [DllImport("user32.dll")]
    private static extern int TrackPopupMenu(
        IntPtr hMenu, uint uFlags, int x, int y, int nReserved, IntPtr hWnd, IntPtr prcRect);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool DestroyMenu(IntPtr hMenu);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool PostMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);
}
