import SwiftUI

/// Horizontal bar meter driven by a rolling history of peak-amplitude samples
/// from `audio_current_level()`. Uses a dB curve so conversational speech
/// spans most of the meter instead of hugging the floor.
struct LevelMeter: View {
    let levels: [Float]
    let active: Bool

    // Anything quieter than `floorDb` is treated as silence; anything louder
    // than 0 dBFS fills the bar. Tuned so normal mic input visibly fills the
    // meter without clipping.
    static let floorDb: Float = -55

    var body: some View {
        GeometryReader { geo in
            let barSpacing: CGFloat = 2
            let barCount = CGFloat(levels.count)
            let barWidth = max(1, (geo.size.width - barSpacing * (barCount - 1)) / barCount)

            HStack(alignment: .center, spacing: barSpacing) {
                ForEach(Array(levels.enumerated()), id: \.offset) { _, level in
                    let scaled = CGFloat(LevelMeter.dbNormalize(level))
                    let h = max(3, scaled * geo.size.height)
                    RoundedRectangle(cornerRadius: 1.5)
                        .fill(active ? Color.openWhisperRecording : Color.secondary.opacity(0.35))
                        .frame(width: barWidth, height: h)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .center)
        }
    }

    /// Maps a linear amplitude [0, 1] to a meter fill [0, 1] using a dB curve.
    static func dbNormalize(_ amplitude: Float) -> Float {
        let db = 20 * log10f(max(amplitude, 1e-6))
        return max(0, min(1, (db - floorDb) / -floorDb))
    }
}
