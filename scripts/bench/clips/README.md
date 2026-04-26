# Bench clips

Two curated clips drive the recognizer bench in TASK-33:

- `en.wav` — ~30 s English dictation, 16 kHz mono PCM
- `en.ref.txt` — hand-typed reference transcript (ground truth)
- `da.wav` — ~30 s Danish dictation, 16 kHz mono PCM
- `da.ref.txt` — hand-typed reference transcript

## Recording recipe

To re-record (mic = same as your daily setup so the bench reflects real use):

```sh
ffmpeg -f avfoundation -i ":default" -ac 1 -ar 16000 -sample_fmt s16 -t 30 en.wav
```

Or use Voice Memos / QuickTime, then convert:

```sh
ffmpeg -i input.m4a -ac 1 -ar 16000 -sample_fmt s16 en.wav
```

Verify format:

```sh
afinfo en.wav   # expect: 1 ch, 16000 Hz, Int16
```

## Reference transcript guidance

- Type what you actually said, not what you meant to say.
- Lowercase doesn't matter — the bench tokenizer lowercases both sides.
- Strip punctuation: the tokenizer also strips `[.,!?;:'"()]` from both sides.
- Numbers spelled out vs digits: pick one and stick to it. "twenty three" vs "23"
  bias the WER number. Match what Parakeet typically emits (it spells out
  small integers, uses digits for years/IDs).
- Hesitations ("um", "uh"): include if you said them — both engines may
  emit or skip them, and that's part of the comparison.

## Files are not committed if recordings contain PII

`clips/` is gitignored at the directory level. Recordings stay local.
Reference transcripts also stay local. The decision doc records the WER
numbers, not the audio.
