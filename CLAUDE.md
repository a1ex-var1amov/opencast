# OpenCast — project notes for Claude Code

Open-source Raycast-style launcher. Tauri 2 (Rust) + React/TS. MIT.
macOS first; `crates/opencast-core` must stay portable (no Tauri deps).

## Layout
- `crates/opencast-core` — config, command registry + fuzzy search, scripts,
  app discovery, calculator, saved layouts, AI provider adapters (+ all tests)
- `src-tauri` — shell: window, global hotkeys, IPC, clipboard/paste,
  `window_mgmt.rs` (window snapping + layouts via JXA/System Events)
- `src` — React UI; `src/mock.ts` is a browser-only stand-in backend
- `examples/config.toml` — default config, embedded via include_str!

## Build / test
- `cargo test` — core tests (runs anywhere)
- `npm run tauri dev` — dev app; `./build-app.command` — release build,
  installs to ~/Applications/OpenCast.app and relaunches it
- User config lives in ~/.config/opencast/config.toml (NOT in this repo);
  reload via the "Reload Commands" command in the launcher

## Conventions
- rustfmt with max_width=120 (rustfmt.toml)
- No new crate deps without need; keys never in config files
- Window mgmt/selection grabbing need the macOS Accessibility permission
