using System.Diagnostics;
using System.Runtime.InteropServices;
using Microsoft.UI.Xaml;
using OpenWhisper.Util;

namespace OpenWhisper;

public partial class App : Application
{
    // Keep mutex alive for the lifetime of this process; releasing it is what
    // lets the next launch become the new primary instance.
    private static Mutex? _singleInstanceMutex;

    private Window? _window;

    public App()
    {
        // Single-instance check up front, before any heavy init. If another
        // OpenWhisper is already running we bring its main window forward and
        // bail immediately — matches macOS's `terminatePriorInstances()`
        // intent (different direction: here the newcomer yields, there the
        // newcomer wins, but the end state is "exactly one process").
        if (IsAlreadyRunning())
        {
            ActivateExistingInstance();
            Environment.Exit(0);
            return;
        }

        // Pre-load sherpa's native DLLs explicitly. Copying them to the app
        // root (the csproj target) isn't sufficient on WinUI 3 unpackaged
        // apps — the WindowsAppRuntime bootstrap manipulates the DLL search
        // path, and sherpa's managed wrapper's implicit DllImport lookup
        // sometimes fails to find `sherpa-onnx-c-api.dll` even when it's
        // sitting next to the exe. Forcing it into the loaded-module table
        // with NativeLibrary.Load(absolute path) sidesteps the search.
        PreloadNativeDll("onnxruntime");
        PreloadNativeDll("sherpa-onnx-c-api");
        PreloadNativeDll("openwhisper_core");

        InitializeComponent();

        // Surface unhandled exceptions on the UI thread. Default WinUI 3
        // behavior is to terminate silently on unhandled — terrible for
        // debugging. Log + keep running so the user sees what broke.
        UnhandledException += (_, e) =>
        {
            Debug.WriteLine($"[unhandled] {e.Exception}");
            Console.Error.WriteLine($"[unhandled] {e.Exception}");
            try
            {
                System.IO.File.AppendAllText(
                    System.IO.Path.Combine(System.IO.Path.GetTempPath(), "openwhisper-crash.log"),
                    $"{DateTime.Now:O}\n{e.Exception}\n\n");
            }
            catch { /* best effort */ }
            e.Handled = true;
        };
        AppDomain.CurrentDomain.UnhandledException += (_, e) =>
        {
            Debug.WriteLine($"[appdomain unhandled] {e.ExceptionObject}");
            Console.Error.WriteLine($"[appdomain unhandled] {e.ExceptionObject}");
        };
        TaskScheduler.UnobservedTaskException += (_, e) =>
        {
            Debug.WriteLine($"[unobserved task] {e.Exception}");
            Console.Error.WriteLine($"[unobserved task] {e.Exception}");
            e.SetObserved();
        };
    }

    protected override void OnLaunched(Microsoft.UI.Xaml.LaunchActivatedEventArgs args)
    {
        _window = new MainWindow();
        _window.Activate();
    }

    private static bool IsAlreadyRunning()
    {
        _singleInstanceMutex = new Mutex(
            initiallyOwned: true,
            name: "OpenWhisper.SingleInstance.v1",
            out bool createdNew);
        return !createdNew;
    }

    private static void ActivateExistingInstance()
    {
        // FindWindowW matches the topmost window with title "OpenWhisper".
        // Title is set on MainWindow.xaml — if that ever changes, update here.
        IntPtr hwnd = FindWindowW(lpClassName: null, lpWindowName: "OpenWhisper");
        if (hwnd == IntPtr.Zero) return;

        // If the running instance has minimized / hidden the window (tray-only
        // mode), show it before trying to raise focus.
        ShowWindow(hwnd, SW_SHOW);
        if (IsIconic(hwnd)) ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
    }

    private const int SW_SHOW = 5;
    private const int SW_RESTORE = 9;

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr FindWindowW(string? lpClassName, string? lpWindowName);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool IsIconic(IntPtr hWnd);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool SetForegroundWindow(IntPtr hWnd);

    private static void PreloadNativeDll(string name)
    {
        try
        {
            string exeDir = AppContext.BaseDirectory;
            string full = System.IO.Path.Combine(exeDir, name + ".dll");
            if (!System.IO.File.Exists(full))
            {
                SpikeLog.Log($"PreloadNativeDll: missing {full}");
                return;
            }
            var handle = NativeLibrary.Load(full);
            SpikeLog.Log($"PreloadNativeDll: loaded {name} handle=0x{handle.ToInt64():X}");
        }
        catch (Exception ex)
        {
            SpikeLog.Log($"PreloadNativeDll: {name} FAILED {ex.GetType().Name}: {ex.Message}");
        }
    }
}
