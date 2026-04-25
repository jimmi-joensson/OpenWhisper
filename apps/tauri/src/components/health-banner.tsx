interface HealthBannerProps {
  message: string;
  onRetry?: () => void;
  retryLabel?: string;
}

export function HealthBanner({ message, onRetry, retryLabel = "Retry" }: HealthBannerProps) {
  return (
    <div
      role="status"
      style={{
        background: "var(--info-bg)",
        border: "1px solid var(--info-border)",
        borderRadius: 8,
        padding: "10px 12px",
        display: "flex",
        alignItems: "flex-start",
        gap: 10,
        color: "var(--foreground)",
        fontSize: 12.5,
      }}
    >
      <span style={{ display: "inline-flex", alignItems: "center", height: "1.4em", flexShrink: 0 }}>
        <InfoGlyph />
      </span>
      <span style={{ flex: 1, lineHeight: 1.4 }}>{message}</span>
      {onRetry && (
        <button
          onClick={onRetry}
          style={{
            fontFamily: "var(--font-sys)",
            fontSize: 12,
            padding: "4px 10px",
            borderRadius: 6,
            border: "1px solid var(--info-border)",
            background: "transparent",
            color: "var(--info)",
            cursor: "pointer",
          }}
        >
          {retryLabel}
        </button>
      )}
    </div>
  );
}

function InfoGlyph({ size = 14 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="8" cy="8" r="7" fill="none" stroke="var(--info)" strokeWidth="1.25" />
      <rect x="7.25" y="6.75" width="1.5" height="5" fill="var(--info)" />
      <rect x="7.25" y="3.75" width="1.5" height="1.75" fill="var(--info)" />
    </svg>
  );
}
