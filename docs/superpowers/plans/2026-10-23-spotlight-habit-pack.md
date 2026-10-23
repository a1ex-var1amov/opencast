# Spotlight habit pack Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development. Checkbox tracking. **Do not git commit** unless the user asks.

**Goal:** Make empty-query and short-query search feel like Spotlight for apps, files, web fallback, and recent docs.

**Architecture:** Tune `Registry::search_with_usage` empty path; extend `fallback_commands` with `"web"`; improve `files_mac` folder rows; add `recents.rs` cache mirrored on history/bookmarks refresh.

**Tech Stack:** opencast-core, opencast-ffi, Swift panel. No new crate deps.

## Global Constraints

- No new deps; core stays portable.
- No emoji in MD/source.
- rustfmt max_width 120.
- Ponytail: shortest diff; `// ponytail:` for ceilings.
- Graphify before exploring.
- Do not implement Spotify covers in this plan.
- Do not commit unless asked.

## File map

| Task | Files |
|---|---|
| Empty query | `crates/opencast-core/src/commands.rs` (+ tests) |
| Files/folders | `crates/opencast-ffi/src/files_mac.rs`, panel file-row mapping in `OpenCastApp.swift` / Theme if needed |
| Web fallback | `crates/opencast-core/src/engine.rs` `fallback_commands`, `examples/config.toml`, tests |
| Recents | new `crates/opencast-core/src/recents.rs`, `lib.rs`, `engine.rs` Extras/search, `refresh_mac.rs`, Swift Kind only if new kind string |

---

### Task 1: Empty query — apps + pins lead

**Files:** Modify `crates/opencast-core/src/commands.rs`

- [x] Change empty-query assembly so order is: **pins (any)** → **apps by frecency** → **capped non-app commands** (existing Window≤12, Builtin≤16) → remainder
- [x] Replace test `empty_query_lists_commands_before_apps` with `empty_query_lists_apps_before_uncapped_commands` (apps appear before window/builtin flood; with usage frecency on an app, that app ranks near top)
- [x] Keep `empty_query_never_fills_the_screen_with_one_kind` passing (caps still apply to rows before the app block… or caps apply within the command section after apps — either way apps must be visible in first 60)
- [x] `cargo test -p opencast-core commands::`

### Task 2: File/folder rows clearer

**Files:** `crates/opencast-ffi/src/files_mac.rs`; wherever searchFiles JSON is built (dispatch/host); Swift row mapping if subtitle/icon set there

- [x] Do not drop directories in `filter_paths` (today only drops `.app`)
- [x] Prefer marking folders (e.g. trailing `/` in returned path or separate flag in JSON if already supported)
- [x] Subtitle = abbreviated parent path; folder icon when directory
- [x] If soft-expand debounce hides files too long, reduce delay slightly only if code is obvious — ponytail: skip if non-trivial
- [x] Unit or ffi test: a directory path survives filter
- [x] `cargo test -p opencast-ffi` / check

### Task 3: `"web"` fallback token

**Files:** `engine.rs` `fallback_commands`; `examples/config.toml` comment; test near `fallbacks_follow_config_order`

- [x] When fallbacks contains `"web"` (case-insensitive), push a synthetic Command: id `fallback:web`, title `Search the Web`, subtitle/url via DuckDuckGo `https://duckduckgo.com/?q={query}` as Quicklink kind (plan must open URL with query substituted — reuse existing quicklink run path; if synthetic needs a registry entry, register ephemeral or handle in `plan()`)
- [x] Only appears through existing weak-hit fallback injection (already gated by `fallback_threshold`)
- [x] Comment in examples/config.toml
- [x] Test: config with `fallbacks = ["web"]` → `fallback_commands` contains the web row; `plan` opens substituted URL
- [x] `cargo test -p opencast-core`

### Task 4: Recent documents in main search

**Files:** Create `recents.rs`; wire Extras + refresh like history; search merge

- [x] `recents::{Visit, load_cache, save_cache, scan_here, matching}` — mdfind last-used in home, exclude `/Library/`, limit ~50, copy-free query via sqlite-not-needed (mdfind CLI like Swift already does)
- [x] Refresh from `refresh_mac` off main thread; cache `recents.json`
- [x] Search: prefix `recent`/`recents` shows them first; also blend top few when query matches name (like history)
- [x] Kind: `CommandKind::File`, subtitle `Recent · ~/…`
- [x] Opening: existing file open plan for `file:` ids — use `file:<path>` or `recent:` handled in plan to open
- [x] Tests with fixture paths / matching helpers (no live mdfind required for unit tests)
- [x] `cargo test -p opencast-core`

### Task 5: Verify

- [x] `cargo test -p opencast-core --lib -- --skip extensions::`
- [x] `cargo test -p opencast-ffi`
- [x] `./test-swift.command`
- [x] `graphify update .`
- [x] Update README one-liner under search/fallbacks if behavior user-visible

## Spotify

Out of scope. See design outline only.

## Spec coverage

Design v1 goals 1–5 → Tasks 1–5. Spotify → no tasks.
