import iconSrc from "../assets/icon-128.png";
import { HealthBanner } from "./health-banner";
import { TranscriptRow } from "./transcript-row";
import { useCurrentHotkey } from "../lib/use-current-hotkey";
import { useLastTranscription } from "../lib/use-last-transcription";
import { formatHotkeyLabel } from "../lib/hotkey-format";

interface HomePaneProps {
  hotkeyError?: string | null;
  onHotkeyRetry?: () => void;
  micError?: string | null;
  automationError?: string | null;
  onAutomationOpenSettings?: () => void;
  recognizerError?: string | null;
  onRecognizerRetry?: () => void;
}

export function HomePane({
  hotkeyError,
  onHotkeyRetry,
  micError,
  automationError,
  onAutomationOpenSettings,
  recognizerError,
  onRecognizerRetry,
}: HomePaneProps) {
  const toggleConfig = useCurrentHotkey("toggle");
  const chord = formatHotkeyLabel(toggleConfig);
  const latest = useLastTranscription();

  return (
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
          <HealthBanner message={micError} />
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

      {latest && <TranscriptRow text={latest.text} timestamp={latest.timestamp} />}
    </div>
  );
}
