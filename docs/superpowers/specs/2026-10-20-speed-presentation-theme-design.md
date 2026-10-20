# Speed, presentation, and theme

Three product cuts. OpenCast stays a launcher; Proofr stays out of this spec.

## 1. Speed and full app index

macOS app discovery leaves the one-level directory scan. The ffi host runs Spotlight:

```
mdfind 'kMDItemContentTypeTree == "com.apple.application-bundle"'
```

A pure filter in `opencast-core` (unit tested) decides the list:

- Keep a path only if it ends in `.app` and no earlier component ends in `.app`.
- Skip `LSBackgroundOnly`. Keep `LSUIElement`.
- Name is `CFBundleDisplayName`, then `CFBundleName`, then the file stem.
- One row per path. `Reload Commands` rescans.

If Spotlight returns nothing, fall back to the existing directory scan so a fresh Mac still lists `/Applications`.

`Engine::load_with_apps` lets the ffi host pass that list. `Engine::load` keeps calling `apps::scan()` for tests and non-mac builds.

Search stays in-process after load. No new deps. No second search index on the Swift side in this cut.

## 2. Presentation: island or Spotlight

`[general]` gains:

```toml
presentation = "island"   # or "spotlight"
```

Default `island`: current top-centered 760pt panel under the pointer screen.

`spotlight`: same width, vertically centered on that screen (Spotlight-like), list window still eight rows. Notch wrap from the earlier island spec is not this cut.

Changing presentation in config needs Reload Commands (or an Appearance-style apply if we wire it later). For this cut, Reload is enough; the value is read when the panel shows and when content resizes.

## 3. Theme and hotkey map

`[general]` gains:

```toml
theme = "system"   # or "light" or "dark"
```

Default `system`. Unknown values warn and behave as `system`.

The island, chat, and forms follow that choice. Selected rows, Ask AI, and click boxes use the Mac accent color (`NSColor.controlAccentColor`), updating when the system accent changes.

Three builtins write `theme` under `[general]` and apply immediately without a full re-scan of apps if possible; Reload still works:

- `builtin:theme-system` — Appearance: System
- `builtin:theme-light` — Appearance: Light
- `builtin:theme-dark` — Appearance: Dark

Writing theme edits only the `theme` line (insert or replace under `[general]`); the rest of `config.toml` is unchanged.

### Hotkeys on rows

Each search result may carry `hotkey: string | null` (camelCase in JSON). The engine fills it from resolved `[[hotkeys]]` bindings (and the launcher toggle key is not shown on every row). Swift shows the key on the right of the row when present. Keywords include the key string so typing `ctrl+alt+left` finds that command.

### Searchable shortcuts map

Builtin `builtin:shortcuts` — title **Shortcuts**, keywords `hotkey`, `keys`, `keymap`, `tab`, `click`, `` ` ``. Running it sets the search query to a fixed prefix that surfaces the in-app key rows (or simply fills `query` with `shortcut` and relies on keyword matches). Prefer: running Shortcuts clears to mode search and sets `query` to `↑` is wrong — set query to empty and inject synthetic commands? Lazier: add static builtin-like commands that are always in the registry:

| id | title | keywords | action |
|----|-------|----------|--------|
| `shortcut:return` | Return — Run selected | return, enter | no-op toast or activate selected |
| `shortcut:tab` | Tab — Ask AI | tab, ask | open chat with current query |
| `shortcut:backtick` | Backtick — Click Text | `, click, seek | start click |
| `shortcut:esc` | Esc — Back / hide | escape | escape behavior |
| `shortcut:arrows` | Arrows — Move selection | up, down | toast only |

Running **Shortcuts** sets `query` to `shortcut` so those rows float up. Rows with a real action run it; informational ones flash a one-line hint and stay open.

Global hotkey strings already appear on the bound command rows via `hotkey`.
