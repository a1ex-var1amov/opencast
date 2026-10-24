# Status of the parity and extension-compat plans (2026-10-21, 23:40)

Checks `raycast-parity-pass.md` and `raycast-extensions-compat.md` against
the code after the extensions-and-builtins pass. ✅ done · 🟡 partial ·
⬜ not started. "Done" means implemented and unit-tested in core where
possible; Swift parts are compiled only by `./build-app.command`.

> **This is a snapshot of one evening, not the current state.** Rows below say
> ⬜/🟡 for things that have since landed. What shipped afterwards, verified
> against the code: Restart / Shut Down / Log Out / Quit All Apps with
> confirmations, `[[ai_presets]]` + `profile.md` + `include` for shared config,
> the translator (`tr …`, `translate_to`), browser page context
> (`PageContext.swift`), model comparison (`CompareModels.swift`), emoji skin
> tones, the full window set (sixths, First/Last Two Thirds, Center Half,
> Maximize Height/Width, Reasonable Size, Move to edge, Toggle Fullscreen, Undo
> Window Move), region glyphs + live preview + HUD + drag-to-snap +
> `[[window_commands]]`, theme files (`theme_file`, `examples/themes/`), sticky
> notes (`NotesEditor.swift`), Focus with site blocking and a menu-bar timer,
> Dictate Anywhere, the "Keyboard Shortcuts" sheet (`CheatSheet.swift`),
> Universal Actions, the File Buffer, `alfred_*` variables for plain scripts,
> and OpenCast Doctor.
>
> For the surface as it is now read the README and `examples/config.toml` (they
> are the reference), and "Status and roadmap" in the README for what is next:
> MCP OAuth, browser history, Raycast React-extension compatibility and Linux
> packaging. (MCP resources/prompts, images to vision models and universal
> binaries have since landed.)

## Conflicts found and fixed tonight

- **Currency made network requests unasked.** Now opt-in:
  `[general] currency = false` by default, matching "no new network traffic
  unless the user turns it on".
- **Browse Extensions titles said "Raycast Script Commands · …".** Titles now
  describe the content ("App Controls (script commands)"); the row subtitle
  shows the source ("from github.com/raycast/script-commands").
- **README had no independence notice.** Added the About text from the compat
  doc's Decisions.

## raycast-parity-pass.md

### Part 1: feature gaps

| Item | Status | Notes |
|---|---|---|
| Per-command global hotkeys | 🟡 | `[[hotkeys]] key/command` already binds any command id or title. Missing: `hotkey` inside `[[commands]]`, "Record Hotkey" ⌘K action, conflict list in Settings |
| Shortcut cheat sheet | ⬜ | Menu Search (AX menu walk with shortcuts) exists to build on |
| Click Text leader line | ⬜ | |
| System-wide dictation | ⬜ | Dictation works in the search field and chat only |
| Emoji & symbols picker | 🟡 | `emoji …` search exists; no skin tone, no recents; grid view now available |
| System commands | 🟡 | Have: Lock, Sleep, Empty Trash, Eject All, Toggle Dark Mode, Volume, Brightness, Quit/Force Quit Front App, media keys, Settings panes. Missing: Restart, Shut Down, Log Out, Toggle Hidden Files, Quit All, Hide Others, Show Desktop, confirmations |
| Screenshot search | ✅ | Clipboard history OCRs screenshots; `shots …` searches them |
| AI presets | ⬜ | |
| AI profile / memory file | ⬜ | |
| Team / shared config (`include`) | ⬜ | |
| Currency (opt-in) | ✅ | `currency = true`; rates cached 12 h |
| Translator | ⬜ | Possible today as an `[[ai_commands]]` entry; no Translation framework |
| Browser page context | ⬜ | |
| Focus sessions | 🟡 | Apps hidden on activation, timer, footer countdown. Missing: domains/tab closing, goal, menu-bar timer, pause, presets |
| Model comparison | ⬜ | |
| Floating notes | 🟡 | Notes Editor window with search + autosave. Missing: float-on-top, hotkey, markdown rendering, multiple windows |
| Themes editor / gallery | ⬜ | Deprioritized by Alex |

### Part 2: window management UX

| Item | Status | Notes |
|---|---|---|
| Command set | 🟡 | Have: halves, quarters, thirds, Maximize, Almost Maximize, Center, Next/Previous Display, Restore. Missing: Center Half, First/Last Two Thirds, sixths, Maximize Height/Width, Reasonable Size, Larger/Smaller, Move to edge, Display N, Toggle Fullscreen, Undo |
| Region icons, live preview, HUD, picker | ⬜ | |
| Cycling, gaps, history/undo, layout-preserving display moves | ⬜ | |
| Fast AX writes (`AXEnhancedUserInterface` off while moving) | ✅ | Speed pass |
| Drag to snap, custom window commands, layouts UX | ⬜ | Layouts by display topology exist |

### Part 3: the best of Alfred

| Item | Status | Notes |
|---|---|---|
| Universal Actions | 🟡 | "Act on Selection" (bind a hotkey): text and Finder files. Missing: URL type, one action model shared with ⌘K, `accepts` for scripts/AI commands, per-type ranking, default hotkey |
| File navigation + File Buffer | ⬜ | |
| Alfred Script Filter JSON | ✅ | `items`, `arg` string/array, `icon.path`/`fileicon`, `valid`, `autocomplete`, `mods`, `quicklookurl`, `text.copy`/`largetype`, `variables` (item + top level), `rerun`. Ignored: `uid`, `type`, `match`, `skipknowledge` |
| `alfred_*` environment | 🟡 | Set for imported workflows only; plain scripts in the scripts folder don't get them yet |
| `.alfredworkflow` import | ✅ | Keyword, Script Filter, Hotkey triggers (`# @opencast.hotkey`), Run Script, Open URL, Open File, Reveal, Launch Apps, Copy to Clipboard (+ auto-paste), Notification, Large Type, Arg and Vars, Junction, Conditional (bash branches), Transform, modifier branches, Configure UI for userconfiguration defaults. Still skipped at runtime when reached: Delay, Replace, External Trigger, etc. |
| `.alfredsnippets` import | ✅ | Imports into `[[snippets]]` |
| Pipelines | ⬜ | |
| Rich script output | 🟡 | Per-item `markdown` detail pane and `view: "grid"` done; `@opencast.mode markdown` / `grid` header modes not |
| Large Type | ✅ | ⌘L and command |
| Spell & Define | 🟡 | Define done (`define …`); Spell not |
| Contacts | ✅ | Email, call, message, copy. Missing: Large Type number, email files |
| Recent documents per app, Shift-tap Quick Look, clipboard swatches / save as snippet, `sync_dir`, usage stats | ⬜ | iCloud sync exists as the one preset |

Extra, not in the plan: OpenCast Doctor + `smoke.command`, git-installed
script-command extensions, time zones, System Settings panes, bookmarks,
Reminders, Act on Selection file actions.

## raycast-extensions-compat.md

Nothing of the Node host is built yet (`core/ext/`, the
`@opencast/compat-raycast` shim, the react-reconciler, `ExtensionView.swift`,
app-managed Node). What exists that it will build on:

- **UI building blocks.** The richer list protocol already covers much of
  P1's rendering: rows with icons and subtitles, a markdown detail pane
  (`List.Item.Detail` / `Detail`), per-row actions with shortcuts
  (`ActionPanel`), a grid (`Grid`, planned for P2), argument forms (manifest
  `arguments`), the Keychain-backed Settings window (password preferences).
- **Install plumbing.** Extensions folder, Browse / Install / Update / Manage
  commands, sparse shallow git checkout of one folder (no full monorepo
  clone, no store servers), and an install that is reversible by deleting a
  folder.

To settle before P1:

1. **Same folder, two kinds.** Script-command extensions and future Node
   extensions both live in `~/.config/opencast/extensions/<name>/`. A
   `package.json` depending on `@raycast/api` should mark a Node extension;
   the script scan already skips `node_modules`, but it must skip Node
   extensions entirely so their helper scripts don't show up as commands.
2. **One Browse Extensions.** The command name is taken by the
   script-command browser. The `raycast/extensions` index should feed the
   same list, with a source label per row, rather than add a second browser.
3. **Install method.** The doc says to fetch a tarball of one extension
   directory. The script-command installer uses a sparse, blobless, depth-1
   checkout instead, which also only downloads that folder and keeps `git
   pull` updates. Pick one for both.
4. **Next concrete step:** the usage scan over `raycast/extensions` (symbols
   by weighted usage, coverage per implemented set) to fix the P1 cut.
