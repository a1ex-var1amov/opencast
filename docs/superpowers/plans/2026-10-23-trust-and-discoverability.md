# Trust and discoverability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [x]`) syntax for tracking. **Do not git commit** unless the user explicitly asks — stage nothing; leave a clean working tree the parent can commit later.

**Goal:** Ship the 2026-10-23 trust/discoverability design: visible hotkey failures, no silent Kind drops, permission nudges, config deep-links, browser history, minimal MCP OAuth, snippet editor, hyper-key hardening, Seek Vision OCR, shared `url_encode`, Model/engine splits, and Swift tests.

**Architecture:** Prefer surfacing existing engine/FFI signals (hotkey `Vec<String>` problems) over new subsystems. Browser history mirrors `bookmarks.rs`. OAuth reuses Keychain + loopback. Splits are move-only.

**Tech Stack:** Rust `opencast-core` / `opencast-ffi`, SwiftUI panel, macOS Keychain / Vision / Carbon hotkeys. No new crate deps.

## Global Constraints

- No new crate dependencies; `opencast-core` stays portable (no AppKit).
- No emoji in MD or source comments.
- rustfmt `max_width=120`.
- Ponytail: shortest working diff; mark ceilings with `// ponytail:` when deferring.
- Before exploring code: `graphify query` / `explain` first (repo rule).
- Do not commit unless parent says so.
- After code edits: parent runs `graphify update .`.

## File map

| Area | Primary files |
|---|---|
| Hotkeys UI | `macos/OpenCastApp.swift` (`checkHotkeys`), `macos/SettingsWindow.swift`, maybe `crates/opencast-ffi/src/host.rs` if new op |
| Kind | `macos/Models.swift`, `macos-tests/main.swift`, icon switch sites in `LauncherView.swift` |
| Permissions nudge | `macos/OpenCastApp.swift` (`show`), `macos/SettingsWindow.swift` |
| Config deep-link | `macos/SettingsWindow.swift` |
| url_encode | `crates/opencast-core/src/engine.rs`, `pipeline.rs`, `lib.rs` if re-export |
| Hyper key | `crates/opencast-ffi/src/hyperkey.rs`, `host.rs` |
| Seek OCR | `crates/opencast-ffi/src/window_mgmt.rs`, optionally `macos/ClipboardHistory.swift` helper |
| Browser history | new `crates/opencast-core/src/history.rs`, `engine.rs` assemble, `CommandKind` or reuse, `doctor.rs`, `lib.rs` |
| MCP OAuth | `crates/opencast-core/src/mcp.rs`, `config.rs`, FFI/Keychain, Settings MCP pane |
| Snippets UI | `macos/SettingsWindow.swift`, config set ops in host |
| Model split | `macos/OpenCastApp.swift` → `ChatModel.swift` / `SeekModel.swift` (names TBD) |
| Engine split | `engine.rs` → `window.rs` for `WindowAction`, `builtins.rs` optional |

---

### Task 1: Unknown Kind fallback

**Files:** Modify `macos/Models.swift`; `macos/LauncherView.swift` (kind icon/label switches); Test `macos-tests/main.swift`

**Produces:** `Command.Kind.unknown` with `label == "Unknown"`; `Command.init?` never fails solely on kind string.

- [x] Add `case unknown` to `Kind`; in `init?(_ raw:)`, if `Kind(rawValue:)` fails, set `.unknown` and append `kind: <raw>` to subtitle when missing
- [x] Exhaustive switches: treat `.unknown` like `.builtin` for icons
- [x] Test: decode JSON row with `"kind":"brandnew"` yields non-nil Command with `.unknown`
- [x] Run `./test-swift.command` — pass

### Task 2: Unify `url_encode`

**Files:** Modify `crates/opencast-core/src/pipeline.rs`, `engine.rs`

**Produces:** Single form-encoding helper (`+` for space); Spotify URI encoder stays separate (`%20`).

- [x] Delete `pipeline::url_encode`; call `crate::engine::url_encode` (or move both to a 10-line `encode.rs` if engine↔pipeline cycle — prefer engine public fn)
- [x] Keep `spotify_search_url` local `%20` behavior
- [x] `cargo test -p opencast-core` — pass

### Task 3: Hyper-key crash marker

**Files:** Modify `crates/opencast-ffi/src/hyperkey.rs`, `host.rs`

**Produces:** While Caps Lock is remapped, a marker file exists under the config dir; launch clears remap if marker present even when hidutil parse is flaky; `libc::atexit` restores.

- [x] `marker_path() -> PathBuf` next to config (`opencast-hyperkey.on`)
- [x] Write marker when remapping on; remove when restoring
- [x] On host start: if marker exists, `restore_caps_lock_if_ours()` then remove marker
- [x] Register `atexit` that calls restore (in addition to existing signals)
- [x] `cargo test -p opencast-ffi` (or compile) — pass on macOS

### Task 4: Seek OCR via Vision (drop JXA)

**Files:** Modify `crates/opencast-ffi/src/window_mgmt.rs`; optionally extract shared helper

**Produces:** OCR path does not invoke `osascript` for Vision; uses in-process Vision or Swift callback already used by clipboard.

- [x] Graphify/read how ClipboardHistory OCR works; reuse the same API from FFI (Swift bridge) or call Vision from Rust via existing mac bindings if present
- [x] Remove OCR JXA script string and `osascript` call for Seek
- [x] Leave a unit/fixture test for SeekHit parsing if one exists; add minimal parse test if cheap
- [x] Build: `cargo check -p opencast-ffi` on macOS

### Task 5: Surface hotkey problems + Screen Recording nudge + config deep-links

**Files:** Modify `macos/OpenCastApp.swift`, `macos/SettingsWindow.swift`

**Produces:** Settings shows last hotkey problem lines; Permissions notes rebuild TCC reset; General has section jump buttons; panel flashes once if Screen Recording missing when showing.

- [x] `SettingsState.hotkeyProblems: [String]`; `checkHotkeys` fills it and `flash` first problem if any (thenHide: false)
- [x] General pane: list problems if non-empty; Permissions detail text mentions rebuilt binaries need re-toggle
- [x] On `show()`, if `!CGPreflightScreenCaptureAccess()`, flash once per session (`didNudgeScreenRecording`)
- [x] Buttons: Open `[[hotkeys]]` / `[[snippets]]` / `[[mcp_servers]]` — open config.toml (existing) + status string naming the section (ponytail: no line-jump in editor)
- [x] Manual: set invalid hotkey in config, Reload, open Settings — see the line

### Task 6: Swift tests for Kind + handleUi id table (if extractable)

**Files:** Modify `macos-tests/main.swift`; only extract pure functions if already pure

**Produces:** Kind unknown test (Task 1) plus one assert on a pure helper if `handleUi` routing cannot be tested without AppKit — do not force AppKit into macos-tests.

- [x] Kind test from Task 1
- [x] If a pure `UiAction` parser exists or can be 20 lines, test 3 ids; else skip with comment
- [x] `./test-swift.command` pass

### Task 7: Snippet editor (minimal)

**Files:** Modify `macos/SettingsWindow.swift`; host `configSet` / snippet list ops as needed

**Produces:** Settings General (or small Snippets section): list keywords, add, delete; writes `[[snippets]]` via existing TOML mutation path.

- [x] Discover how config writes snippets today (`configSet`, reload)
- [x] UI: List + keyword/expansion fields + Add / Delete
- [x] Reload `SnippetExpander` after save
- [x] Smoke: add `;test` snippet, expand in TextEdit

### Task 8: Browser history search

**Files:** Create `crates/opencast-core/src/history.rs`; Modify `lib.rs`, `engine.rs` assemble/search, `doctor.rs`, `CommandKind` if needed, panel Kind if new

**Produces:** History URLs as searchable commands (subtitle `History · Chrome`); copy-then-query SQLite like other tools; never block main thread on TCC folders (refresh path already async).

- [x] Chrome-family: copy `History` to temp, `SELECT url, title FROM urls ORDER BY last_visit_time DESC LIMIT N`
- [x] Wire into registry like bookmarks
- [x] Doctor line when History unreadable
- [x] Add Swift `Kind` case if new kind string is introduced (must not silently drop)
- [x] `cargo test -p opencast-core` history unit tests with fixture DB or mocked rows

### Task 9: MCP OAuth (minimal)

**Files:** Modify `crates/opencast-core/src/mcp.rs`, `config.rs`, FFI Keychain helpers, Settings MCP pane

**Produces:** Server config fields for authorize/token/client_id; loopback redirect; access+refresh in Keychain; Authorization Bearer on HTTP MCP.

- [x] Config fields on `McpServer` (optional)
- [x] `oauth_ensure_token(server) -> Result<String>` refresh or interactive (open URL + local listener)
- [x] Settings: "Sign in…" button per OAuth server
- [x] Test: refresh path with mocked HTTP if feasible; otherwise unit-test URL/state builder
- [x] Document in README MCP section one short paragraph

### Task 10: Split `engine.rs` (move-only)

**Files:** Move `WindowAction` (+ related) into `crates/opencast-core/src/window.rs` (already exists — extend); optionally `builtins` table to `builtins.rs`; keep re-exports so call sites compile

**Produces:** `engine.rs` line count drops; behavior identical; tests still pass.

- [x] Move types/impls; `pub use` from engine if needed
- [x] `cargo test -p opencast-core` pass

### Task 11: Split Model (move-only)

**Files:** Peel chat / seek / clipboard-UI chunks from `macos/OpenCastApp.swift` into new Swift files in `macos/`; update `build-app.command` / compile lists if files are enumerated

**Produces:** `OpenCastApp.swift` shrinks; Model remains coordinator; no behavior change.

- [x] Identify compile entry (all `macos/*.swift` glob?)
- [x] Move Seek + Chat helpers first (least coupled)
- [x] `./build-app.command` or `./run-opencast.command` compiles
- [x] `./test-swift.command` pass

### Task 12: Verify

- [x] `cargo test`
- [x] `./test-swift.command`
- [x] `graphify update .`
- [x] Parent summarizes remaining manual checks (OAuth browser, history on real Chrome, Seek OCR)

---

## Wave order (parallelism)

- **Wave 1 (parallel):** Tasks 1, 2, 3, 4 (no shared files)
- **Wave 1b:** Task 5 + 6 (Settings/OpenCastApp + tests) after 1 so Kind tests land
- **Wave 2 (parallel):** Tasks 7, 8, 9
- **Wave 3 (sequential):** Task 10 then 11 (large moves), then 12

## Spec coverage

All design table rows map to Tasks 1–11. Non-goals have no tasks.
