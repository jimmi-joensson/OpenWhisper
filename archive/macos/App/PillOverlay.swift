import AppKit
import SwiftUI

/// Visible state of the dictation pill. `idle` is the resting baseline —
/// the pill stays visible so the app is always discoverable + always
/// registered with the window server (keeps us in the Force Quit dialog).
enum PillStatus {
    case idle
    case recording
    case transcribing
}

/// Observable state driving the pill's animation. Shared between the
/// `PillWindowController` (which updates it from timers + hotkey flow) and
/// the `PillView` (which renders it).
@MainActor
@Observable
final class PillState {
    var status: PillStatus = .idle
    var levels: [Float] = Array(repeating: 0, count: 12)
}

/// SwiftUI content of the pill. Borderless capsule on an HUD-style material.
/// Three visuals — idle dots at rest, level meter while recording,
/// progress spinner + meter during transcription. Tap routes through
/// `onIdleTap` while in `.idle` state so the pill doubles as a one-click
/// handle for the main UI. Other states are click-through (controller
/// toggles `ignoresMouseEvents`).
struct PillView: View {
    let state: PillState
    let onIdleTap: () -> Void

    var body: some View {
        HStack(spacing: 4) {
            switch state.status {
            case .idle:
                IdleDots()
            case .recording:
                LevelMeter(levels: state.levels, active: true)
                    .frame(height: 10)
            case .transcribing:
                ProgressView()
                    .controlSize(.mini)
                    .scaleEffect(0.55)
                    .frame(width: 8, height: 8)
                LevelMeter(levels: state.levels, active: false)
                    .frame(height: 10)
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 5)
        .background(
            Capsule()
                .fill(.black.opacity(0.55))
                .background(.ultraThinMaterial, in: Capsule())
        )
        .overlay(
            Capsule().strokeBorder(.white.opacity(0.08), lineWidth: 1)
        )
        .contentShape(Capsule())
        .onTapGesture {
            if state.status == .idle {
                onIdleTap()
            }
        }
        .onHover { hovering in
            if state.status == .idle {
                if hovering {
                    NSCursor.pointingHand.push()
                } else {
                    NSCursor.pop()
                }
            }
        }
    }
}

/// Idle indicator: three muted dots. Low chrome, signals "tap hotkey to speak"
/// without taking attention. Keeps the pill the same height as the active
/// states so the layout doesn't jump when recording starts.
private struct IdleDots: View {
    var body: some View {
        HStack(spacing: 3) {
            ForEach(0..<3, id: \.self) { _ in
                Circle()
                    .fill(.white.opacity(0.4))
                    .frame(width: 3, height: 3)
            }
        }
        .frame(height: 10)
        .padding(.horizontal, 6)
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

    private static let pillSize = CGSize(width: 70, height: 22)
    private static let gapAboveDock: CGFloat = 14

    init() {
        let hosting = NSHostingView(
            rootView: PillView(state: state) {
                AppBridge.showMainWindow()
            }
        )
        hosting.frame = NSRect(origin: .zero, size: Self.pillSize)

        let panel = NSPanel(
            contentRect: hosting.frame,
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.isFloatingPanel = true
        // `.statusBar` sits above app-specific overlays that some apps
        // (Figma's canvas tooltips, design-tool HUDs) install at the
        // `.floating` level. Same tier as NSStatusItem menus, so the pill
        // is always reachable regardless of which app currently has focus.
        panel.level = .statusBar
        panel.hidesOnDeactivate = false
        panel.backgroundColor = .clear
        panel.isOpaque = false
        panel.hasShadow = true
        // Idle = clickable (tap to open main UI). Recording/transcribing
        // flip this to true via `setClickable(false)` so clicks pass through
        // to whatever app the user is dictating into.
        panel.ignoresMouseEvents = false
        panel.isMovableByWindowBackground = false
        // Deliberately omit `.canJoinAllSpaces` / `.fullScreenAuxiliary`:
        // the pill stays on the desktop space it was created on, so
        // full-screen apps (movies, focused editors) cover it. It reappears
        // when the user leaves full-screen. Hotkey + menu bar icon still
        // work in full-screen, so dictation itself is unaffected.
        panel.collectionBehavior = [
            .stationary,
        ]
        panel.contentView = hosting

        self.panel = panel
    }

    private func setClickable(_ clickable: Bool) {
        panel.ignoresMouseEvents = !clickable
    }

    /// Bring the pill on screen in its idle resting state. Call once at
    /// app launch; subsequent state changes use `show(status:)`.
    func showIdle() {
        state.status = .idle
        setClickable(true)
        positionAboveDock()
        panel.orderFrontRegardless()
    }

    func show(status: PillStatus) {
        hideTask?.cancel()
        hideTask = nil
        state.status = status
        setClickable(status == .idle)
        positionAboveDock()
        panel.orderFrontRegardless()
    }

    func update(levels: [Float]) {
        state.levels = levels
    }

    /// Return to idle after a short grace delay so the user sees the final
    /// recording/transcribing state instead of the pill snapping back
    /// instantly. The pill stays on screen — we never orderOut, so the
    /// app keeps a visible window registered with the window server (which
    /// is what gets us into the Force Quit dialog).
    func returnToIdleAfter(delay: Duration = .milliseconds(250)) {
        hideTask?.cancel()
        hideTask = Task { @MainActor in
            try? await Task.sleep(for: delay)
            guard !Task.isCancelled else { return }
            self.state.status = .idle
            self.state.levels = Array(repeating: 0, count: self.state.levels.count)
            self.setClickable(true)
        }
    }

    func returnToIdleImmediately() {
        hideTask?.cancel()
        hideTask = nil
        state.status = .idle
        state.levels = Array(repeating: 0, count: state.levels.count)
        setClickable(true)
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
