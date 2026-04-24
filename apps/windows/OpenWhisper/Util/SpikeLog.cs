using System.Diagnostics;

namespace OpenWhisper.Util;

/// <summary>
/// Dumb line-logger for diagnosing the early Windows bring-up crashes.
///
/// Writes to <c>%TEMP%\openwhisper-trace.log</c> with every line flushed
/// immediately, so the trail survives even a native-side SEH crash that
/// kills the process outside the CLR's reach. Also mirrors to stderr and
/// <see cref="Debug"/> for IDE / dotnet run visibility.
///
/// Intended to be removed once the shell is stable — this is dev-diagnostic
/// only, not a real logging layer.
/// </summary>
internal static class SpikeLog
{
    private static readonly string LogPath = Path.Combine(Path.GetTempPath(), "openwhisper-trace.log");
    private static readonly object Gate = new();

    static SpikeLog()
    {
        try { File.WriteAllText(LogPath, $"=== {DateTime.Now:O} OpenWhisper trace start ===\n"); }
        catch { /* best effort */ }
    }

    public static void Log(string message)
    {
        string line = $"{DateTime.Now:HH:mm:ss.fff} [tid {Environment.CurrentManagedThreadId}] {message}";
        Debug.WriteLine(line);
        try { Console.Error.WriteLine(line); } catch { }
        lock (Gate)
        {
            try
            {
                using var fs = new FileStream(LogPath, FileMode.Append, FileAccess.Write, FileShare.Read);
                using var sw = new StreamWriter(fs);
                sw.WriteLine(line);
                sw.Flush();
                fs.Flush(flushToDisk: true); // force to physical disk so a crash can't lose it
            }
            catch { /* best effort */ }
        }
    }
}
