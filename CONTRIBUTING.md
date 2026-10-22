# Contributing

Short version: Rust core in `crates/`, SwiftUI panel in `macos/`, everything the
user can see is driven by `config.toml`. Keep the core portable, keep hot paths
off disk, and add a test with every fix.

## Layout

- `crates/opencast-core` — config parsing, command registry + fuzzy search,
  apps, scripts, calculator, keymap, AI provider adapters, MCP client, chats,
  pipelines, doctor, sync. No platform UI deps: this crate must build on Linux.
- `crates/opencast-ffi` — the macOS C ABI (`macos/opencast.h`): hotkeys,
  hyper key, clipboard, CGEvent paste, Shortcuts, Seek OCR, the MCP hub and the
  AI tool loop. Linux builds a stub.
- `macos/` — the SwiftUI panel. `WindowManager.swift` holds snapping and AX
  reads; nothing on a hot path spawns `osascript`.
- Panel ↔ engine is JSON over `opencast_call`. Panel-only features return
  `ExecResult::Ui { action }`.

## Build, test, run

```sh
cargo test                        # core tests, runs anywhere
./smoke.command                   # tests + clippy + Doctor on your real config
./build-app.command               # build and install ~/Applications/OpenCast.app
./run-opencast.command            # debug app at target/debug/OpenCast.app
./release.command                 # signed + notarized dmg/zip (needs Developer ID)
```

The FFI crate and the panel only compile on a Mac, so check Swift/Rust-FFI
changes with `./build-app.command` there. Do **not** export
`MACOSX_DEPLOYMENT_TARGET` for cargo (Rust 1.91 release builds then cannot load
proc macros); the app's minimum comes from swiftc's `-target`.

## Rules this codebase follows

- **No new crate dependencies without a real need.** `tz.rs` parses TZif and
  POSIX rules instead of pulling a tz crate; `currency.rs` uses one HTTP call.
- **Keys never go in config files.** Secrets live in the Keychain; config holds
  `api_key_env` / `api_key_cmd`.
- **Search must not touch disk.** Sidecar state (usage, pins, notes) is cached in
  `Engine` at load.
- **rustfmt** with `max_width = 120` (`rustfmt.toml`); CI runs `cargo fmt --all
  -- --check` and clippy with `-D warnings`.
- Comments say why, not what, and mention the failure a line prevents.

## Adding a setting

1. Add the field to the right struct in `crates/opencast-core/src/config.rs`
   with a `#[serde(default = …)]`, and validate it in `validate()`.
2. Document it in `examples/config.toml` (that file is embedded in the binary and
   is the reference for the whole surface). There is a test that fails if a key
   in the example config is not in the structs.
3. Expose it in Settings (`macos/SettingsWindow.swift`) when there is a natural
   control, and through the `settings` / `setSetting` ops.

## Changing search

Ranking lives in `Registry::search_with_usage` (`commands.rs`). Two invariants
have tests — keep them:

- A typed query must never return an empty list when apps are installed (the
  letter-prefix fallback).
- An empty query must not fill the screen with one kind: window commands and
  built-ins are capped, apps and Recents are kept, and anything pinned, boosted
  or actually used is exempt.

## Docs

Design notes go in `docs/superpowers/specs/` (dated files, what was built and
why). User-facing changes get a line in `CHANGELOG.md`.
