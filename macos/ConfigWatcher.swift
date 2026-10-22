import Foundation

/// Reload when `config.toml` or the scripts folder changes on disk (your
/// editor, a git pull, iCloud sync from another Mac). No more "Reload Commands".
@MainActor
final class ConfigWatcher {
    static let shared = ConfigWatcher()

    private var sources: [DispatchSourceFileSystemObject] = []
    private var pending: Task<Void, Never>?
    private var configDir = ""

    func start(configDir: String) {
        self.configDir = configDir
        lastConfigStamp = (try? FileManager.default.attributesOfItem(atPath: configPath)[.modificationDate]) as? Date
        arm()
    }

    /// Watch the files themselves (follows symlinks, e.g. iCloud-synced config)
    /// and the folders (editors save by replacing the file).
    private func arm() {
        for s in sources { s.cancel() }
        sources = []
        let config = URL(fileURLWithPath: configDir).appendingPathComponent("config.toml").resolvingSymlinksInPath().path
        let scripts = URL(fileURLWithPath: configDir).appendingPathComponent("scripts").resolvingSymlinksInPath().path
        let configFolder = (config as NSString).deletingLastPathComponent
        for path in Set([config, scripts, configFolder]) {
            let fd = open(path, O_EVTONLY)
            guard fd >= 0 else { continue }
            let src = DispatchSource.makeFileSystemObjectSource(
                fileDescriptor: fd,
                eventMask: [.write, .delete, .rename, .extend],
                queue: .main
            )
            let isFolder = path != config
            src.setEventHandler { [weak src] in
                let mask = src?.data ?? []
                MainActor.assumeIsolated { ConfigWatcher.shared.changed(path: path, isFolder: isFolder, mask: mask) }
            }
            src.setCancelHandler { close(fd) }
            src.resume()
            sources.append(src)
        }
    }

    private var lastConfigStamp: Date?

    private func changed(path: String, isFolder: Bool, mask: DispatchSource.FileSystemEvent) {
        // The config folder also holds files OpenCast writes all the time
        // (usage, clipboard); only react when config.toml itself changed.
        if isFolder, path != scriptsPath {
            let stamp = (try? FileManager.default.attributesOfItem(atPath: configPath)[.modificationDate]) as? Date
            guard stamp != lastConfigStamp else { return }
            lastConfigStamp = stamp
        }
        if mask.contains(.delete) || mask.contains(.rename) {
            // The file was replaced: watch the new one.
            Task { @MainActor in
                try? await Task.sleep(nanoseconds: 200_000_000)
                ConfigWatcher.shared.arm()
            }
        }
        pending?.cancel()
        pending = Task { @MainActor in
            try? await Task.sleep(nanoseconds: 400_000_000)
            guard !Task.isCancelled else { return }
            let text = await Task.detached { Engine.call(["op": "reload"])["text"] as? String ?? "Reloaded" }.value
            SnippetExpander.shared.reload()
            Model.shared.refreshSettings()
            if Model.shared.launcher?.isVisible == true {
                Model.shared.flash(text, error: text.contains("warning"), thenHide: false)
                if Model.shared.ready { Model.shared.search() }
            }
        }
    }

    private var configPath: String {
        URL(fileURLWithPath: configDir).appendingPathComponent("config.toml").resolvingSymlinksInPath().path
    }

    private var scriptsPath: String {
        URL(fileURLWithPath: configDir).appendingPathComponent("scripts").resolvingSymlinksInPath().path
    }
}
