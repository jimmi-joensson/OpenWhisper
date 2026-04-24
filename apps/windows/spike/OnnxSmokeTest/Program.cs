// Spike: load sherpa-onnx Parakeet-TDT v3 (multilingual) and transcribe a WAV.
//
// Throwaway validation of the Windows ASR path before committing to the full
// WinUI 3 shell. Model weights are downloaded to a user-scope cache the first
// time this runs. This mirrors the ship-time behavior (download on first run)
// so the spike is a faithful-enough rehearsal.
//
// Success criteria: transcribes samples/smoke-test.wav to something close
// to the macOS reference on the same input, without crashing, within a few
// seconds on CPU. DirectML EP attempt comes later — sherpa-onnx x64 runtime
// ships CPU-only; DirectML requires a separate build path.

using System.Diagnostics;
using System.Net.Http;
using System.Runtime.InteropServices;
using System.Text;
using SherpaOnnx;

const string ModelName = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";
const string ModelUrl =
    $"https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/{ModelName}.tar.bz2";

string wavPath = args.Length > 0
    ? args[0]
    : FindRepoFile("apps", "macos", "Resources", "samples", "smoke-test.wav");

string cacheRoot = Path.Combine(
    Environment.GetFolderPath(Environment.SpecialFolder.UserProfile),
    ".cache", "openwhisper", "models");
string modelDir = Path.Combine(cacheRoot, ModelName);

if (!Directory.Exists(modelDir))
{
    await DownloadAndExtractAsync(ModelUrl, cacheRoot, modelDir);
}

// sherpa-onnx C# bindings marshal paths as ANSI but the native side reads UTF-8.
// Non-ASCII chars in the user profile (e.g. the ø in "JimmiJønsson") get mangled
// into invalid UTF-8 and the model fails to open. Workaround: collapse paths to
// their 8.3 short-name form, which is pure ASCII.
var encoder = ToShortPath(Path.Combine(modelDir, "encoder.int8.onnx"));
var decoder = ToShortPath(Path.Combine(modelDir, "decoder.int8.onnx"));
var joiner  = ToShortPath(Path.Combine(modelDir, "joiner.int8.onnx"));
var tokens  = ToShortPath(Path.Combine(modelDir, "tokens.txt"));

foreach (var path in new[] { encoder, decoder, joiner, tokens })
{
    if (!File.Exists(path))
    {
        Console.Error.WriteLine($"ERROR: missing model file: {path}");
        return 1;
    }
}

Console.WriteLine($"[spike] model dir : {modelDir}");
Console.WriteLine($"[spike] wav       : {wavPath}");

var config = new OfflineRecognizerConfig();
config.ModelConfig.Transducer.Encoder = encoder;
config.ModelConfig.Transducer.Decoder = decoder;
config.ModelConfig.Transducer.Joiner = joiner;
config.ModelConfig.Tokens = tokens;
config.ModelConfig.ModelType = "nemo_transducer";
config.ModelConfig.NumThreads = 1;
config.ModelConfig.Debug = 0;
config.ModelConfig.Provider = "cpu";
config.DecodingMethod = "greedy_search";

var tLoadStart = Stopwatch.GetTimestamp();
using var recognizer = new OfflineRecognizer(config);
var loadMs = Stopwatch.GetElapsedTime(tLoadStart).TotalMilliseconds;
Console.WriteLine($"[spike] model loaded in {loadMs:F0} ms");

var (samples, sampleRate) = ReadWav16BitMono(wavPath);
Console.WriteLine($"[spike] audio: {samples.Length} samples @ {sampleRate} Hz ({samples.Length / (double)sampleRate:F2} s)");

var tInferStart = Stopwatch.GetTimestamp();
using var stream = recognizer.CreateStream();
stream.AcceptWaveform(sampleRate, samples);
recognizer.Decode(stream);
var inferMs = Stopwatch.GetElapsedTime(tInferStart).TotalMilliseconds;

var result = stream.Result;
Console.WriteLine($"[spike] decoded in {inferMs:F0} ms ({(samples.Length / (double)sampleRate) * 1000 / inferMs:F1}x realtime)");
Console.WriteLine($"[spike] text     : \"{result.Text}\"");
if (result.Tokens is { Length: > 0 })
{
    Console.WriteLine($"[spike] tokens   : [{string.Join(", ", result.Tokens)}]");
}

return 0;


static async Task DownloadAndExtractAsync(string url, string cacheRoot, string modelDir)
{
    Directory.CreateDirectory(cacheRoot);
    var archivePath = Path.Combine(cacheRoot, Path.GetFileName(url));

    if (!File.Exists(archivePath))
    {
        Console.WriteLine($"[spike] downloading {url}");
        using var http = new HttpClient { Timeout = TimeSpan.FromMinutes(15) };
        using var resp = await http.GetAsync(url, HttpCompletionOption.ResponseHeadersRead);
        resp.EnsureSuccessStatusCode();

        var total = resp.Content.Headers.ContentLength ?? -1;
        var tmpPath = archivePath + ".part";
        await using (var fs = File.Create(tmpPath))
        await using (var net = await resp.Content.ReadAsStreamAsync())
        {
            var buf = new byte[1 << 16];
            long written = 0;
            var lastReport = Stopwatch.GetTimestamp();
            int n;
            while ((n = await net.ReadAsync(buf)) > 0)
            {
                await fs.WriteAsync(buf.AsMemory(0, n));
                written += n;
                if (Stopwatch.GetElapsedTime(lastReport).TotalSeconds >= 1)
                {
                    lastReport = Stopwatch.GetTimestamp();
                    if (total > 0)
                    {
                        Console.Write($"\r[spike] downloaded {written / 1_048_576.0:F1} / {total / 1_048_576.0:F1} MB ({100.0 * written / total:F1}%)  ");
                    }
                    else
                    {
                        Console.Write($"\r[spike] downloaded {written / 1_048_576.0:F1} MB  ");
                    }
                }
            }
        }
        File.Move(tmpPath, archivePath);
        Console.WriteLine();
    }
    else
    {
        Console.WriteLine($"[spike] archive already present: {archivePath}");
    }

    Console.WriteLine($"[spike] extracting to {cacheRoot}");
    // Windows ships bsdtar as `tar.exe`; supports .tar.bz2 natively since Win10 1803.
    var psi = new ProcessStartInfo("tar.exe", $"-xf \"{archivePath}\" -C \"{cacheRoot}\"")
    {
        RedirectStandardError = true,
        UseShellExecute = false,
    };
    using var p = Process.Start(psi)!;
    var err = await p.StandardError.ReadToEndAsync();
    await p.WaitForExitAsync();
    if (p.ExitCode != 0)
    {
        throw new InvalidOperationException($"tar failed (exit {p.ExitCode}): {err}");
    }
    if (!Directory.Exists(modelDir))
    {
        throw new InvalidOperationException($"extraction finished but model dir not found: {modelDir}");
    }
}


static (float[] samples, int sampleRate) ReadWav16BitMono(string path)
{
    // Minimal 16-bit PCM WAV reader. The macOS Resources/samples/smoke-test.wav
    // was captured by OpenWhisper itself (16 kHz mono s16le) so we don't need
    // a general-purpose decoder yet. Fail loudly on anything else.
    using var fs = File.OpenRead(path);
    using var br = new BinaryReader(fs);

    if (new string(br.ReadChars(4)) != "RIFF") throw new InvalidDataException("not a RIFF file");
    br.ReadUInt32(); // file size
    if (new string(br.ReadChars(4)) != "WAVE") throw new InvalidDataException("not a WAVE file");

    short audioFormat = 0;
    short numChannels = 0;
    int sampleRate = 0;
    short bitsPerSample = 0;
    byte[]? dataBytes = null;

    while (fs.Position < fs.Length)
    {
        var id = new string(br.ReadChars(4));
        var size = br.ReadUInt32();
        if (id == "fmt ")
        {
            audioFormat = br.ReadInt16();
            numChannels = br.ReadInt16();
            sampleRate = br.ReadInt32();
            br.ReadInt32();              // byteRate
            br.ReadInt16();              // blockAlign
            bitsPerSample = br.ReadInt16();
            if (size > 16) br.ReadBytes((int)size - 16); // skip any extension
        }
        else if (id == "data")
        {
            dataBytes = br.ReadBytes((int)size);
        }
        else
        {
            br.ReadBytes((int)size);
        }
    }

    if (audioFormat != 1) throw new NotSupportedException($"only PCM supported (got format {audioFormat})");
    if (bitsPerSample != 16) throw new NotSupportedException($"only 16-bit supported (got {bitsPerSample})");
    if (dataBytes is null) throw new InvalidDataException("no data chunk found");

    int frameCount = dataBytes.Length / 2 / numChannels;
    var samples = new float[frameCount];
    int si = 0;
    for (int f = 0; f < frameCount; f++)
    {
        int acc = 0;
        for (int c = 0; c < numChannels; c++)
        {
            short s = (short)(dataBytes[si] | (dataBytes[si + 1] << 8));
            si += 2;
            acc += s;
        }
        samples[f] = acc / (numChannels * 32768.0f);
    }
    return (samples, sampleRate);
}


static string ToShortPath(string longPath)
{
    if (!OperatingSystem.IsWindows()) return longPath;
    var buf = new StringBuilder(260);
    int len = GetShortPathNameW(longPath, buf, buf.Capacity);
    if (len == 0 || len > buf.Capacity)
    {
        // Short-name generation can be disabled per-volume. If so, fall back to
        // the original path — callers will surface any downstream encoding failure.
        return longPath;
    }
    return buf.ToString();
}

[DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
static extern int GetShortPathNameW(string lpszLongPath, StringBuilder lpszShortPath, int cchBuffer);


static string FindRepoFile(params string[] relativeSegments)
{
    // Walk up from AppContext.BaseDirectory to find the repo root (identified
    // by the Cargo.toml at the workspace root), then join the relative path.
    var dir = new DirectoryInfo(AppContext.BaseDirectory);
    while (dir is not null)
    {
        if (File.Exists(Path.Combine(dir.FullName, "Cargo.toml")))
        {
            return Path.Combine(new[] { dir.FullName }.Concat(relativeSegments).ToArray());
        }
        dir = dir.Parent;
    }
    throw new InvalidOperationException("could not locate repo root (no Cargo.toml ancestor)");
}
