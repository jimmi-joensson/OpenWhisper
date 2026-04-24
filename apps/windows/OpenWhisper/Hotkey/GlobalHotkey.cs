using System.Runtime.InteropServices;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;
using WinRT.Interop;

namespace OpenWhisper.Hotkey;

/// <summary>
/// Win32 global hotkey registration via <c>RegisterHotKey</c>.
///
/// Default binding for OpenWhisper on Windows: <c>Left Ctrl + Space</c>.
/// Registration is per-window; we attach to the main window's HWND so the
/// hotkey goes away automatically when the app exits. We subclass the
/// window procedure via <c>SetWindowSubclass</c> (comctl32) to intercept
/// WM_HOTKEY without fighting the WinAppSDK message pump.
/// </summary>
internal sealed class GlobalHotkey : IDisposable
{
    // Modifier flags for RegisterHotKey (winuser.h).
    private const uint MOD_ALT = 0x0001;
    private const uint MOD_CONTROL = 0x0002;
    private const uint MOD_SHIFT = 0x0004;
    private const uint MOD_WIN = 0x0008;
    private const uint MOD_NOREPEAT = 0x4000;

    // VK_SPACE
    private const uint VK_SPACE = 0x20;

    private const int WM_HOTKEY = 0x0312;

    // Arbitrary ID unique within the window; we only have one hotkey.
    private const int HotkeyId = 0xBEE5;

    private readonly IntPtr _hwnd;
    private readonly SUBCLASSPROC _proc; // kept alive — the GC must not collect this
    private bool _registered;
    private bool _subclassed;

    public event EventHandler? Pressed;

    public GlobalHotkey(Window window)
    {
        _hwnd = WindowNative.GetWindowHandle(window);
        _proc = WndProc;
    }

    /// <summary>
    /// Register Ctrl + Space as a system-wide hotkey. Returns false if
    /// another app already owns that combination; caller should surface.
    /// RegisterHotKey doesn't distinguish left/right Ctrl — the MOD_CONTROL
    /// flag binds both, and that's the intended behavior on Windows.
    ///
    /// Deliberate platform-convention choice: Windows = Ctrl+Space chord
    /// (idiomatic Windows, zero hook complexity, no AV friction). Mac =
    /// Right Command tap-not-hold. Don't port Mac's semantics here.
    /// </summary>
    public bool Register()
    {
        if (_registered) return true;

        if (!_subclassed)
        {
            if (!SetWindowSubclass(_hwnd, _proc, HotkeyId, 0))
            {
                return false;
            }
            _subclassed = true;
        }

        if (!RegisterHotKey(_hwnd, HotkeyId, MOD_CONTROL | MOD_NOREPEAT, VK_SPACE))
        {
            // Keep the subclass installed — it's cheap and lets the next
            // Register() retry succeed quickly. Subclass is only torn down
            // in Dispose.
            return false;
        }
        _registered = true;
        return true;
    }

    /// <summary>
    /// Stop listening for the hotkey without tearing down subclass state.
    /// Designed for transient disable scenarios — the primary one being a
    /// fullscreen app coming to the foreground, where we deliberately
    /// release Ctrl+Space so the fullscreen app can consume it and the
    /// user can't accidentally trigger dictation.
    /// </summary>
    public void Unregister()
    {
        if (!_registered) return;
        UnregisterHotKey(_hwnd, HotkeyId);
        _registered = false;
    }

    private IntPtr WndProc(IntPtr hwnd, uint msg, UIntPtr wParam, IntPtr lParam, UIntPtr uIdSubclass, IntPtr dwRefData)
    {
        if (msg == WM_HOTKEY && wParam == (UIntPtr)HotkeyId)
        {
            Pressed?.Invoke(this, EventArgs.Empty);
            return IntPtr.Zero;
        }
        return DefSubclassProc(hwnd, msg, wParam, lParam);
    }

    public void Dispose()
    {
        if (_registered)
        {
            UnregisterHotKey(_hwnd, HotkeyId);
            _registered = false;
        }
        if (_subclassed)
        {
            RemoveWindowSubclass(_hwnd, _proc, HotkeyId);
            _subclassed = false;
        }
    }

    // --- Win32 P/Invoke ---

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool RegisterHotKey(IntPtr hWnd, int id, uint fsModifiers, uint vk);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool UnregisterHotKey(IntPtr hWnd, int id);

    private delegate IntPtr SUBCLASSPROC(IntPtr hWnd, uint uMsg, UIntPtr wParam, IntPtr lParam, UIntPtr uIdSubclass, IntPtr dwRefData);

    [DllImport("comctl32.dll", SetLastError = true)]
    private static extern bool SetWindowSubclass(IntPtr hWnd, SUBCLASSPROC pfnSubclass, uint uIdSubclass, UIntPtr dwRefData);

    [DllImport("comctl32.dll", SetLastError = true)]
    private static extern bool RemoveWindowSubclass(IntPtr hWnd, SUBCLASSPROC pfnSubclass, uint uIdSubclass);

    [DllImport("comctl32.dll")]
    private static extern IntPtr DefSubclassProc(IntPtr hWnd, uint uMsg, UIntPtr wParam, IntPtr lParam);
}
