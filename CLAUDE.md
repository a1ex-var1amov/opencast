# OpenCast — project notes for Claude Code

Open-source Raycast-style launcher. SwiftUI plus `crates/opencast-ffi`. MIT.
macOS first; `crates/opencast-core` must stay portable (no platform UI deps).

## Layout
- `crates/opencast-core` — config, command registry + fuzzy search, scripts
  (incl. Script Filters), app discovery, calculator, saved layouts, keymap,
  AI provider adapters (+ all tests; `Message.images` carries image attachments,
  `supports_vision`/`looks_vision` decides pixels vs the panel's OCR text). Sidecar
  state (usage, pins, notes) is
  cached in `Engine` — search must never read disk.
- `crates/opencast-core` also: `mcp.rs` (MCP client: stdio + HTTP; tool, resource
  and prompt caches — `mcp-tools.json` and `mcp-extras.json`, so listing commands
  never connects),
  `chats.rs` (saved chats), `sync.rs` (iCloud symlink sync), `updates.rs`,
  `usage.rs` (frecency + learned picks), `extensions.rs` (git-installed
  extensions), `alfred.rs` (Alfred workflow → generated bash commands),
  `doctor.rs` (health report), `currency.rs`, `tz.rs` (TZif + POSIX rules,
  no tz crate), `panes.rs` (System Settings), `bookmarks.rs`, `window.rs`
  (window command geometry: gaps, cycling, displays, custom commands; the
  panel asks it via the `windowTarget` op), `pipeline.rs` (`[[pipelines]]`
  steps and text transforms; FFI `exec::run_pipeline` runs them),
  `webtext.rs` (HTML → text for page context). Providers
  support tool calling (`ToolSpec`, `ToolCall`, `ChatEvent::ToolCalls`) in
  all three wire formats.
- `crates/opencast-ffi` — macOS C ABI: hotkeys, hyper key, clipboard, CGEvent
  paste/copy, Seek OCR, cached app index, Shortcuts (`shortcuts_mac.rs`),
  `agent.rs` (MCP hub, AI tool loop with approvals), `extensions_mac.rs`
  (install/import/doctor; plutil, ditto), `refresh_mac.rs` (bookmarks, rates)
- `macos/` — SwiftUI panel. `WindowManager.swift` does snapping, layouts and
  AX selection reads; nothing spawns `osascript` on a hot path. `Spaces.swift`
  covers Mission Control desktops (SkyLight reads, a synthesized Dock swipe to
  switch — with the IOHID payload macOS 27+ requires — and SkyLight's bridged
  operation to move a window; every private symbol is looked up by name), with
  its AppKit-free labelling in `SpaceLabel.swift`. Also
  `SettingsWindow.swift`, `Preview.swift` (pane + Quick Look), `MenuSearch`,
  `Schedule` (EventKit), `Dictation`, `SnippetExpander`, `ConfigWatcher`,
  `Attachments`, `MacExtras` (Contacts, Reminders, dictionary, media keys,
  Large Type, Focus incl. site blocking and menu bar timer), `NotesEditor`
  (+ sticky notes), `WindowUX` (region glyphs, HUD, live preview overlay,
  drag to snap), `PageContext` (front browser tab), `CompareModels`,
  `CheatSheet`, `Dictation` (+ Dictate Anywhere HUD)
- Panel ↔ engine is JSON over `opencast_call`; panel-only features come back
  as `ExecResult::Ui { action }`
- `examples/config.toml` — default config, embedded via include_str!

## Build / test
- Requirements: Rust and the Xcode command-line tools
- `cargo test` — core tests (runs anywhere)
- `./test-swift.command` — panel logic tests: compiles the pure panel files
  (DeepLink, MeetingLinks, SnippetMatch) with `macos-tests/main.swift` (assert
  style, no XCTest); extracted files must not touch Bridge/AppKit
- `./smoke.command` — fmt, tests, clippy, panel logic tests, then OpenCast Doctor
  on the real config
- The FFI crate only compiles for macOS (Linux builds a stub), so FFI and Swift
  changes are verified by `./build-app.command` on a Mac
- `./build-app.command` — builds and installs the app to ~/Applications/OpenCast.app
  (this arch; `OC_ARCH=universal` builds arm64 + x86_64 and lipo's them)
- `./run-opencast.command` — debug app at target/debug/OpenCast.app
- `./release.command` — signed + notarized universal dmg/zip (Developer ID needed);
  prints the Homebrew cask filled in from the git remote
- `packaging/build-arch.zsh <arm64|x86_64> <log> <out>` — one arch slice (Rust
  staticlib + swiftc link); CI runs it for x86_64 so the Intel build cannot rot
- Do NOT export MACOSX_DEPLOYMENT_TARGET for cargo (Rust 1.91 release builds
  then cannot load proc macros); swiftc `-target` sets the app minimum
- User config lives in ~/.config/opencast/config.toml (NOT in this repo);
  reload via the "Reload Commands" command in the launcher

## Conventions
- rustfmt with max_width=120 (rustfmt.toml)
- No new crate deps without need; keys never in config files
- Window mgmt/selection grabbing need the macOS Accessibility permission
- Design notes live in `docs/superpowers/specs/`; the 2026-10-20 speed and
  customization spec describes the config surface (`[[commands]]`,
  `fallbacks`, `[keys]`, `theme_file`, AI `output`); 2026-10-21 covers MCP,
  Shortcuts, ranking, actions/preview, chats, settings, sync, releases
