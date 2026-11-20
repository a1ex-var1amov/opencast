import Foundation

/// UI timings and log lines. Batched into the Rust logger; no second log file.
enum Telemetry {
    private static let lock = NSLock()
    private static var pending: [(String, String)] = []
    private static var flushTimer: Timer?

    static func setLevel(_ level: String) {
        level.withCString { opencast_set_log_level($0) }
    }

    static func sample(_ name: String, ms: Int, budget: Int) {
        let ms = UInt32(clamping: max(0, ms))
        let budget = UInt32(clamping: max(0, budget))
        name.withCString { opencast_sample($0, ms, budget) }
    }

    static func event(_ level: String, _ message: String) {
        lock.lock()
        pending.append((level, message))
        let full = pending.count >= 40
        lock.unlock()
        if full {
            flush()
        } else {
            schedule()
        }
    }

    static func flush() {
        lock.lock()
        let batch = pending
        pending.removeAll()
        lock.unlock()
        for (level, message) in batch {
            level.withCString { l in
                message.withCString { m in opencast_log(l, m) }
            }
        }
    }

    /// Registers the panic callback. The C function stays in this file: naming
    /// it from AppShell made `swiftc -O` emit `_opencastReleaseStayAwake` twice.
    @inline(never)
    static func installPanicCleanup() {
        opencast_set_panic_cleanup(opencastReleaseStayAwake)
    }

    private static func schedule() {
        guard Thread.isMainThread else {
            DispatchQueue.main.async { schedule() }
            return
        }
        guard flushTimer == nil else { return }
        flushTimer = Timer.scheduledTimer(withTimeInterval: 0.4, repeats: false) { _ in
            flushTimer = nil
            flush()
        }
    }
}

/// Rust calls this from the panic hook and from terminate/signal. IOKit only.
@_cdecl("opencastReleaseStayAwake")
@inline(never)
private func opencastReleaseStayAwake() {
    StayAwakeController.shared.panicRelease()
}
