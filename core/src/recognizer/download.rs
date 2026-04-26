//! Model archive download + extract.
//!
//! Mirrors what `apps/windows/OpenWhisper/Dictation/Recognizer.cs` does:
//! pull `<model>.tar.bz2` from the k2-fsa/sherpa-onnx releases, extract
//! into `~/.cache/openwhisper/models/<model>/`. Same archive consumed by
//! both Mac (this code) and Windows shells — single source of weights.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const MODEL_NAME: &str = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";
const MODEL_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2";

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
    let tmp = dest.with_extension("part");
    let mut out = fs::File::create(&tmp).map_err(|e| format!("create tmp: {e}"))?;
    let mut reader = resp.into_reader();
    io::copy(&mut reader, &mut out).map_err(|e| format!("write archive: {e}"))?;
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
    let bz = bzip2::read::BzDecoder::new(f);
    let mut tar = tar::Archive::new(bz);
    tar.unpack(into).map_err(|e| format!("unpack: {e}"))?;
    Ok(())
}
