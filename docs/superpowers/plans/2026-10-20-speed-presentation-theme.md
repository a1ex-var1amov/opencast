# Speed, presentation, and theme Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Full Spotlight app index, `presentation = island|spotlight`, system/light/dark theme with Mac accent, and hotkeys visible + searchable.

**Architecture:** Portable filters and config live in `opencast-core`. macOS `mdfind` and theme/presentation application live in ffi + Swift. Search JSON gains an optional `hotkey` field.

**Tech Stack:** Rust (`opencast-core`, `opencast-ffi`), SwiftUI panel, existing TOML config. No new crates.

## Global Constraints

- `opencast-core` stays portable (no AppKit).
- rustfmt `max_width = 120`.
- No new Cargo dependencies.
- Do not implement Proofr/grmr or notch-wrap wings in this plan.
- Do not commit unless the user asks.

---

### Task 1: Full macOS app index

**Files:**
- Modify: `crates/opencast-core/src/apps.rs`
- Modify: `crates/opencast-core/src/engine.rs` (`load` / `load_with_apps`)
- Create: `crates/opencast-ffi/src/apps_mac.rs`
- Modify: `crates/opencast-ffi/src/lib.rs`, `host.rs`, `exec.rs`

**Interfaces:**
- Produces: `apps::MacAppHint { path, display_name, bundle_name, background_only }`, `apps::filter_mac_apps(hints) -> Vec<AppEntry>`, `apps::parse_info_plist(src) -> (Option<String>, Option<String>, bool)`, `Engine::load_with_apps(dir, apps)`
- Consumes: existing `AppEntry`, `Engine::assemble`

- [ ] **Step 1: Core filter + plist helpers + tests in `apps.rs`**

```rust
#[derive(Debug, Clone)]
pub struct MacAppHint {
    pub path: PathBuf,
    pub display_name: Option<String>,
    pub bundle_name: Option<String>,
    pub background_only: bool,
}

pub fn is_nested_app(path: &Path) -> bool {
    let mut comps = path.components().peekable();
    while let Some(c) = comps.next() {
        let s = c.as_os_str().to_string_lossy();
        if s.ends_with(".app") && comps.peek().is_some() {
            return true;
        }
    }
    false
}

pub fn filter_mac_apps(hints: Vec<MacAppHint>) -> Vec<AppEntry> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for h in hints {
        if h.background_only || is_nested_app(&h.path) {
            continue;
        }
        if h.path.extension().and_then(|x| x.to_str()) != Some("app") {
            continue;
        }
        let name = h
            .display_name
            .filter(|s| !s.is_empty())
            .or(h.bundle_name.filter(|s| !s.is_empty()))
            .or_else(|| h.path.file_stem().and_then(|s| s.to_str()).map(str::to_string))
            .unwrap_or_default();
        if name.is_empty() || !seen.insert(h.path.clone()) {
            continue;
        }
        out.push(AppEntry { name, path: h.path });
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// CFBundleDisplayName, CFBundleName, LSBackgroundOnly from an Info.plist XML body.
pub fn parse_info_plist(src: &str) -> (Option<String>, Option<String>, bool) {
    (
        plist_string(src, "CFBundleDisplayName"),
        plist_string(src, "CFBundleName"),
        plist_bool(src, "LSBackgroundOnly").unwrap_or(false),
    )
}
```

`plist_string` / `plist_bool` scan for `<key>…</key>` then the next `<string>` / `<true/>` / `<false/>`. Unit-test nested path, background skip, name preference, and a fixture plist.

- [ ] **Step 2: `Engine::load_with_apps`**

```rust
pub fn load(config_dir: &Path) -> Self {
    Self::load_with_apps(config_dir, apps::scan())
}

pub fn load_with_apps(config_dir: &Path, apps: Vec<AppEntry>) -> Self {
    // same as current load, but use `apps` instead of apps::scan()
}
```

- [ ] **Step 3: `apps_mac.rs` in ffi**

`mdfind` one query, for each path read `Contents/Info.plist`, build `MacAppHint`, `filter_mac_apps`. If mdfind fails or yields empty after filter, `apps::scan()`.

- [ ] **Step 4: Wire `host::install` and `exec::reload` through `load_with_apps` + `discover()`.**

- [ ] **Step 5: `cargo test -p opencast-core` and `cargo test -p opencast-ffi --lib`.**

---

### Task 2: Presentation island | spotlight

**Files:**
- Modify: `crates/opencast-core/src/config.rs` (`General.presentation`)
- Modify: `examples/config.toml`
- Modify: `crates/opencast-ffi/src/dispatch.rs` (emit presentation on a small `config` op or include in search bootstrap)
- Modify: `macos/OpenCastApp.swift` (`frame(on:height:)`)

**Interfaces:**
- Produces: `general.presentation: String` default `"island"`; Swift reads via new op `{"op":"settings"}` → `{ theme, presentation }`
- Consumes: existing panel frame math

- [ ] **Step 1: Config field**

```rust
#[serde(default = "default_presentation")]
pub presentation: String, // "island" | "spotlight"
fn default_presentation() -> String { "island".into() }
```

Validate unknown → warning in `Engine::assemble` or `Config::validate` (warn path: push to warnings in assemble if not island/spotlight).

- [ ] **Step 2: dispatch `settings` op** returning theme + presentation from engine config.

- [ ] **Step 3: Swift** on show and after reload: fetch settings; `presentation == "spotlight"` centers vertically:

```swift
y: screen.frame.midY - height / 2 - room / 2  // tor island: maxY - height - room
```

- [ ] **Step 4: Document in `examples/config.toml`.**

---

### Task 3: Theme system | light | dark + Mac accent

**Files:**
- Modify: `config.rs`, `engine.rs` (builtins + `Plan::SetTheme`), `exec.rs`, `examples/config.toml`
- Modify: `macos/Theme.swift`, `OpenCastApp.swift`, `LauncherView.swift`, `ChatView.swift` as needed
- Add: config write helper in core `config::set_general_key(dir, key, value)`

**Interfaces:**
- Produces: `theme` config; builtins `theme-system|light|dark`; `Plan::SetTheme(&'static str)`; Swift `colorScheme` override + `Theme.accent` from `NSColor.controlAccentColor`

- [ ] **Step 1: `set_general_key` in config.rs** — replace `key = "..."` under `[general]` or insert after `[general]` line. Unit test with tempfile.

- [ ] **Step 2: Builtins + plan + exec apply** write theme, emit `{"type":"settings","theme":…}`, return Hide false / toast.

- [ ] **Step 3: Swift** apply `preferredColorScheme`; observe `AppleAquaColorVariant` / distributed notification for accent; `Theme.accent` reads control accent.

---

### Task 4: Hotkey on rows + Shortcuts command

**Files:**
- Modify: `commands.rs` (`hotkey: Option<String>` on Command, serde)
- Modify: `engine.rs` (fill hotkeys after assemble; shortcut builtins; Shortcuts builtin)
- Modify: `macos/Models.swift`, `LauncherView.swift`
- Modify: `OpenCastApp.swift` (handle shortcut:* and Shortcuts)

**Interfaces:**
- Produces: `Command.hotkey`; ids `shortcut:return|tab|backtick|esc|arrows`; `builtin:shortcuts`

- [ ] **Step 1: After building commands, map id→key from `hotkey_bindings()` and set `command.hotkey`. Add key to keywords.**

- [ ] **Step 2: Register shortcut rows + Shortcuts builtin. `plan` for `builtin:shortcuts` → new `Plan::FillQuery("shortcut".into())` or handle in Swift via result type. Prefer exec emits `{"type":"query","query":"shortcut"}` and returns show.**

- [ ] **Step 3: Swift rows show hotkey; Accept `query` event; shortcut activations call existing startClick / openChat / escape.**

- [ ] **Step 4: Tests for hotkey field and Shortcuts plan.**

---

### Task 5: Build and install

- [ ] `cargo test -p opencast-core && cargo test -p opencast-ffi --lib`
- [ ] Release build + swiftc + ad-hoc sign + ditto to `~/Applications/OpenCast.app`
- [ ] Smoke: open launcher, confirm more apps than before, toggle presentation in config + reload, Appearance commands, hotkey visible if bound.

## Spec coverage

| Spec item | Task |
|-----------|------|
| mdfind + filter + fallback | 1 |
| load_with_apps | 1 |
| presentation island/spotlight | 2 |
| theme system/light/dark + accent | 3 |
| hotkey on rows + Shortcuts | 4 |
| build/install | 5 |
