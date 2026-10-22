# Extensions, Alfred import, richer lists and built-ins (2026-10-21)

Follow-up to the beyond-Raycast pass. Goal: close the remaining gaps with
Raycast Pro, Alfred and Spotlight without new crate dependencies.

## Health
- `doctor.rs`: file sizes (config > 256 KB warns, > 5 MB is refused at load),
  any line repeated 20+ times in a TOML file, engine warnings, providers
  without keys, MCP servers with no cached tools, search p50/p95 with the
  user's real apps and history. Launcher command **OpenCast Doctor**; terminal
  `cargo run --release -p opencast-core --example doctor`; `smoke.command`.
- Root cause it would have caught: the pre-0.2 `set_general_key` duplicated
  everything above `[general]` on each write (2^24 headers, 2.7 GB).
  `set_general_key` now refuses to write a file that grew by more than one key.

## Extensions
- Plain git clones in `<config>/extensions/<name>/` with a small manifest
  (`.opencast-extension.json`: source, subpath, branch). Sources: `owner/repo`,
  `owner/repo/sub/folder` (sparse checkout), GitHub `tree` URLs, any git URL.
- Discovery walks each extension (depth 5, skipping `.git`, `images`,
  `node_modules`…) with the normal script parser, so Raycast script commands
  work unchanged; the extension name becomes the package/subtitle.
- Browse: curated Raycast script-command folders + GitHub topic
  `opencast-extension`. Update = `git pull --ff-only`. Remove = delete folder.
- Trust: extensions run code; Browse says so. No auto-update.

## Alfred workflows
- `alfred.rs` turns Keyword and Script Filter inputs into generated bash
  commands under `<workflow>/opencast-commands/`. Each runs the connected
  object chain as bash functions with Alfred's env (`alfred_workflow_data`,
  `_cache`, bundle id, workflow variables and user-config defaults).
- `{query}` is substituted with awk from the environment (never `eval`d), with
  Alfred-style escaping for shell vs. other interpreters.
- Modifier connections map to `OPENCAST_MOD` (set by the panel from the
  action's `cmd+return`-style key).
- Unsupported object types print a clear message when reached.
- macOS glue (`extensions_mac.rs`): `ditto -x -k` for the zip, `plutil` for
  the plist. Finder › Open With registers `.alfredworkflow`.

## Richer Script Filter output
- Items: `markdown` (detail pane), `actions` (⌘K + their own keys), `quicklook`,
  `copy`, `valid`, `variables`, image icons (`/path`, Alfred `{path}`,
  `fileicon:`); Alfred `mods`, `text.copy`, `text.largetype`, `quicklookurl`.
- Response: `view: "grid"`, `columns`, `rerun` (seconds), top-level
  `variables`. Item variables are passed to the run step as env.
- Panel-built lists (Contacts, Reminders, Selection) use the same rows with a
  local action handler.

## Built-ins
- Core (tested): currency (`open.er-api.com`, cached 12 h in `rates.json`),
  time zones (TZif parser + POSIX footer rules over `/usr/share/zoneinfo`,
  parsed zones cached in the engine), System Settings panes, bookmarks
  (Chromium JSON, Safari plist via plutil, Arc sidebar), `define …` row.
- Swift: Contacts (CNContactStore), Reminders (EventKit, NSDataDetector for
  dates), dictionary (DCSCopyTextDefinition), media keys (NX_KEYTYPE events),
  Large Type window, Act on Selection (AX text or Finder files), Focus sessions
  (hide `[focus]` apps on activation), Notes editor window (autosave).
- Startup: the panel's first show now waits for settings (it used to appear in
  island position before the presentation setting arrived).

## Not done
Browser history (needs sqlite via `/usr/bin/sqlite3`), website blocking in
Focus (needs root), Raycast React extensions, OAuth MCP, vision models.
