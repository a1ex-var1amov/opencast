# Speed and customization pass

Goal: make the daily-use paths feel as fast as Raycast/Rectangle, remove the
process spawns on the hot paths, and expose the customization knobs Raycast
and Alfred users set up on day one. Everything stays MIT, local-first, no
new network traffic.

Scope is the whole repo as reviewed on 2026-10-20 (including the uncommitted
parity branch). Tasks are grouped by crate so the portable core lands first
and is verified with `cargo test` before the macOS layer.

## Findings that drove this

| Area | Today | Cost | Fix |
|------|-------|------|-----|
| Window snap / layouts | JXA via `osascript` per action + 180 ms sleep | 150-400 ms | AX API from Swift; no subprocess |
| Paste / selection grab | `osascript keystroke` | ~100 ms + spawn | CGEvent keystrokes in Rust; AX `kAXSelectedText` from Swift first |
| Search per keystroke | reads+parses `usage.json`, `pinned.json`, `notes.json`; lowercases every title | disk I/O on UI path | sidecars cached in `Engine`; lowercase titles precomputed; id index |
| `usage::bump` | read+parse+write JSON before the plan runs | adds to every Return | in-memory counts, flushed after run |
| Startup | `mdfind` + 1 `Info.plist` read per app, before first show; binary plists silently fail | first show waits; wrong names | cached `apps.json`, one `mdls` for names, background refresh |
| Row render | `NSWorkspace.icon(forFile:)` on main per new row; `NSImage(systemSymbolName:)` per row per frame; thumbs loaded in `body` | main-thread hitches | background icon prefetch; memoized symbol table; thumbs loaded off-main |
| Clipboard polling | 150 ms timer always reads `frontmostApplication`; Desktop listed+stat'd every 2 s | battery | only read front app on changeCount change; FS event source for screenshot dir |
| Clipboard privacy | ignores a fixed bundle-id list | leaks passwords from unlisted apps | honour `org.nspasteboard.ConcealedType` / `TransientType` |
| Hyper key | `hidutil` remap persists after a crash; disabled tap never re-enabled | Caps Lock dead until reboot | cleanup on SIGTERM/launch; tap watchdog re-enables |
| Quit | `process::exit(0)` in a tokio task | Swift never flushes clipboard / StayAwake | `ExecResult::Quit`, Swift terminates |
| Scripts | run with the GUI env (no Homebrew, nvm...) | "works in Terminal, not here" | run via `$SHELL -lc`, PATH prepended |
| Config writes | textual TOML surgery | can corrupt configs | `toml_edit` (keeps comments) |
| StayAwake | spawns `caffeinate` | subprocess per toggle | `IOPMAssertionCreateWithName` |
| Build script | second `cargo rustc` to print native libs; stale `target/release` broke proc-macros | slow builds; broken build | print libs from the same build; `cargo clean` note |

## Customization added

All in `config.toml`; everything optional; reload with *Reload Commands*.

### `[[commands]]` overrides (any command, including apps and builtins)

```toml
[[commands]]
match = "Slack"            # command id ("app:/Applications/Slack.app") or title, case-insensitive
alias = "sl"               # exact alias match ranks above everything
icon = "💬"                # emoji or SF Symbol name ("bubble.left.fill")
title = "Slack (work)"     # rename
boost = 100                # added to the search score
hidden = true              # never listed
```

### Fallbacks

```toml
[general]
fallbacks = ["link:search-kagi", "builtin:chat", "files"]
```

Ordered rows shown when a query has fewer than `fallback_threshold` (default 3)
strong hits. `files` is the Spotlight expand; anything else is a command id or
title. Replaces the hardcoded "every `{query}` quicklink" rule.

### `[keys]` in-panel keymap

```toml
[keys]
ask_ai = "Tab"
click_text = "`"
actions = "Cmd+K"
pin = "Cmd+Shift+P"
paste_answer = "Cmd+Return"
copy_answer = "Cmd+Shift+C"
new_chat = "Cmd+N"
```

Parsed in core (`keymap.rs`) into a table the Swift panel reads once per
reload via the `keymap` op. Unknown keys fall back to defaults with a warning.

### Theme file

```toml
[general]
theme_file = "~/.config/opencast/themes/mine.toml"
```

```toml
# themes/mine.toml
accent = "#ff6a00"           # default: system accent
panel_light = "#f5f5f7"
panel_dark = "#1c1c1e"
radius = 18
row_height = 44
width = 760
font_size = 14
glass = true                 # false = opaque panel
```

Served through the `settings` op; Swift's `Theme` reads it.

### AI command options

```toml
[[ai_commands]]
id = "fix-grammar"
prompt = "Fix spelling and grammar:\n\n{selection}"
temperature = 0.2
max_tokens = 800
output = "replace"           # chat (default) | replace | paste | copy
```

`output = "replace"` streams the answer and then pastes it over the selection
without opening chat (the Raycast "AI on selection" feel). `{clipboard}` and
`{app}` placeholders join `{selection}` and `{input}`.

### Calculator

`sqrt(2)`, `sin(pi/2)`, `log(100)`, `ln`, `exp`, `abs`, `round`, `floor`,
`ceil`, `pi`, `e`, `0x1f + 0b101`, `15% of 80`, `80 + 15%`, `10 km in mi`,
`72 f in c`, `3 gb in mb`, `2 h in min`. Results keep the unit. No currency
(would need network); a `[[units]]` table can add custom ratios.

### Script Filter (Alfred-style)

```bash
#!/bin/bash
# @opencast.title Recent Branches
# @opencast.mode list
# @opencast.filter true        # OpenCast filters the items as you type
git branch --sort=-committerdate --format='%(refname:short)' | head -20 \
  | jq -R '{title: ., arg: ., subtitle: "checkout"}' | jq -s .
```

A `list` script prints a JSON array of `{title, subtitle?, icon?, arg,
autocomplete?}`. The panel shows the items; Return re-runs the script with
`OPENCAST_ACTION=run` and the chosen `arg` as `$1` (mode for that second run
comes from `@opencast.action_mode`, default `compact`). `filter = false`
re-runs the script on every keystroke with the query as `$1`
(`OPENCAST_ACTION=list`).

### Layouts by display topology

Each saved layout records a display signature (sorted display frames). When
restoring by name, the layout saved under the current signature wins; a layout
saved elsewhere still applies (clamped to the current screens). *Remember
Layout* with an existing name on a new display set adds a variant instead of
overwriting.

## Architecture notes

- Window management moves from Rust/JXA to Swift/AX (`macos/WindowManager.swift`).
  Core still decides (`Plan::Window`, `WindowAction::frac()`); exec returns
  `ExecResult::Window { action, frac }` and `ExecResult::Layout { op, apps }`.
  The hotkey path emits those results without a `show`.
  Rust `window_mgmt.rs` keeps only the pure geometry (`target_rect`), the Seek
  OCR scan and `click_at`.
- Layout capture flows Swift → Rust through a new `layoutSave` op so the
  store stays in core and testable.
- `Engine` now owns the sidecar state (`usage`, `pins`, `notes`) behind an
  `RwLock`, so search is pure CPU. Writers go through `Engine` methods.
- `Registry` keeps `title_lower` and an id → index map.
- Selection: Swift tries `kAXSelectedTextAttribute` on the focused element
  and passes `selection` with the `execute` request; Rust falls back to the
  CGEvent ⌘C marker trick only when that is empty.
- No new crates except `toml_edit` (config writes). AX/IOKit are reached
  from Swift, CGEvent from the existing `core-graphics` dep.

## Verification

- `cargo test` (core) and `cargo clippy -p opencast-core -- -D warnings`.
- `cargo clippy -p opencast-ffi -- -D warnings` on macOS.
- `./build-app.command`, then: snap a window with a hotkey (should be
  instant), paste a snippet, run an AI command on a selection in Safari and
  in VS Code, copy a password from 1Password and confirm it is not in
  Clipboard History, Quit OpenCast and confirm Caps Lock is restored.
