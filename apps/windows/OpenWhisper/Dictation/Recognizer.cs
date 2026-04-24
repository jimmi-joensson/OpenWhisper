using System.Diagnostics;
using System.Net.Http;
using OpenWhisper.Util;
using SherpaOnnx;

// SpikeLog alias for diagnostic breadcrumbs during Windows bring-up.

namespace OpenWhisper.Dictation;

/// <summary>
/// Wraps sherpa-onnx's OfflineRecognizer for Parakeet-TDT v3 multilingual.
/// Owns first-run model download (to a user-scope cache) and keeps the
/// recognizer warm between utterances so subsequent decodes are fast.
///
/// Matches the "host-push" architecture from docs/claude-windows-handoff.md:
/// inference lives in the shell, not the Rust core. Transcript text is
/// passed back into core via <see cref="Core.DeliverTranscript"/>.
/// </summary>
internal sealed class Recognizer : IDisposable
{
    private const string ModelName = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";
    private const string ModelUrl =
        $"https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/{ModelName}.tar.bz2";

    private readonly OfflineRecognizer _inner;

    private Recognizer(OfflineRecognizer inner)
    {
        _inner = inner;
    }

    /// <summary>
    /// Resolves (downloading if needed) and loads the Parakeet v3 model.
    /// Progress callbacks fire during download; null progress means a cached
    /// archive was reused. Load takes ~2.6 s on current hardware.
    /// </summary>
    public static async Task<Recognizer> LoadAsync(IProgress<DownloadProgress>? progress = null, CancellationToken ct = default)
    {
        SpikeLog.Log("Recognizer.LoadAsync entered");
        string cacheRoot = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.UserProfile),
            ".cache", "openwhisper", "models");
        string modelDir = Path.Combine(cacheRoot, ModelName);
        SpikeLog.Log($"Recognizer.LoadAsync modelDir={modelDir} exists={Directory.Exists(modelDir)}");

        if (!Directory.Exists(modelDir))
        {
            await DownloadAndExtractAsync(cacheRoot, modelDir, progress, ct).ConfigureAwait(false);
        }

        var encoder = PathTricks.ToShortPath(Path.Combine(modelDir, "encoder.int8.onnx"));
        var decoder = PathTricks.ToShortPath(Path.Combine(modelDir, "decoder.int8.onnx"));
        var joiner  = PathTricks.ToShortPath(Path.Combine(modelDir, "joiner.int8.onnx"));
        var tokens  = PathTricks.ToShortPath(Path.Combine(modelDir, "tokens.txt"));
        SpikeLog.Log($"Recognizer.LoadAsync short-paths resolved: encoder={encoder}");

        foreach (var p in new[] { encoder, decoder, joiner, tokens })
        {
            if (!File.Exists(p))
                throw new FileNotFoundException($"Model file missing after extraction: {p}");
        }

        var config = new OfflineRecognizerConfig();
        config.ModelConfig.Transducer.Encoder = encoder;
        config.ModelConfig.Transducer.Decoder = decoder;
        config.ModelConfig.Transducer.Joiner = joiner;
        config.ModelConfig.Tokens = tokens;
        config.ModelConfig.ModelType = "nemo_transducer";
        config.ModelConfig.NumThreads = 1;
        config.ModelConfig.Provider = "cpu";
        config.ModelConfig.Debug = 1; // verbose native-side logging while we're still bringing up the shell
        config.DecodingMethod = "greedy_search";
        SpikeLog.Log("Recognizer.LoadAsync config built, constructing OfflineRecognizer (native init ~2.6s)");

        var rec = new OfflineRecognizer(config);
        SpikeLog.Log("Recognizer.LoadAsync OfflineRecognizer constructed");
        return new Recognizer(rec);
    }

    /// <summary>
    /// Transcribe a mono 16 kHz f32 sample buffer and return the raw text
    /// + decode latency. Caller should run post-processing
    /// (<see cref="Core.ProcessTranscript"/>) before displaying or injecting.
    /// </summary>
    public TranscribeResult Transcribe(float[] samples)
    {
        var t0 = Stopwatch.GetTimestamp();
        using var stream = _inner.CreateStream();
        stream.AcceptWaveform(16_000, samples);
        _inner.Decode(stream);
        var elapsed = Stopwatch.GetElapsedTime(t0);
        return new TranscribeResult(stream.Result.Text, elapsed);
    }

    public void Dispose() => _inner.Dispose();


    private static async Task DownloadAndExtractAsync(
        string cacheRoot,
        string modelDir,
        IProgress<DownloadProgress>? progress,
        CancellationToken ct)
    {
        Directory.CreateDirectory(cacheRoot);
        var archivePath = Path.Combine(cacheRoot, Path.GetFileName(ModelUrl));

        if (!File.Exists(archivePath))
        {
            using var http = new HttpClient { Timeout = TimeSpan.FromMinutes(20) };
            using var resp = await http.GetAsync(ModelUrl, HttpCompletionOption.ResponseHeadersRead, ct).ConfigureAwait(false);
            resp.EnsureSuccessStatusCode();

            long total = resp.Content.Headers.ContentLength ?? -1;
            var tmpPath = archivePath + ".part";
            await using (var fs = File.Create(tmpPath))
            await using (var net = await resp.Content.ReadAsStreamAsync(ct).ConfigureAwait(false))
            {
                var buf = new byte[1 << 16];
                long written = 0;
                int n;
                var lastReport = Stopwatch.GetTimestamp();
                while ((n = await net.ReadAsync(buf, ct).ConfigureAwait(false)) > 0)
                {
                    await fs.WriteAsync(buf.AsMemory(0, n), ct).ConfigureAwait(false);
                    written += n;
                    if (Stopwatch.GetElapsedTime(lastReport).TotalMilliseconds >= 250)
                    {
                        lastReport = Stopwatch.GetTimestamp();
                        progress?.Report(new DownloadProgress(written, total));
                    }
                }
            }
            File.Move(tmpPath, archivePath);
            progress?.Report(new DownloadProgress(new FileInfo(archivePath).Length, total));
        }

        // Windows ships bsdtar as `tar.exe`; supports .tar.bz2 since Win10 1803.
        var psi = new ProcessStartInfo("tar.exe", $"-xf \"{archivePath}\" -C \"{cacheRoot}\"")
        {
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
        };
        using var p = Process.Start(psi) ?? throw new InvalidOperationException("tar.exe failed to start");
        var err = await p.StandardError.ReadToEndAsync(ct).ConfigureAwait(false);
        await p.WaitForExitAsync(ct).ConfigureAwait(false);
        if (p.ExitCode != 0)
            throw new InvalidOperationException($"tar failed (exit {p.ExitCode}): {err}");
        if (!Directory.Exists(modelDir))
            throw new InvalidOperationException($"model dir not found after extraction: {modelDir}");
    }
}

internal readonly record struct DownloadProgress(long BytesReceived, long TotalBytes)
{
    public double? PercentComplete => TotalBytes > 0 ? 100.0 * BytesReceived / TotalBytes : null;
}

internal readonly record struct TranscribeResult(string RawText, TimeSpan Elapsed);
