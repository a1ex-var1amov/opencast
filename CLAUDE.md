# OpenCast — project notes for Claude Code

Open-source Raycast-style launcher. SwiftUI plus `crates/opencast-ffi`. MIT.
macOS first; `crates/opencast-core` must stay portable (no platform UI deps).

## Layout
- `crates/opencast-core` — config, command registry + fuzzy search, scripts,
  app discovery, calculator, saved layouts, AI provider adapters (+ all tests)
- `crates/opencast-ffi` — macOS C ABI: hotkeys, clipboard, paste, window management
- `macos/` — SwiftUI panel
- `examples/config.toml` — default config, embedded via include_str!

## Build / test
- Requirements: Rust and the Xcode command-line tools
- `cargo test` — core tests (runs anywhere)
- `./build-app.command` — builds and installs the app to ~/Applications/OpenCast.app
- `./run-opencast.command` — debug app at target/debug/OpenCast.app
- User config lives in ~/.config/opencast/config.toml (NOT in this repo);
  reload via the "Reload Commands" command in the launcher

## Conventions
- rustfmt with max_width=120 (rustfmt.toml)
- No new crate deps without need; keys never in config files
- Window mgmt/selection grabbing need the macOS Accessibility permission
