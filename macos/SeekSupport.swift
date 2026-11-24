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
        DispatchQueue.global(qos: .userInitiated).async {
            let outcome: Result<SeekScan, Error>
            do { outcome = .success(try SeekAX.scan()) } catch { outcome = .failure(error) }
            DispatchQueue.main.async {
                Model.shared.busy = false
                switch outcome {
                case .success(let scan):
                    Model.shared.presentSeek(scan)
                case .failure(let error):
                    Model.shared.cancelClick()
                    if let fail = error as? SeekAX.Failure, case .accessibility = fail {
                        Model.shared.permissionToast("Click Text", needs: "Accessibility")
                    } else {
                        Model.shared.flash(error.localizedDescription, error: true, thenHide: false)
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

    private func endSeek() {
        cancelClick()
        hide()
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
        let center = CGPoint(
            x: scan.monitor.x + match.x + match.w / 2,
            y: scan.monitor.y + match.y + match.h / 2
        )
        let element = hit.element
        cancelClick()
        hide(fromRust: true)
        // GCD: AXUIElement is not Sendable; same pattern as menu-search press.
        DispatchQueue.global(qos: .userInitiated).async {
            SeekAX.press(SeekHit(
                label: hit.label, x: hit.x, y: hit.y, w: hit.w, h: hit.h,
                words: hit.words, element: element
            ), fallbackCenter: center)
        }
    }
}
