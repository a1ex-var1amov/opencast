# Parity gaps (B) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans or implement task-by-task in-session. Steps use checkbox syntax.

**Goal:** Ship emoji, deep links, pins, volume/brightness, system dark mode, plus clipboard persist, fallback quicklinks, quit-by-name, better file search.

**Architecture:** Sidecars (`pinned.json`, `clipboard.json`) + builtins + Info.plist URL scheme; reuse `PasteText` / `SystemShell` / `FillQuery`.

**Tech Stack:** Rust opencast-core/ffi, Swift AppKit panel.

## Global Constraints

- No new crate deps; opencast-core stays portable.
- No Settings UI / snippet editor / image clipboard.
- rustfmt max_width 120; no emoji in MD files.

---

### Task 1: Pins + usage-aware empty query

**Files:** Create `crates/opencast-core/src/pins.rs`; Modify `commands.rs`, `lib.rs`, `engine.rs` search; ffi `exec` bump optional; Swift Cmd+Shift+P + host op `pin`.

- [ ] `pins::{load,toggle,is_pinned,boost}` on `pinned.json`
- [ ] Empty query sorts pinned first; search score +500 when pinned
- [ ] Host op `pin` toggles id; Swift Cmd+Shift+P on selection
- [ ] `cargo test -p opencast-core` pins tests pass

### Task 2: System builtins (volume, brightness, dark mode, quit apps)

**Files:** `engine.rs` BUILTINS + plan; `exec.rs` already has SystemShell.

- [ ] Toggle Dark Mode, Volume Up/Down/Mute, Brightness Up/Down
- [ ] For each app: `quit:app:<path>` title `Quit {Name}`, kind Builtin or new, plan SystemShell quit by name; exclude from empty query (only when q matches quit/name)
- [ ] Tests for plan arms

### Task 3: Emoji picker

**Files:** Create `crates/opencast-core/src/emoji.rs`; wire into Registry search or Engine.search merge when query looks like emoji.

- [ ] Static ~200 entries; PasteText on run (`emoji:<name>` id)
- [ ] Test: search `emoji grin` finds grinning

### Task 4: File search content + fallback quicklinks

**Files:** `files_mac.rs`, `dispatch.rs`, `commands.rs` or engine search.

- [ ] `content:` / `in:` prefix; else length≥5 also content mdfind
- [ ] Append `{query}` quicklinks when &lt;3 non-calc hits

### Task 5: Clipboard persist + deep links

**Files:** `ClipboardHistory.swift`, `Info.plist`, `OpenCastApp.swift` AppDelegate.

- [ ] Load/save `clipboard.json` under config dir (path from settings or `~/.config/opencast`)
- [ ] CFBundleURLTypes `opencast`; handle `run/` and `search?q=`

### Task 6: Build + install

- [ ] `cargo test`; release build + install OpenCast.app
- [ ] `graphify update .`
