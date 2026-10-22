//! Silence-aligned chunking for long recordings.
//!
//! Parakeet's encoder attends over the whole input, so memory grows
//! with length on the ORT path, and one long `recognizer_transcribe`
//! call would hold the engine lock away from live dictation. Splitting
//! into ≤ `MAX_CHUNK_SECS` pieces at the quietest point near each
//! boundary keeps both bounded without cutting words in half.

use std::ops::Range;

use super::TARGET_SAMPLE_RATE;

/// Upper bound on one chunk. Parakeet-TDT was trained on utterances in
/// this range; quality is flat up to here.
pub const MAX_CHUNK_SECS: usize = 30;

/// How far back from the hard limit we search for a quiet cut point.
/// Chunks therefore land between 22 s and 30 s, except the last one.
const SEARCH_SECS: usize = 8;

/// Energy window for the quiet-point search (20 ms).
const FRAME: usize = TARGET_SAMPLE_RATE as usize / 50;

/// Split `samples` (16 kHz mono) into contiguous ranges covering the
/// whole buffer, each at most `MAX_CHUNK_SECS` long.
pub fn split_points(samples: &[f32]) -> Vec<Range<usize>> {
    let max = MAX_CHUNK_SECS * TARGET_SAMPLE_RATE as usize;
    let search = SEARCH_SECS * TARGET_SAMPLE_RATE as usize;
    let mut out = Vec::new();
    let mut start = 0;
    while samples.len() - start > max {
        let window = start + max - search..start + max;
        let cut = quietest_frame(&samples[window.clone()]) + window.start;
        out.push(start..cut);
        start = cut;
    }
    if start < samples.len() || out.is_empty() {
        out.push(start..samples.len());
    }
    out
}

/// Offset (into `window`) of the centre of the lowest-energy frame.
fn quietest_frame(window: &[f32]) -> usize {
    window
        .chunks(FRAME)
        .enumerate()
        .map(|(i, f)| (i, f.iter().map(|s| s * s).sum::<f32>()))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| (i * FRAME + FRAME / 2).min(window.len()))
        .unwrap_or(window.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: usize = TARGET_SAMPLE_RATE as usize;

    #[test]
    fn short_audio_is_one_chunk() {
        let s = vec![0.1; 10 * SR];
        assert_eq!(split_points(&s), vec![0..10 * SR]);
    }

    #[test]
    fn empty_audio_is_one_empty_chunk() {
        assert_eq!(split_points(&[]), vec![0..0]);
    }

    #[test]
    fn chunks_cover_everything_and_respect_max() {
        let s = vec![0.3; 95 * SR];
        let chunks = split_points(&s);
        assert_eq!(chunks.first().unwrap().start, 0);
        assert_eq!(chunks.last().unwrap().end, s.len());
        for pair in chunks.windows(2) {
            assert_eq!(pair[0].end, pair[1].start);
        }
        for c in &chunks {
            assert!(c.len() <= MAX_CHUNK_SECS * SR);
        }
    }

    #[test]
    fn cut_lands_in_the_silence() {
        // Loud everywhere except a 200 ms gap at 25 s.
        let mut s = vec![0.5; 40 * SR];
        let gap = 25 * SR..25 * SR + SR / 5;
        s[gap.clone()].fill(0.0);
        let chunks = split_points(&s);
        assert_eq!(chunks.len(), 2);
        assert!(gap.contains(&chunks[0].end), "cut at {}", chunks[0].end);
    }
}
