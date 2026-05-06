import { useStatsSummary } from "../lib/use-stats-summary";

/// WPM hardcoded until TASK-88.3 lands the user_wpm setting. 40 wpm is
/// the average adult typist baseline (matches design + spec default).
const DEFAULT_WPM = 40;

/// "Slow but real" speech ceiling used to cap the dictation duration in
/// the time-saved formula. If the user leaves the mic on for 30 seconds
/// after saying 5 words, raw `seconds_total` would charge them for the
/// silence and Time Saved would shrink toward 0 (or even render `—` at
/// short word counts). We cap the effective dictation seconds at the
/// slowest plausible real-speech rate, so silence at the tail end of a
/// recording doesn't penalize the credit. The raw duration is still
/// stored in SQLite — we can drop the cap (or replace with VAD-based
/// active-time) without a schema change.
const SPEAKING_CEILING_WPM = 100;

function effectiveDictationSeconds(words: number, dictationSeconds: number): number {
  if (words === 0) return dictationSeconds;
  const ceilingSecs = (words / SPEAKING_CEILING_WPM) * 60;
  return Math.min(dictationSeconds, ceilingSecs);
}

function timeSavedMs(words: number, dictationSeconds: number, wpm: number): number {
  const typingMs = (words / wpm) * 60_000;
  const dictationMs = effectiveDictationSeconds(words, dictationSeconds) * 1000;
  return Math.max(0, typingMs - dictationMs);
}

function fmtMinutes(ms: number): string {
  const m = Math.round(ms / 60_000);
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60);
  const r = m % 60;
  return r === 0 ? `${h} hr` : `${h} hr ${r} min`;
}

interface StatItem {
  label: string;
  value: string;
  hint: string;
}

export function StatsStrip() {
  const { summary } = useStatsSummary();

  const wordsToday = summary?.words_today ?? 0;
  const wordsWeek = summary?.words_week ?? 0;
  const wordsAllTime = summary?.words_all_time ?? 0;
  const secondsTotal = summary?.seconds_total ?? 0;

  const isEmpty = wordsAllTime === 0;
  const savedMs = timeSavedMs(wordsAllTime, secondsTotal, DEFAULT_WPM);

  const items: StatItem[] = [
    {
      label: "Words today",
      value: isEmpty ? "0" : wordsToday.toLocaleString(),
      hint: isEmpty ? "—" : "across this device",
    },
    {
      label: "Words this week",
      value: isEmpty ? "0" : wordsWeek.toLocaleString(),
      hint: isEmpty ? "—" : "last 7 days",
    },
    {
      label: "Words all-time",
      value: isEmpty ? "0" : wordsAllTime.toLocaleString(),
      hint: "since first launch",
    },
    {
      label: "Time saved",
      value: isEmpty ? "—" : fmtMinutes(savedMs),
      hint: isEmpty ? "vs. typing" : `vs. typing at ${DEFAULT_WPM} wpm`,
    },
  ];

  return (
    <div className="ow-stats-strip" data-testid="stats-strip">
      {items.map((it, i) => (
        <div
          key={it.label}
          className="ow-stats-strip__cell"
          data-testid={`stats-cell-${i}`}
          data-empty={isEmpty || undefined}
        >
          <div className="ow-stats-strip__label">{it.label}</div>
          <div className="ow-stats-strip__value">{it.value}</div>
          <div className="ow-stats-strip__hint">{it.hint}</div>
        </div>
      ))}
    </div>
  );
}
