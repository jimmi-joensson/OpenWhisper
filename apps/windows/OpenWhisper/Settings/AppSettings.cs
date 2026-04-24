using System.Text.Json;
using System.Text.Json.Serialization;
using OpenWhisper.Util;

namespace OpenWhisper.Settings;

/// <summary>
/// Minimal JSON-backed settings store at <c>%LOCALAPPDATA%\OpenWhisper\settings.json</c>.
/// Intentionally tiny — no settings UI exists yet. Grow this as settings get
/// added, but default every field to the zero-config behavior so an empty /
/// missing file yields the same experience as the Mac app.
/// </summary>
public sealed class AppSettings
{
    /// <summary>
    /// When false (default), the main window is hidden from the taskbar and
    /// Alt-Tab — the tray icon is OpenWhisper's only taskbar-area presence,
    /// mirroring macOS's <c>.accessory</c> activation policy. Power users who
    /// prefer a taskbar entry can flip this on.
    /// </summary>
    [JsonPropertyName("show_in_taskbar")]
    public bool ShowInTaskbar { get; set; } = false;

    public static string DefaultPath =>
        Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "OpenWhisper",
            "settings.json");

    public static AppSettings Load()
    {
        try
        {
            var path = DefaultPath;
            if (!File.Exists(path)) return new AppSettings();
            var json = File.ReadAllText(path);
            return JsonSerializer.Deserialize<AppSettings>(json) ?? new AppSettings();
        }
        catch (Exception ex)
        {
            SpikeLog.Log($"AppSettings.Load failed: {ex}");
            return new AppSettings();
        }
    }

    public void Save()
    {
        try
        {
            var path = DefaultPath;
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            var json = JsonSerializer.Serialize(this, new JsonSerializerOptions { WriteIndented = true });
            File.WriteAllText(path, json);
        }
        catch (Exception ex)
        {
            SpikeLog.Log($"AppSettings.Save failed: {ex}");
        }
    }
}
