import micUrl from "../assets/openwhisper-mic.png";

interface MicGlyphProps {
  size?: number;
  fill?: string;
  recording?: boolean;
  className?: string;
}

// Renders the OpenWhisper mic via CSS mask-image so `currentColor` theming
// works without breaking the pixel grid. WebKit anti-aliases inline SVGs at
// small sizes even with shape-rendering="crispEdges"; rasterizing once via
// the bundled PNG mask preserves the dictaphone shape on small UI surfaces
// (button affordance, window chrome) — same approach NSImage uses for the
// macOS menu bar.
export function MicGlyph({
  size = 18,
  fill = "currentColor",
  recording = false,
  className = "",
}: MicGlyphProps) {
  const color = recording ? "var(--recording)" : fill;
  return (
    <span
      aria-hidden="true"
      className={className}
      style={{
        display: "inline-block",
        width: size,
        height: size,
        flexShrink: 0,
        backgroundColor: color,
        WebkitMaskImage: `url(${micUrl})`,
        WebkitMaskSize: "contain",
        WebkitMaskRepeat: "no-repeat",
        WebkitMaskPosition: "center",
        maskImage: `url(${micUrl})`,
        maskSize: "contain",
        maskRepeat: "no-repeat",
        maskPosition: "center",
      }}
    />
  );
}
