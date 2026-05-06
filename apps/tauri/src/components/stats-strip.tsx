import { useStatsSummary } from "../lib/use-stats-summary";
import { useUserWpm } from "../lib/use-user-wpm";

/// Time saved = time the user would have spent typing the same words
/// at `wpm`, minus the time they actually spent speaking. The
/// `seconds_total` value coming from the Rust side is already
/// active-speech time (energy VAD on the drained samples — see
/// core::audio::estimate_voiced_ms), so silence at the tail of a
/// recording doesn't appear here.
function timeSavedMs(words: number, speechSeconds: number, wpm: number): number {
  const typingMs = (words / wpm) * 60_000;
  const speechMs = speechSeconds * 1000;
  return Math.max(0, typingMs - speechMs);
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
  const { wpm } = useUserWpm();

  const wordsToday = summary?.words_today ?? 0;
  const wordsWeek = summary?.words_week ?? 0;
  const wordsAllTime = summary?.words_all_time ?? 0;
  const secondsTotal = summary?.seconds_total ?? 0;

  const isEmpty = wordsAllTime === 0;
  const savedMs = timeSavedMs(wordsAllTime, secondsTotal, wpm);

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
      hint: isEmpty ? "vs. typing" : `vs. typing at ${wpm} wpm`,
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
