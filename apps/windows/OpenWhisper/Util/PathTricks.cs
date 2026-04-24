using System.Runtime.InteropServices;
using System.Text;

namespace OpenWhisper.Util;

/// <summary>
/// Windows path helpers for interop with native libraries that read paths as UTF-8.
///
/// sherpa-onnx's C# bindings (and any other native dep that uses
/// <c>[MarshalAs(UnmanagedType.LPStr)]</c> on a path) corrupt non-ASCII
/// characters on this profile — Jimmi's username contains a Danish <c>ø</c>
/// and paths get mangled into invalid UTF-8 before the native side reads
/// them. Collapsing to the 8.3 short form guarantees pure ASCII.
/// </summary>
internal static class PathTricks
{
    public static string ToShortPath(string longPath)
    {
        if (!OperatingSystem.IsWindows()) return longPath;
        var buf = new StringBuilder(260);
        int len = GetShortPathNameW(longPath, buf, buf.Capacity);
        // Volumes with 8.3 generation disabled return 0; fall back so the
        // caller still sees the long path and surfaces any downstream error.
        return (len == 0 || len > buf.Capacity) ? longPath : buf.ToString();
    }

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern int GetShortPathNameW(string lpszLongPath, StringBuilder lpszShortPath, int cchBuffer);
}
