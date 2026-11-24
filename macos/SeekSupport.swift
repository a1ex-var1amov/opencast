import AppKit
import CoreGraphics

// ponytail: Seek helpers peeled from OpenCastApp Model — move-only, no behavior change.
extension Model {
    func startClick() {
        guard !clicking, !busy else { return }
        clicking = true
        seekQuery = ""
        seekIndex = 0
        noteContentChanged()
        focusTick += 1
        runSeekScan()
    }

    /// Accessibility labels on the display under the cursor (see SeekAX).
    func runSeekScan() {
        busy = true
        seekRun += 1
        let run = seekRun
        DispatchQueue.global(qos: .userInitiated).async {
            let t0 = Date()
            let outcome: Result<SeekScan, Error>
            do {
                guard PermissionAccess.accessibilityWorks() else { throw SeekAX.Failure.accessibility }
                outcome = .success(try SeekAX.scan())
            } catch { outcome = .failure(error) }
            let ms = Int(-t0.timeIntervalSinceNow * 1000)
            DispatchQueue.main.async {
                let model = Model.shared
                guard run == model.seekRun else { return }
                model.busy = false
                // Esc or the hotkey closed the session while the scan ran. Showing it now
                // would bring the overlay back over an app that has taken focus.
                guard model.clicking else { return }
                switch outcome {
                case .success(let scan):
                    Diagnostics.log("seek \(scan.hits.count) hits from \(scan.visited) nodes in \(ms) ms")
                    model.presentSeek(scan)
                case .failure(let error):
                    Diagnostics.log("seek failed after \(ms) ms: \(error)", error: true)
                    model.cancelClick()
                    if let fail = error as? SeekAX.Failure, case .accessibility = fail {
                        model.permissionToast("Click Text", needs: "Accessibility")
                    } else {
                        model.flash(error.localizedDescription, error: true, thenHide: false)
                    }
                }
            }
        }
    }

    func cancelClick() {
        clicking = false
        seekScan = nil
        seekQuery = ""
        seekIndex = 0
        seek?.orderOut(nil)
        noteContentChanged()
        focusTick += 1
    }

    var seekSpans: [SeekSpan] {
        guard let seekScan else { return [] }
        return matchSpans(scan: seekScan, query: seekQuery)
    }

    var seekCursor: Int {
        let n = seekSpans.count
        guard n > 0 else { return 0 }
        return ((seekIndex % n) + n) % n
    }

    var seekCount: String {
        let n = seekSpans.count
        if n == 0 {
            return seekQuery.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "" : "0 of 0"
        }
        return "\(seekCursor + 1) of \(n)"
    }

    func presentSeek(_ scan: SeekScan) {
        guard let seek else { return }
        clicking = true
        seekScan = scan
        let anchor = NSScreen.screens.max(by: { $0.frame.maxY < $1.frame.maxY }) ?? NSScreen.main!
        let x = scan.monitor.x
        let y = anchor.frame.maxY - scan.monitor.y - scan.monitor.h
        seek.setFrame(NSRect(x: x, y: y, width: scan.monitor.w, height: scan.monitor.h), display: true)
        // The line starts under the search field when it is on this display.
        if let f = launcher?.frame, f.midX >= x, f.midX <= x + scan.monitor.w, f.minY >= y, f.minY <= y + scan.monitor.h {
            seekLeaderFrom = CGPoint(x: f.midX - x, y: y + scan.monitor.h - f.minY)
        } else {
            seekLeaderFrom = nil
        }
        seek.alphaValue = 1
        // Tahoe: plain orderFront is a no-op while accessory; clear backing drops fills.
        precondition(OverlayPanelPolicy.orderFrontRegardless)
        seek.orderFrontRegardless()
        launcher?.orderFrontRegardless()
    }

    /// Caps Lock tap while the overlay is up: next match (design 2026-10-14).
    func stepSeekFromTap() {
        guard seekScan != nil else { return }
        stepSeek(1)
    }

    /// Esc steps back to search, like every other mode of the island.
    private func endSeek() {
        cancelClick()
    }

    func seekKey(_ event: NSEvent) -> Bool {
        let mods = event.modifierFlags.intersection([.command, .shift, .option, .control])
        if event.keyCode == 53, mods.isEmpty {
            endSeek()
            return true
        }
        if seekScan == nil {
            // Scan still running: swallow Return/Tab so we do not activate a command.
            return event.keyCode == 36 || event.keyCode == 76 || event.keyCode == 48
        }
        if event.keyCode == 48, mods.isEmpty || mods == .shift {
            stepSeek(mods == .shift ? -1 : 1)
            return true
        }
        if mods.isEmpty, event.characters == ";" {
            stepSeek(1)
            return true
        }
        if mods.isEmpty, event.keyCode == 36 || event.keyCode == 76 {
            clickSeek()
            return true
        }
        return false
    }

    private func stepSeek(_ delta: Int) {
        let n = seekSpans.count
        guard n > 0 else { return }
        seekIndex = ((seekIndex + delta) % n + n) % n
    }

    private func clickSeek() {
        guard let scan = seekScan else { return }
        let spans = seekSpans
        let cursor = seekCursor
        guard spans.indices.contains(cursor) else { return }
        let match = spans[cursor]
        guard scan.hits.indices.contains(match.hitIndex) else { return }
        let hit = scan.hits[match.hitIndex]
        let center = CGPoint(x: hit.x + hit.w / 2, y: hit.y + hit.h / 2)
        // An AXUIElement is a CF handle that is safe to message from any thread.
        nonisolated(unsafe) let element = hit.element
        cancelClick()
        hide()
        DispatchQueue.global(qos: .userInitiated).async { seekPress(element, at: center) }
    }
}

/// AXPress on the control when the hit has one, otherwise a click at its visible centre.
private func seekPress(_ element: AXUIElement?, at center: CGPoint) {
    if let element {
        var pid: pid_t = 0
        AXUIElementGetPid(element, &pid)
        NSRunningApplication(processIdentifier: pid)?.activate()
        usleep(80_000)
        // The scan's 0.15 s is too short for a press that opens a sheet or a menu.
        AXUIElementSetMessagingTimeout(element, 2)
        let result = AXUIElementPerformAction(element, kAXPressAction as CFString)
        Diagnostics.log("seek AXPress result \(result.rawValue)")
        // A timeout can mean the press landed and the app is busy with it. A click on top
        // of that presses twice, so click only when the press was refused.
        if result == .success || result == .cannotComplete { return }
    }
    _ = Engine.call(["op": "seekClick", "x": Double(center.x), "y": Double(center.y)])
}
