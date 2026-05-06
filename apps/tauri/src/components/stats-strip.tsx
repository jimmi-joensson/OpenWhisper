import { Card, CardContent, CardHeader, CardTitle } from "./ui/card";
import { useStatsSummary } from "../lib/use-stats-summary";

/// WPM hardcoded until TASK-88.3 lands the user_wpm setting. The Time
/// Saved math is "time the user would have spent typing the same words
/// at this WPM, minus the seconds they actually spent dictating."
/// 40 wpm is the average adult typist baseline and matches the spec
/// default.
const DEFAULT_WPM = 40;

function timeSavedSeconds(words: number, dictationSeconds: number, wpm: number): number {
  return Math.max(0, (words / wpm) * 60 - dictationSeconds);
}

function formatTimeSaved(secs: number): string {
  if (secs < 1) return "—";
  if (secs < 60) return `${Math.round(secs)} sec`;
  const mins = secs / 60;
  if (mins < 60) return mins < 10 ? `${mins.toFixed(1)} min` : `${Math.round(mins)} min`;
  const hrs = mins / 60;
  return hrs < 10 ? `${hrs.toFixed(1)} hrs` : `${Math.round(hrs)} hrs`;
}

interface StatCardProps {
  label: string;
  value: string;
  subcaption: React.ReactNode;
  testId: string;
}

function StatCard({ label, value, subcaption, testId }: StatCardProps) {
  return (
    <Card size="sm" data-testid={testId}>
      <CardHeader>
        <CardTitle className="text-xs font-normal tracking-wide text-muted-foreground uppercase">
          {label}
        </CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-1">
        <span className="text-2xl font-medium tabular-nums">{value}</span>
        <span className="text-xs text-muted-foreground">{subcaption}</span>
      </CardContent>
    </Card>
  );
}

export function StatsStrip() {
  const { summary } = useStatsSummary();

  const wordsToday = summary?.words_today ?? 0;
  const wordsWeek = summary?.words_week ?? 0;
  const wordsAllTime = summary?.words_all_time ?? 0;
  const secondsTotal = summary?.seconds_total ?? 0;

  const isEmpty = wordsAllTime === 0;
  const savedSecs = timeSavedSeconds(wordsAllTime, secondsTotal, DEFAULT_WPM);
  const timeSavedValue = isEmpty ? "—" : formatTimeSaved(savedSecs);

  return (
    <section
      className="ow-home__stats grid grid-cols-4 gap-3"
      data-testid="stats-strip"
    >
      <StatCard
        label="Today"
        value={wordsToday.toLocaleString()}
        subcaption="words dictated"
        testId="stats-card-today"
      />
      <StatCard
        label="This week"
        value={wordsWeek.toLocaleString()}
        subcaption="words dictated"
        testId="stats-card-week"
      />
      <StatCard
        label="All time"
        value={wordsAllTime.toLocaleString()}
        subcaption="words dictated"
        testId="stats-card-all-time"
      />
      <StatCard
        label="Time saved"
        value={timeSavedValue}
        subcaption="vs. typing"
        testId="stats-card-time-saved"
      />
    </section>
  );
}
