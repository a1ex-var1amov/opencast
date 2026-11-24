// Click Text's scan (macos/SeekAX.swift) against the screen in front of you.
// Compiled by ./probe-click-text.command. Needs Accessibility for the terminal
// that runs it. Prints counts and timings; labels only with --labels.
import AppKit
import ApplicationServices

@main
enum SeekProbe {
    static func main() {
        var failures = 0
        func check(_ ok: Bool, _ what: String) {
            print(ok ? "  ok   \(what)" : " FAIL  \(what)")
            if !ok { failures += 1 }
        }
        func rect(_ x: CGFloat, _ y: CGFloat, _ w: CGFloat, _ h: CGFloat) -> CGRect { CGRect(x: x, y: y, width: w, height: h) }

        // What hides what. Front to back, as the window server lists them.
        let stack = [
            SeekWindow(pid: 1, frame: rect(0, 0, 100, 100), layer: 25), // overlay level, full screen
            SeekWindow(pid: 2, frame: rect(0, 0, 60, 60), layer: 0), // a sheet of app 2
            SeekWindow(pid: 2, frame: rect(0, 0, 80, 80), layer: 0), // app 2's window
            SeekWindow(pid: 3, frame: rect(10, 10, 50, 50), layer: 0), // wholly behind app 2
            SeekWindow(pid: 4, frame: rect(50, 50, 50, 50), layer: 0), // peeks out from behind app 2
        ]
        check(SeekAX.occluders(stack, inFrontOf: 2).isEmpty, "an overlay level and the app's own sheet do not hide its window")
        check(SeekAX.occluders(stack, inFrontOf: 2, own: [1]).count == 1, "the app's own top-level window in front does")
        check(SeekAX.hidden(stack[3].frame, by: SeekAX.occluders(stack, inFrontOf: 3)), "a window wholly behind another app is hidden")
        check(!SeekAX.hidden(stack[4].frame, by: SeekAX.occluders(stack, inFrontOf: 4)), "a window that peeks out is not")
        check(SeekAX.hidden(.null, by: []), "nothing left on screen counts as hidden")
        if failures > 0 { exit(1) }

        guard AXIsProcessTrusted() else {
            print("\nThis terminal has no Accessibility permission, so the live scan was skipped.")
            return
        }
        let showLabels = CommandLine.arguments.contains("--labels")
        let started = Date()
        let scan: SeekScan
        do { scan = try SeekAX.scan() } catch {
            print("\nscan failed after \(Int(-started.timeIntervalSinceNow * 1000)) ms: \(error.localizedDescription)")
            exit(1)
        }
        let ms = Int(-started.timeIntervalSinceNow * 1000)
        let m = scan.monitor
        let bounds = rect(m.x, m.y, m.w, m.h)
        let windows = SeekAX.onScreenWindows(in: bounds)
        func name(_ pid: pid_t) -> String { NSRunningApplication(processIdentifier: pid)?.localizedName ?? "pid \(pid)" }
        func frontApp(at p: CGPoint) -> pid_t? { windows.first { $0.layer == 0 && $0.frame.contains(p) }?.pid }

        print("\ndisplay \(Int(m.w))x\(Int(m.h)), \(windows.count) windows on screen, \(Set(windows.map(\.pid)).count) apps")
        print("scan: \(scan.hits.count) hits from \(scan.visited) nodes in \(ms) ms (budget \(Int(SeekAX.budget * 1000)) ms)")

        // Which app's window is on top where each hit sits. Hits are only ever read from
        // windows that show, so this should list the apps you can actually see.
        var under: [String: Int] = [:]
        var wrongApp = 0
        for hit in scan.hits {
            let centre = CGPoint(x: hit.x + hit.w / 2, y: hit.y + hit.h / 2)
            let top = frontApp(at: centre)
            under[top.map(name) ?? "no normal window", default: 0] += 1
            if let el = hit.element {
                var pid: pid_t = 0
                AXUIElementGetPid(el, &pid)
                if let top, top != pid { wrongApp += 1 }
            }
        }
        print("hits by the app whose window is on top there:")
        for (app, n) in under.sorted(by: { $0.value > $1.value }) { print("  \(app): \(n)") }

        let controls = scan.hits.filter { $0.element != nil }.count
        let big = scan.hits.filter { $0.w * $0.h > m.w * m.h * 0.10 }.count
        var repeats = 0
        for (i, h) in scan.hits.enumerated() {
            guard let first = scan.hits[..<i].first(where: {
                $0.label == h.label && rect($0.x, $0.y, $0.w, $0.h).intersects(rect(h.x, h.y, h.w, h.h))
            }) else { continue }
            repeats += 1
            func shape(_ s: SeekHit) -> String {
                var role: CFTypeRef?
                if let el = s.element { AXUIElementCopyAttributeValue(el, kAXRoleAttribute as CFString, &role) }
                return "[\(Int(s.x)),\(Int(s.y)) \(Int(s.w))x\(Int(s.h))] \(role as? String ?? "text")"
            }
            print("  repeat: \(shape(first)) then \(shape(h)), \(h.label.count) characters, under \(frontApp(at: CGPoint(x: h.x + h.w / 2, y: h.y + h.h / 2)).map(name) ?? "-")")
        }
        print("\(controls) press a control, \(scan.hits.count - controls) click plain text")
        for q in ["e", "a", "s"] {
            print("typing \"\(q)\" highlights \(scan.hits.filter { $0.label.lowercased().contains(q) }.count)")
        }
        check(wrongApp == 0, "no control sits under another app's window (\(wrongApp))")
        check(big == 0, "no hit covers more than a tenth of the display (\(big))")
        check(repeats == 0, "no hit repeats an overlapping one with the same label (\(repeats))")
        check(ms <= Int(SeekAX.budget * 1000) + 300, "the scan stays inside its budget (\(ms) ms)")
        if showLabels {
            print("")
            for h in scan.hits { print("  [\(Int(h.x)),\(Int(h.y)) \(Int(h.w))x\(Int(h.h))]\(h.element == nil ? "" : " control") \(h.label)") }
        }
        exit(failures > 0 ? 1 : 0)
    }
}
