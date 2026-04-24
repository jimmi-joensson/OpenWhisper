using System.Diagnostics;
using System.Runtime.InteropServices;
using Microsoft.UI.Xaml;
using OpenWhisper.Util;

namespace OpenWhisper;

public partial class App : Application
{
    private Window? _window;

    public App()
    {
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
