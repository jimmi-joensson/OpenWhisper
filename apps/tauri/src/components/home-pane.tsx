import iconSrc from "../assets/icon-128.png";
import { useEffect, useState } from "react";

import { HealthBanner } from "./health-banner";
import { HOME_HISTORY_COUNT, HomeHistory } from "./home-history";
import { HomeSwitches } from "./home-switches";
import { StatsStrip } from "./stats-strip";
import { TranscriptRow } from "./transcript-row";
import { useCurrentHotkey } from "../lib/use-current-hotkey";
import { useLastTranscription } from "../lib/use-last-transcription";
import { useHistoryPage, useHistorySettings } from "../lib/use-history";
import { formatHotkeyLabel } from "../lib/hotkey-format";

// How long a finished dictation may take to show up in the history
// list before Home shows it in a row of its own instead.
const LATEST_SETTLE_MS = 1500;

interface HomePaneProps {
  hotkeyError?: string | null;
  onHotkeyRetry?: () => void;
  micError?: string | null;
  onMicOpenSettings?: () => void;
  automationError?: string | null;
  onAutomationOpenSettings?: () => void;
  recognizerError?: string | null;
  onRecognizerRetry?: () => void;
  // Open the settings behind a quick-switch card.
  onOpenSettings?: (target: "refine" | "history") => void;
  // Open the History screen.
  onOpenHistory?: () => void;
}

export function HomePane({
  hotkeyError,
  onHotkeyRetry,
  micError,
  onMicOpenSettings,
  automationError,
  onAutomationOpenSettings,
  recognizerError,
  onRecognizerRetry,
  onOpenSettings,
  onOpenHistory,
}: HomePaneProps) {
  const toggleConfig = useCurrentHotkey("toggle");
  const chord = formatHotkeyLabel(toggleConfig);
  const latest = useLastTranscription();
  const history = useHistorySettings();
  const recent = useHistoryPage("", 0, HOME_HISTORY_COUNT);
  // With history on, the dictation that just finished is the first
  // entry of the history list and is shown there, with its dictionary
  // fixes and Refine marker. The row of its own is only for a latest
  // transcript that is not in the list: history off, or not stored.
  const newest = recent.page?.entries[0];
  const latestEntryId =
    latest && newest && newest.text.trim() === latest.text.trim() ? newest.id : null;
  // The list re-reads a moment after the dictation ends. Until then
  // the entry is not there yet; wait briefly before falling back to
  // the row of its own, so it does not flash in and out.
  const [settled, setSettled] = useState(true);
  const latestAt = latest?.timestamp ?? null;
  useEffect(() => {
    if (latestAt === null) return;
    setSettled(false);
    const timer = setTimeout(() => setSettled(true), LATEST_SETTLE_MS);
    return () => clearTimeout(timer);
  }, [latestAt]);
  const historyOn = history.settings?.enabled ?? false;
  const showLatestRow = latest !== null && latestEntryId === null && (!historyOn || settled);

  return (
    <div className="ow-home-route">
      {/* Full-bleed strip sits flush against the title bar, outside
          the content column's max-width gutter. Container query in
          App.css collapses 4 → 2 → 1 columns by container width. */}
      <div className="ow-stats-strip-container">
        <StatsStrip />
      </div>

      <div className="ow-home">
        {hotkeyError && (
          <div data-testid="hotkey-banner" className="ow-home__banner">
            <HealthBanner
              message={hotkeyError}
              onRetry={onHotkeyRetry}
              retryLabel="Restart"
            />
          </div>
        )}
        {micError && (
          <div data-testid="mic-banner" className="ow-home__banner">
            <HealthBanner
              message={micError}
              onRetry={onMicOpenSettings}
              retryLabel="Open Settings"
            />
          </div>
        )}
        {automationError && (
          <div data-testid="automation-banner" className="ow-home__banner">
            <HealthBanner
              message={automationError}
              onRetry={onAutomationOpenSettings}
              retryLabel="Open Settings"
            />
          </div>
        )}
        {recognizerError && (
          <div data-testid="recognizer-banner" className="ow-home__banner">
            <HealthBanner
              message={recognizerError}
              onRetry={onRecognizerRetry}
              retryLabel="Retry"
            />
          </div>
        )}

        <section className="ow-home__hero">
          <img
            src={iconSrc}
            alt=""
            data-testid="home-app-icon"
            className="ow-home__icon"
            width={64}
            height={64}
          />
          <h1 className="ow-home__headline">Ready when you are</h1>
          <p className="ow-home__hint" data-testid="home-hotkey-hint">
            Press <kbd>{chord}</kbd> anywhere — speak — press again to stop.
          </p>
        </section>

        <HomeSwitches
          history={history.settings}
          onHistoryChange={(enabled) => history.update({ enabled })}
          onOpenSettings={onOpenSettings}
        />

        {showLatestRow && latest && (
          <TranscriptRow
            text={latest.text}
            timestamp={latest.timestamp}
            replacements={latest.replacements}
            refine={latest.refine}
          />
        )}

        {history.settings && (
          <HomeHistory
            enabled={history.settings.enabled}
            page={recent.page}
            error={recent.error}
            latest={
              latest && latestEntryId !== null
                ? { id: latestEntryId, replacements: latest.replacements, refine: latest.refine }
                : null
            }
            onSeeAll={onOpenHistory}
          />
        )}
      </div>
    </div>
  );
}
