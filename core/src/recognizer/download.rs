//! Model archive download + extract.
//!
//! Mirrors what `apps/windows/OpenWhisper/Dictation/Recognizer.cs` does:
//! pull `<model>.tar.bz2` from the k2-fsa/sherpa-onnx releases, extract
//! into `~/.cache/openwhisper/models/<model>/`. Same archive consumed by
//! both Mac (this code) and Windows shells — single source of weights.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::dictation;

const MODEL_NAME: &str = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";
const MODEL_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2";
/// Read buffer for the chunked download. 1 MiB keeps the per-chunk
/// progress-emit cost trivial (~470 calls for the full ~487 MB archive)
/// while still feeling smooth in the UI.
const DOWNLOAD_CHUNK_BYTES: usize = 1 * 1024 * 1024;

pub struct ModelPaths {
    pub encoder: PathBuf,
    pub decoder: PathBuf,
    pub joiner: PathBuf,
    pub tokens: PathBuf,
}

/// Resolve (downloading + extracting if needed) the four files sherpa
/// needs to construct an OfflineRecognizer. Blocks on download; safe to
/// call multiple times — re-uses the cache directory.
pub fn ensure_model() -> Result<ModelPaths, String> {
    let cache_root = cache_root()?;
    let model_dir = cache_root.join(MODEL_NAME);

    if !model_dir.exists() {
        fs::create_dir_all(&cache_root)
            .map_err(|e| format!("create cache dir: {e}"))?;
        let archive = cache_root.join(format!("{MODEL_NAME}.tar.bz2"));
        if !archive.exists() {
            download_to(&archive)?;
        }
        extract(&archive, &cache_root)?;
        if !model_dir.exists() {
            return Err(format!(
                "model dir not present after extraction: {}",
                model_dir.display()
            ));
        }
    }

    let paths = ModelPaths {
        encoder: model_dir.join("encoder.int8.onnx"),
        decoder: model_dir.join("decoder.int8.onnx"),
        joiner: model_dir.join("joiner.int8.onnx"),
        tokens: model_dir.join("tokens.txt"),
    };
    for p in [&paths.encoder, &paths.decoder, &paths.joiner, &paths.tokens] {
        if !p.exists() {
            return Err(format!("missing model file: {}", p.display()));
        }
    }
    Ok(paths)
}

fn cache_root() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "no home dir".to_string())?;
    Ok(home.join(".cache").join("openwhisper").join("models"))
}

fn download_to(dest: &Path) -> Result<(), String> {
    eprintln!("[recognizer] downloading {MODEL_URL}");
    let resp = ureq::get(MODEL_URL)
        .call()
        .map_err(|e| format!("download GET: {e}"))?;
    // Total may be 0 if the server omits Content-Length (rare for GitHub
    // releases, but possible behind some proxies). UI handles 0 as "unknown
    // total → indeterminate progress".
    let total: u64 = resp
        .header("Content-Length")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    dictation::dictation_set_download_progress(0, total);

    let tmp = dest.with_extension("part");
    let mut out = fs::File::create(&tmp).map_err(|e| format!("create tmp: {e}"))?;
    let mut reader = resp.into_reader();
    let mut buf = vec![0u8; DOWNLOAD_CHUNK_BYTES];
    let mut done: u64 = 0;
    // Log a stdout line each time we cross a 10 % boundary. Cheap visibility
    // for operators tailing logs (CI, support, dev shell) without flooding
    // — the per-chunk dictation_set_download_progress feeds the UI bar at
    // ~470 updates over a 487 MB download, but only ~10 lines hit stderr.
    let mut next_pct_milestone: u64 = 10;
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("read archive: {e}"))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])
            .map_err(|e| format!("write archive: {e}"))?;
        done += n as u64;
        dictation::dictation_set_download_progress(done, total);
        if total > 0 {
            let pct = (done * 100) / total;
            if pct >= next_pct_milestone {
                eprintln!(
                    "[recognizer] downloading: {pct}% ({done}/{total} bytes)"
                );
                next_pct_milestone = pct + 10;
            }
        }
    }
    out.flush().map_err(|e| format!("flush archive: {e}"))?;
    drop(out);

    // Integrity guard: ureq's Read can return Ok(0) on a truncated stream
    // (HTTP/1.1 allows the server to close mid-body). Without this check a
    // partial download silently becomes a "successful" archive, extraction
    // proceeds with a corrupt encoder.onnx, and the user hits the cryptic
    // ORT load failure that motivated TASK / issue #3.
    if total > 0 && done != total {
        let _ = fs::remove_file(&tmp);
        return Err(format!(
            "download truncated: got {done} bytes, expected {total} (Content-Length)"
        ));
    }

    fs::rename(&tmp, dest).map_err(|e| format!("rename tmp: {e}"))?;
    eprintln!(
        "[recognizer] saved archive {} ({} bytes)",
        dest.display(),
        fs::metadata(dest).map(|m| m.len()).unwrap_or(0)
    );
    Ok(())
}

fn extract(archive: &Path, into: &Path) -> Result<(), String> {
    eprintln!("[recognizer] extracting {} → {}", archive.display(), into.display());
    let f = fs::File::open(archive).map_err(|e| format!("open archive: {e}"))?;
    let total = f
        .metadata()
        .map(|m| m.len())
        .map_err(|e| format!("archive metadata: {e}"))?;
    // Seed the bar at 0/total so the UI flips from "downloading 100%" to
    // "extracting 0%" instantly instead of waiting for the first bz2 chunk.
    dictation::dictation_set_extract_progress(0, total);
    let counted = ProgressReader { inner: f, done: 0, total, last_pushed: 0 };
    let bz = bzip2::read::BzDecoder::new(counted);
    let mut tar = tar::Archive::new(bz);
    tar.unpack(into).map_err(|e| format!("unpack: {e}"))?;
    // Hand off to the next phase (ORT session build). Status string updates
    // again inside ort_parakeet::ensure_loaded right before resolve_ep.
    dictation::dictation_mark_loading_session();
    Ok(())
}

/// `Read` wrapper that pushes archive-bytes-consumed into the dictation
/// snapshot as bzip2 + tar pull data through it. Bytes counted are the
/// *compressed* archive size — gives a roughly linear bar because bz2
/// decompression reads its input sequentially. The status verb is
/// "extracting model" (set by `dictation_set_extract_progress`).
///
/// Throttled to ~1 MiB increments: bz2 reads from the file in modest
/// chunks (often 32 KB), so without a throttle the dictation mutex would
/// be acquired tens of thousands of times for one extract. 1 MiB
/// granularity gives ~470 updates over a 487 MB archive — same cadence
/// as the download bar — without flooding the lock.
struct ProgressReader<R> {
    inner: R,
    done: u64,
    total: u64,
    last_pushed: u64,
}

const PROGRESS_PUSH_INTERVAL: u64 = 1 * 1024 * 1024;

impl<R: Read> Read for ProgressReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.done += n as u64;
            if self.done.saturating_sub(self.last_pushed) >= PROGRESS_PUSH_INTERVAL
                || self.done == self.total
            {
                dictation::dictation_set_extract_progress(self.done, self.total);
                self.last_pushed = self.done;
            }
        }
        Ok(n)
    }
}
