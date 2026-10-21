# Parity gaps + cheap partials (option B)

Approved scope: close remaining Raycast OOTB **Gaps**, plus cheap **Partial** thickeners. No store extensions, no Settings UI, no snippet editor UI, no image clipboard, no extra menubar apps.

Approach: sidecars next to config + builtins (same pattern as Stay Awake / clipboard).

## Gaps

### Emoji picker
- Searchable when query is `emoji` / `emo` / `emoji <needle>`.
- Static list of ~200 common emoji (title = glyph + short name; keywords = aliases).
- Return → `PasteText` (same path as snippets).
- No Character Viewer dependency.

### Deep links
- Info.plist URL scheme: `opencast`.
- `opencast://run/<command-id>` — execute that id (URL-decoded).
- `opencast://search?q=…` — show launcher with query filled.
- Handle in AppDelegate `application(_:open:)`.

### Favorites / pinned
- Store: `~/.config/opencast/pinned.json` — array of command ids.
- Toggle: searchable **Pin Selection** is awkward; instead:
  - Cmd+Shift+P on a selected row toggles pin (Swift-only key).
  - Builtin **Pinned** fills query that surfaces pinned (optional; empty query already shows them first).
- Empty query: pinned first (before chat / usage).
- Non-empty: large score boost for pinned ids (+500).
- No separate pin UI.

### Volume / brightness
- Builtins (SystemShell / keystroke):
  - Volume Up / Down / Mute (toggle)
  - Brightness Up / Down
- Prefer `osascript` volume; brightness via System Events key codes (F1/F2 style) where possible.
- Keywords: `volume`, `mute`, `brightness`, `display`.

### System Dark Mode
- Builtin **Toggle Dark Mode** — toggles macOS appearance (not OpenCast theme).
- Keep existing Appearance: System/Light/Dark for the panel only.

## Cheap partials

### Clipboard persistence
- On change and on shutdown: write text entries to `clipboard.json` in config dir.
- On install: load (cap 40, max 8k chars each). Still text-only.

### Fallback search
- When query is nonempty and engine returned fewer than N “strong” hits (e.g. &lt; 3 non-calculator), append quicklinks whose URL contains `{query}` with the query substituted in the subtitle/title for display; executing still goes through existing quicklink plan.
- No new config keys if quicklinks already exist.

### Quit app by name
- Builtin **Quit App…** → FillQuery `quit ` or generate `quit:<app-path>` style commands for running/installed apps is heavy.
- Lazy: for each indexed app, also register `quit:app:<path>` command title `Quit {Name}` with keywords `quit`, `close` — only shown when query matches `quit` category or title. Cap: reuse existing app list; don’t double empty-query noise (exclude from empty query via kind rank / require query).
- Plan: `osascript` quit that app by name.

### File search
- Keep `mdfind -name`.
- If query starts with `content:` (or `in:`), strip prefix and run content mdfind; else if needle length ≥ 5, also merge a small content pass (dedupe by path).

## Out of scope
- Snippet auto-expand / editor UI
- Full Settings UI
- Clipboard images / files
- Richer menubar extras beyond Stay Awake
- Extension store

## Success
- Each Gap is reachable from search or a deep link without editing TOML (except pins persist in json).
- Partials improve daily use without new UI surfaces.
- `cargo test` green; app builds and installs.
