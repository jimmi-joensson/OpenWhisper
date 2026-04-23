import AppKit
import SwiftUI

/// Visible state of the dictation pill.
enum PillStatus {
    case recording
    case transcribing
}

/// Observable state driving the pill's animation. Shared between the
/// `PillWindowController` (which updates it from timers + hotkey flow) and
/// the `PillView` (which renders it).
@MainActor
@Observable
final class PillState {
    var status: PillStatus = .recording
    var levels: [Float] = Array(repeating: 0, count: 24)
}

/// SwiftUI content of the pill. Borderless capsule on an HUD-style material
/// with a mic indicator and a live level meter.
struct PillView: View {
    let state: PillState

    var body: some View {
        HStack(spacing: 10) {
            indicator
            LevelMeter(levels: state.levels, active: state.status == .recording)
                .frame(width: 120, height: 22)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(
            Capsule()
                .fill(.black.opacity(0.55))
                .background(.ultraThinMaterial, in: Capsule())
        )
        .overlay(
            Capsule().strokeBorder(.white.opacity(0.08), lineWidth: 1)
        )
    }

    @ViewBuilder
    private var indicator: some View {
        switch state.status {
        case .recording:
            Circle()
                .fill(.red)
                .frame(width: 10, height: 10)
                .overlay(
                    Circle()
                        .stroke(.red.opacity(0.35), lineWidth: 4)
                )
        case .transcribing:
            ProgressView()
                .controlSize(.small)
                .scaleEffect(0.7)
                .frame(width: 14, height: 14)
        }
    }
}

/// Owns the borderless NSPanel that hosts the pill. The panel floats above
/// normal windows (including full-screen apps), ignores mouse events so the
/// user keeps clicking through to whatever app is behind, and anchors to the
/// bottom-center of the active screen, just above the Dock.
@MainActor
final class PillWindowController {
    let state = PillState()

    private let panel: NSPanel
    private var hideTask: Task<Void, Never>?

    private static let pillSize = CGSize(width: 180, height: 44)
    private static let gapAboveDock: CGFloat = 16

    init() {
        let hosting = NSHostingView(rootView: PillView(state: state))
        hosting.frame = NSRect(origin: .zero, size: Self.pillSize)

        let panel = NSPanel(
            contentRect: hosting.frame,
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.isFloatingPanel = true
        panel.level = .floating
        panel.hidesOnDeactivate = false
        panel.backgroundColor = .clear
        panel.isOpaque = false
        panel.hasShadow = true
        panel.ignoresMouseEvents = true
        panel.isMovableByWindowBackground = false
        panel.collectionBehavior = [
            .canJoinAllSpaces,
            .fullScreenAuxiliary,
            .stationary,
        ]
        panel.contentView = hosting

        self.panel = panel
    }

    func show(status: PillStatus) {
        hideTask?.cancel()
        hideTask = nil
        state.status = status
        positionAboveDock()
        panel.orderFrontRegardless()
    }

    func update(levels: [Float]) {
        state.levels = levels
    }

    /// Hide the pill after a short grace delay so the user sees the final
    /// state instead of the pill vanishing under their cursor.
    func hideAfter(delay: Duration = .milliseconds(250)) {
        hideTask?.cancel()
        hideTask = Task { @MainActor in
            try? await Task.sleep(for: delay)
            guard !Task.isCancelled else { return }
            panel.orderOut(nil)
        }
    }

    func hideImmediately() {
        hideTask?.cancel()
        hideTask = nil
        panel.orderOut(nil)
    }

    private func positionAboveDock() {
        // `visibleFrame` already excludes the Dock and menu bar, so its
        // bottom edge is the top of the Dock (or screen bottom when the
        // Dock auto-hides).
        let screen = panel.screen ?? NSScreen.main
        guard let frame = screen?.visibleFrame else { return }

        let size = Self.pillSize
        let origin = NSPoint(
            x: frame.midX - size.width / 2,
            y: frame.minY + Self.gapAboveDock
        )
        panel.setFrame(NSRect(origin: origin, size: size), display: true)
    }
}
