import AppKit
import AVFoundation
import Speech
import SwiftUI

/// On-device dictation into the search field or the chat composer.
/// Uses Apple's speech recognizer with `requiresOnDeviceRecognition` when the
/// language supports it, so audio never leaves the Mac.
@MainActor
final class Dictation {
    static let shared = Dictation()

    private(set) var active = false
    private var engine: AVAudioEngine?
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?
    private var silence: Task<Void, Never>?
    private var onText: ((String) -> Void)?
    private var onEnd: (() -> Void)?
    /// Seconds without new words before it stops.
    private var silenceSeconds: Double = 2.5
    /// Waiting for the Speech Recognition permission answer.
    private(set) var pendingAuthorization = false

    /// Start, or stop when already running. `onText` gets the running
    /// transcript; `onEnd` runs once when it stops (silence, Esc, or toggle).
    func toggle(silence: Double = 2.5, onText: @escaping (String) -> Void, onEnd: @escaping () -> Void) {
        if active {
            stop()
            return
        }
        silenceSeconds = silence
        self.onText = onText
        self.onEnd = onEnd
        switch SFSpeechRecognizer.authorizationStatus() {
        case .authorized:
            start()
        case .notDetermined:
            pendingAuthorization = true
            SFSpeechRecognizer.requestAuthorization { status in
                Task { @MainActor in
                    Dictation.shared.pendingAuthorization = false
                    if status == .authorized {
                        Dictation.shared.start()
                    } else {
                        Model.shared.permissionToast("Dictation", needs: "Speech Recognition")
                        Dictation.shared.abandon()
                    }
                }
            }
        default:
            Model.shared.permissionToast("Dictation", needs: "Speech Recognition")
            abandon()
        }
    }

    private func start() {
        guard let recognizer = SFSpeechRecognizer(), recognizer.isAvailable else {
            Model.shared.flash("Dictation is not available for this language", error: true, thenHide: false)
            abandon()
            return
        }
        let engine = AVAudioEngine()
        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        if recognizer.supportsOnDeviceRecognition {
            request.requiresOnDeviceRecognition = true
        }
        request.addsPunctuation = true
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        input.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in
            request.append(buffer)
        }
        do {
            engine.prepare()
            try engine.start()
        } catch {
            Model.shared.permissionToast("Dictation", needs: "Microphone")
            abandon()
            return
        }
        self.engine = engine
        self.request = request
        active = true
        Model.shared.dictating = true
        task = recognizer.recognitionTask(with: request) { result, error in
            let text = result?.bestTranscription.formattedString
            let final = result?.isFinal ?? false
            Task { @MainActor in
                let me = Dictation.shared
                guard me.active else { return }
                if let text { me.onText?(text) }
                me.armSilenceTimer()
                if final || error != nil { me.stop() }
            }
        }
        armSilenceTimer()
    }

    /// Stop after `silenceSeconds` without new words.
    private func armSilenceTimer() {
        silence?.cancel()
        let wait = UInt64(silenceSeconds * 1_000_000_000)
        silence = Task { @MainActor in
            try? await Task.sleep(nanoseconds: wait)
            guard !Task.isCancelled else { return }
            Dictation.shared.stop()
        }
    }

    /// Never started (no permission, no microphone): still tell the caller it ended.
    func abandon() {
        let end = onEnd
        onEnd = nil
        onText = nil
        end?()
    }

    func stop() {
        guard active else { return }
        active = false
        Model.shared.dictating = false
        silence?.cancel()
        engine?.inputNode.removeTap(onBus: 0)
        engine?.stop()
        request?.endAudio()
        task?.finish()
        engine = nil
        request = nil
        task = nil
        let end = onEnd
        onEnd = nil
        onText = nil
        end?()
    }
}

/// "Dictate Anywhere": speak into whatever app is in front. A small HUD shows
/// the words as they come; when you stop talking (or press the hotkey again)
/// the text is pasted. Esc cancels.
@MainActor
final class GlobalDictation {
    static let shared = GlobalDictation()
    private var panel: NSPanel?
    private let state = HUDState()
    private var escMonitor: Any?
    private var cancelled = false

    @MainActor
    @Observable
    final class HUDState {
        var text = ""
    }

    func toggle() {
        if Dictation.shared.active {
            Dictation.shared.stop()
            return
        }
        cancelled = false
        state.text = ""
        showHUD()
        escMonitor = NSEvent.addGlobalMonitorForEvents(matching: .keyDown) { e in
            guard e.keyCode == 53 else { return }
            MainActor.assumeIsolated {
                GlobalDictation.shared.cancelled = true
                Dictation.shared.stop()
            }
        }
        Dictation.shared.toggle(silence: 3.0, onText: { text in
            GlobalDictation.shared.state.text = text
        }, onEnd: {
            GlobalDictation.shared.finish()
        })
    }

    private func finish() {
        if let m = escMonitor { NSEvent.removeMonitor(m) }
        escMonitor = nil
        panel?.orderOut(nil)
        let text = state.text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cancelled, !text.isEmpty else { return }
        Task.detached { _ = Engine.call(["op": "paste", "text": text]) }
    }

    private func showHUD() {
        let screen = NSScreen.screens.first { $0.frame.contains(NSEvent.mouseLocation) } ?? NSScreen.main
        guard let vf = screen?.visibleFrame else { return }
        let size = NSSize(width: 420, height: 64)
        let rect = NSRect(x: vf.midX - size.width / 2, y: vf.minY + 40, width: size.width, height: size.height)
        let p = panel ?? {
            let p = NSPanel(contentRect: rect, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
            p.level = .statusBar
            p.isOpaque = false
            p.backgroundColor = .clear
            p.hasShadow = true
            p.ignoresMouseEvents = true
            p.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .transient]
            p.contentView = NSHostingView(rootView: DictationHUDView(state: state))
            return p
        }()
        p.setFrame(rect, display: true)
        p.orderFrontRegardless()
        panel = p
    }
}

struct DictationHUDView: View {
    @Bindable var state: GlobalDictation.HUDState

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "mic.fill")
                .foregroundStyle(.red)
                .symbolEffect(.pulse)
            Text(state.text.isEmpty ? "Listening… (Esc cancels)" : state.text)
                .font(.system(size: 13))
                .lineLimit(2)
                .truncationMode(.head)
                .foregroundStyle(state.text.isEmpty ? .secondary : .primary)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 16)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
    }
}
