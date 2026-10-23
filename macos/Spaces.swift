import AppKit

// MARK: - Spaces (Mission Control desktops)
//
// SkyLight is private, so every symbol is looked up by name: a missing one turns
// into an error message instead of a crash. Two facts drove this design, both
// checked against macOS 26 and 27:
//
//   * Switching spaces is a synthesized Dock swipe. The SkyLight show/hide calls
//     still exist on 27 but do nothing; the Dock only accepts the gesture once
//     the event carries an IOHID payload (the same augmentation
//     InstantSpaceSwitcher and yabai send).
//   * Moving a window is SkyLight's own bridged operation, which wants the
//     space's `id64`, not the id `SLSGetActiveSpace` reports.

struct SpaceInfo {
    /// `ManagedSpaceID` — what `SLSGetActiveSpace` reports.
    let mid: UInt64
    /// `id64` — what the move operation wants (they differ for full-screen tiles).
    let id64: UInt64
    let uuid: String
    /// A full-screen window's own space, not a desktop.
    let fullscreen: Bool
    /// The app that owns a full-screen tile.
    let ownerApp: String?
    /// Window owners on that space, best effort.
    var apps: [String] = []

    var label: String {
        if let ownerApp { return "Full Screen · \(ownerApp)" }
        return "Space"
    }

    var summary: String { SpaceLabels.summary(apps, fullscreen: fullscreen) }
}

enum Spaces {
    static let unavailable = "Spaces are not available on this macOS version"

    private typealias FnCID = @convention(c) () -> Int32
    private typealias FnCopyDisplays = @convention(c) (Int32) -> Unmanaged<CFArray>?
    private typealias FnActive = @convention(c) (Int32) -> Int32
    private typealias FnWindowsInSpace =
        @convention(c) (UInt32, UInt32, CFArray, UInt32, UnsafeMutablePointer<UInt64>, UnsafeMutablePointer<UInt64>) -> Unmanaged<CFArray>?
    private typealias FnGetWindowID = @convention(c) (AXUIElement, UnsafeMutablePointer<UInt32>) -> AXError

    private static let sky = dlopen("/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight", RTLD_NOW)
    private static let appsv = dlopen("/System/Library/Frameworks/ApplicationServices.framework/ApplicationServices", RTLD_NOW)

    private static func sym<T>(_ handle: UnsafeMutableRawPointer?, _ name: String, _ type: T.Type) -> T? {
        guard let handle, let sym = dlsym(handle, name) else { return nil }
        return unsafeBitCast(sym, to: type.self)
    }

    private static func cid() -> Int32? {
        guard let fn = sym(sky, "SLSMainConnectionID", FnCID.self) else { return nil }
        let value = fn()
        return value == 0 ? nil : value
    }

    static var available: Bool { cid() != nil }

    // MARK: reading

    /// Spaces in Mission Control order, left to right.
    static func list() -> [SpaceInfo] {
        let raw = rawSpaces().filter { space in
            let kind = (space["type"] as? NSNumber)?.int32Value ?? 0
            return kind == 0 || kind == 4
        }
        guard !raw.isEmpty else { return [] }
        let owners = appsBySpace(raw)
        // SkyLight lists the spaces right to left: a right swipe moves *down*
        // the array. Verified on macOS 26 and 27; reversing makes the numbering
        // match what Mission Control shows.
        return raw.reversed().map { space in
            let id64 = (space["id64"] as? NSNumber)?.uint64Value ?? 0
            let mid = (space["ManagedSpaceID"] as? NSNumber)?.uint64Value ?? id64
            let kind = (space["type"] as? NSNumber)?.int32Value ?? 0
            let pid = (space["pid"] as? NSNumber)?.int32Value
            return SpaceInfo(
                mid: mid, id64: id64, uuid: (space["uuid"] as? String) ?? "",
                fullscreen: kind == 4,
                ownerApp: pid.flatMap { NSRunningApplication(processIdentifier: pid_t($0))?.localizedName },
                apps: owners[id64] ?? []
            )
        }
    }

    /// The space currently shown, or nil.
    static func activeMid() -> UInt64? {
        guard let c = cid(), let fn = sym(sky, "SLSGetActiveSpace", FnActive.self) else { return nil }
        let value = fn(c)
        return value == 0 ? nil : UInt64(value)
    }

    private static func rawSpaces() -> [[String: Any]] {
        guard let c = cid(), let fn = sym(sky, "SLSCopyManagedDisplaySpaces", FnCopyDisplays.self),
              let displays = fn(c)?.takeRetainedValue() as? NSArray,
              let first = displays.firstObject as? NSDictionary,
              let spaces = first["Spaces"] as? [[String: Any]]
        else { return [] }
        // One entry per display; the first is the one Mission Control arrows move.
        return spaces
    }

    /// Window owners per space, from SkyLight's window lists.
    private static func appsBySpace(_ spaces: [[String: Any]]) -> [UInt64: [String]] {
        guard let c = cid(), let fn = sym(sky, "SLSCopyWindowsWithOptionsAndTags", FnWindowsInSpace.self) else { return [:] }
        var names: [UInt32: String] = [:]
        for w in (CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]]) ?? [] {
            guard (w[kCGWindowLayer as String] as? Int) == 0,
                  let number = (w[kCGWindowNumber as String] as? NSNumber)?.uint32Value,
                  let owner = w[kCGWindowOwnerName as String] as? String
            else { continue }
            names[number] = owner
        }
        var out: [UInt64: [String]] = [:]
        for space in spaces {
            guard let id64 = (space["id64"] as? NSNumber)?.uint64Value else { continue }
            var setTags: UInt64 = 0
            var clearTags: UInt64 = 0
            let list = fn(UInt32(bitPattern: c), 0, [NSNumber(value: id64)] as CFArray, 0x7, &setTags, &clearTags)?
                .takeRetainedValue() as? [NSNumber]
            guard let list else { continue }
            out[id64] = list.compactMap { names[$0.uint32Value] }
        }
        return out
    }

    // MARK: switching

    /// Move the view `steps` spaces left (<0) or right (>0).
    static func switchRelative(_ steps: Int) throws {
        guard steps != 0 else { return }
        guard available else { throw SpacesError(message: unavailable) }
        for _ in 0..<abs(steps) { swipe(steps > 0 ? 1 : -1) }
    }

    /// Move the view until `space` is showing. One swipe at a time, checked
    /// after each: swipes walk every space in order, full-screen tiles included.
    static func switchTo(_ space: SpaceInfo) throws {
        guard available else { throw SpacesError(message: unavailable) }
        let all = list()
        guard all.contains(where: { $0.mid == space.mid }) else {
            throw SpacesError(message: "that space is gone")
        }
        // Recompute the direction after every swipe, so a step that lands
        // somewhere unexpected (a tile, a reordered space) corrects itself.
        for _ in 0..<(all.count * 3) {
            let active = activeMid()
            if active == space.mid { return }
            guard let current = all.firstIndex(where: { $0.mid == active }),
                  let want = all.firstIndex(where: { $0.mid == space.mid })
            else { throw SpacesError(message: "the current space is not where we thought it was") }
            var direction = want > current ? 1 : -1
            for attempt in 0..<2 {
                let before = active
                swipe(direction)
                if activeMid() != before { break }
                // Nothing moved: an end of the row, or the sign is inverted here.
                direction = attempt == 0 ? -direction : direction
            }
        }
        throw SpacesError(message: "the space did not change")
    }

    // MARK: moving windows

    /// Move `window` to the space `delta` desktops away, then follow it there.
    static func move(_ window: AXUIElement, relative delta: Int) throws {
        let all = list()
        guard let active = activeMid(), let current = all.firstIndex(where: { $0.mid == active }) else {
            throw SpacesError(message: unavailable)
        }
        var step = current
        var target: SpaceInfo?
        while step + delta >= 0 && step + delta < all.count {
            step += delta
            // Full-screen tiles belong to their window; skip past them.
            if all[step].fullscreen { continue }
            target = all[step]
            break
        }
        guard let target else {
            throw SpacesError(message: delta > 0 ? "there is no space to the right" : "there is no space to the left")
        }
        try move(window, to: target)
    }

    static func move(_ window: AXUIElement, to space: SpaceInfo) throws {
        guard let wid = windowID(window) else { throw SpacesError(message: "could not identify that window") }
        try moveWindow(id: wid, to: space)
    }

    /// The same, for a window the caller already has a CG window id for.
    static func moveWindow(id wid: UInt32, to space: SpaceInfo) throws {
        guard let cls = NSClassFromString("SLSBridgedMoveWindowsToManagedSpaceOperation") as? NSObject.Type else {
            throw SpacesError(message: unavailable)
        }
        let initSel = Selector(("initWithWindows:spaceID:"))
        guard cls.instancesRespond(to: initSel) else { throw SpacesError(message: unavailable) }
        let windows = [NSNumber(value: wid)] as NSArray
        // Swift cannot call +alloc directly, so send it to the metaclass and
        // initialize the instance by selector. spaceID is the space's id64;
        // passing the ManagedSpaceID is a no-op.
        guard let receiver = (cls as AnyObject).perform(Selector(("alloc")))?.takeRetainedValue(),
              let result = receiver.perform(initSel, with: windows, with: NSNumber(value: space.id64)),
              let op = result.takeUnretainedValue() as? NSObject
        else { throw SpacesError(message: "the window could not be moved") }
        op.perform(Selector(("performWithWMBridgeDelegate")))
        // The bridged operation is asynchronous; give it a beat before following.
        Thread.sleep(forTimeInterval: 0.08)
        try switchTo(space)
    }

    private static func windowID(_ window: AXUIElement) -> UInt32? {
        guard let fn = sym(appsv, "_AXUIElementGetWindow", FnGetWindowID.self) else { return nil }
        var wid: UInt32 = 0
        return fn(window, &wid) == .success ? wid : nil
    }

    // MARK: the gesture

    /// One space left or right: the three phases of a Dock swipe.
    private static func swipe(_ direction: Int) {
        let sign = Double(direction)
        for phase: Int64 in [1, 2, 4] {
            guard let event = CGEvent(source: nil) else { return }
            event.setIntegerValueField(field(55), value: 30)      // Dock swipe
            event.setIntegerValueField(field(110), value: 23)     // fluid touch gesture
            event.setIntegerValueField(field(132), value: phase)  // began / changed / ended
            event.setIntegerValueField(field(123), value: 1)      // motion: one unit
            event.setDoubleValueField(field(124), value: sign * 0.000016)
            event.setIntegerValueField(field(134), value: phase)  // phase alias
            event.setDoubleValueField(field(138), value: 3.0)     // zoom delta
            event.setDoubleValueField(field(169), value: Double(mach_absolute_time()))
            event.setDoubleValueField(field(125), value: 0.1)     // start position x
            if phase == 4 { event.setDoubleValueField(field(129), value: sign * 9999) }
            if augmented(event, phase: phase) {
                // The companion event is what the Dock reads as the gesture ending.
                if let companion = CGEvent(source: nil) {
                    companion.setIntegerValueField(field(55), value: 29)
                    companion.post(tap: .cgSessionEventTap)
                }
            } else {
                event.post(tap: .cgSessionEventTap)
            }
            Thread.sleep(forTimeInterval: phase == 4 ? 0.12 : 0.05)
        }
    }

    /// macOS 27+ rejects the bare gesture: the event must carry the IOHID
    /// payload a real trackpad swipe would have (field 4205 of the serialized
    /// event). Returns false when the event was left as it was.
    private static func augmented(_ event: CGEvent, phase: Int64) -> Bool {
        guard ProcessInfo.processInfo.operatingSystemVersion.majorVersion >= 27 else { return false }
        guard let serialize = sym(cg(), "CGEventCreateData", FnCreateData.self),
              let deserialize = sym(cg(), "CGEventCreateFromData", FnFromData.self),
              let data = serialize(kCFAllocatorDefault, event as CFTypeRef)?.takeRetainedValue()
        else { return false }
        let bytes = [UInt8](data as Data)
        // Serialized events start with a format version; only version 2 carries fields.
        guard bytes.count >= 4, bytes[0] == 0, bytes[1] == 0, bytes[2] == 0, bytes[3] == 2 else { return false }

        let progress = event.getDoubleValueField(field(124))
        let velocityX = event.getDoubleValueField(field(129))
        let velocityY = event.getDoubleValueField(field(130))
        let withVelocity = velocityX != 0 || velocityY != 0 || phase == 4
        var payload: [UInt8] = []
        let stamp = event.timestamp != 0 ? event.timestamp : mach_absolute_time()
        payload += u64(stamp) + u64(0) + u32(0) + u32(0) + u32(withVelocity ? 2 : 1)
        payload += u32(40) + u32(23) + u32(UInt32((phase & 0xFF) << 24)) + [0, 0, 0, 0]
        payload += i32(fixed1616(0.1)) + i32(0) + i32(0) + u32(0) + u16(1) + u16(3) + i32(fixed1616(progress))
        if withVelocity {
            payload += u32(28) + u32(9) + u32(0) + [1, 0, 0, 0]
            payload += i32(fixed1616(velocityX)) + i32(fixed1616(velocityY)) + i32(0)
        }
        var out = bytes
        out += [UInt8(payload.count >> 8 & 0xFF), UInt8(payload.count & 0xFF)]
        out += [UInt8(4205 >> 8 & 0xFF), UInt8(4205 & 0xFF)]
        out += payload
        guard let blob = CFDataCreate(kCFAllocatorDefault, out, out.count),
              let rebuilt = deserialize(kCFAllocatorDefault, blob)?.takeRetainedValue()
        else { return false }
        rebuilt.post(tap: .cgSessionEventTap)
        return true
    }

    private static func cg() -> UnsafeMutableRawPointer? {
        dlopen("/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics", RTLD_NOW)
    }

    private typealias FnCreateData = @convention(c) (CFAllocator?, CFTypeRef) -> Unmanaged<CFData>?
    private typealias FnFromData = @convention(c) (CFAllocator?, CFData) -> Unmanaged<CGEvent>?

    private static func field(_ index: Int) -> CGEventField { CGEventField(rawValue: UInt32(index))! }

    /// Trackpad positions and progress travel as 16.16 fixed point.
    private static func fixed1616(_ value: Double) -> Int32 {
        let scaled = Int32(value * 65536.0)
        if scaled == 0 && value != 0 { return value > 0 ? 1 : -1 }
        return scaled
    }

    private static func u64(_ value: UInt64) -> [UInt8] { withUnsafeBytes(of: value.littleEndian) { Array($0) } }
    private static func u32(_ value: UInt32) -> [UInt8] { withUnsafeBytes(of: value.littleEndian) { Array($0) } }
    private static func u16(_ value: UInt16) -> [UInt8] { withUnsafeBytes(of: value.littleEndian) { Array($0) } }
    private static func i32(_ value: Int32) -> [UInt8] { withUnsafeBytes(of: value.littleEndian) { Array($0) } }
}
