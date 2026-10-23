# Changelog

Roughly [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions are
git tags and match `Cargo.toml` plus `macos/Info.plist`. History before the
first tag is in `git log`.

## Unreleased

### Added
- **Integrations, detachable as a unit**: `integrations = "auto"` (the default)
  registers an app bridge only when its app is installed, `"off"` drops its
  commands, its app-row actions and its hotkeys together, and a list forces one
  in or out (`"spotify"`, `"auto, -spotify"`). A name that does not exist warns
  instead of quietly doing nothing, and Doctor lists what is live.
- **Spotify Search**: name an artist, track, album or playlist and it opens in
  Spotify's own search. The app has no search command over Apple Events, so this
  travels by its deep link — no API credentials, no scraping.
- **An integration puts its verbs on the app row**: ⌘K on the **Spotify** app row
  offers Player Controls…, Play / Pause, Previous Track, Next Track, Now Playing
  and Search in Spotify…, so you do not need to know the command names. The engine
  attaches them, which is what makes switching the integration off take them away
  too.
- **Spotify, driven directly.** The media-key commands follow whatever owns Now
  Playing — a browser tab counts — and read nothing back, so *Next Track* could
  skip a video and *what is playing?* had no answer. **Spotify** opens a live list:
  the current track and where you are in it, Play/Pause, Next, Previous, Seek
  ±10s, Volume Up/Down, Now Playing (Return copies the track line) and Open
  Spotify; the list re-reads after each press, so Next twice is two presses.
  **Spotify Play / Pause**, **Spotify Next Track**, **Spotify Previous Track** and
  **Spotify Now Playing** are commands too, ready for a hotkey. No Shuffle or
  Repeat row: this Spotify takes `set shuffling` and ignores it, and its own menu
  has no such item to press, so the launcher does not pretend. Spotify starts when
  it was not up, and the first use asks the usual automation question.
- **Keyboard Shortcuts drills down**: the command lists the running apps (the
  one you were in first), Return opens that app's shortcut list with the key
  equivalent in the hotkey column, and Return there activates the app and
  presses the menu item. Typing filters at every level, Esc steps back a level,
  and **Keyboard Shortcuts** is also an action on any running app's row. The
  separate floating sheet is gone: the launcher list does the same job and can
  run what it shows.
- **Spaces** (Mission Control desktops): **Switch Space** lists every desktop
  with the apps that live on it — Return moves the view there, and each desktop
  also gets a **Move Window to Space N** row that takes the window along. **Next
  Space**, **Previous Space**, **Move Window to Next/Previous Space** are
  commands too, and `window_hotkeys` presets now combine with a comma:
  `"rectangle, spaces"` adds ⌃⌥⇧ + arrows. The `spectacle` set claims those two
  for make-smaller / make-larger, so the preset written first keeps them and the
  launcher reports the command left keyless. A full-screen window owns a space of
  its own: the list shows it, and moving a window skips past those. Switching
  posts the Dock swipe the system expects (with the IOHID payload macOS 27
  demands); moving uses SkyLight's own operation. Neither needs Accessibility.
- **Rectangle's window hotkeys, in one line**: `window_hotkeys = "rectangle"` in
  `[general]` installs ⌃⌥ + arrows for the halves, U I J K for the quarters, D E
  F T G for the thirds, ⌃⌥⌅ maximize, ⌃⌥⌫ put it back, ⌃⌥= / ⌃⌥- bigger and
  smaller, ⌃⌥⌘ + arrows for displays. `"spectacle"` gives the ⌥⌘ set Rectangle
  registers out of the box; `"off"` (the code default, and what configs written
  before this keep) binds nothing, as before. Keys you bind yourself — in
  `[[hotkeys]]` or by recording one in the Window tab — replace just that preset
  entry. New installs get the Rectangle set from the example config.
- **Full Screen** and **Exit Full Screen** window commands next to **Toggle
  Fullscreen**. Bind them to different hotkeys (⌃⌘F in, and whatever gets you
  out), and pressing one when it does not apply says so — “that window is
  already full screen” — instead of quietly doing the opposite.
- MCP servers contribute more than tools. A server that advertises **resources**
  gets one command per document (Return reads it into the panel; the URI and MIME
  type show in the preview); one that advertises **prompt templates** gets a
  command per template — fill its arguments and the server's own wording opens in
  a chat, unsent, ready to edit or attach something to. Listed once by **Refresh
  MCP Tools** and cached in `mcp-extras.json`, so searching never connects.
  Servers without those capabilities are unaffected.
- Image attachments reach vision models as pixels. Screenshots, clipboard images
  and image files are downscaled and sent to `gpt-4o`, `claude…`, `gemini…`,
  `…-vl`, `pixtral…` and friends; every other model keeps getting on-device OCR
  text, so attachments work with a local llama too. Guessed from the model name,
  forced with `vision = true|false` per provider.
- `opencast://search?q=…` and `opencast://run/<command-id>` deep links, plus
  opening `.alfredworkflow` / `.alfredsnippets` files from Finder (documented
  in README).
- Unknown keys and section names in `config.toml` are reported as warnings
  (`OpenCast Doctor`, and the launcher after a config reload) instead of being
  silently ignored. Suggestions included: `scipt_timeout_secs` → “did you mean
  `script_timeout_secs`?”.
- OpenCast refuses to start a second time: a lock file in the config folder, and
  an alert explaining why the new copy quit.
- The Homebrew cask is printed ready to paste (version, sha256 and owner/repo
  from your git remote) instead of a template full of `OWNER` placeholders.
- Panel logic has tests: `./test-swift.command` compiles the pure panel files
  with an assert-style runner (no Xcode project, no XCTest) and CI runs it.
  30 checks cover deep links, meeting-link detection and snippet matching —
  logic now extracted into `DeepLink.swift`, `MeetingLinks.swift` and
  `SnippetMatch.swift`.

### Changed
- An empty launcher shows a mix of kinds instead of one wall of window commands:
  at most 12 window and 16 built-in rows, with apps and Recents kept. Anything
  you pinned, boosted or actually use is exempt from the cap.
- AI requests are dropped after 120 s without a single byte (a wedged local
  server or tunnel no longer holds the panel open forever). Long answers that
  keep streaming are unaffected.
- The app is built universal: `./release.command` now produces one dmg that runs
  on Intel and Apple Silicon Macs (`OC_ARCH=arm64` for a single slice),
  `OC_ARCH=universal ./build-app.command` does the same locally, and CI compiles
  the Intel slice so it cannot rot unnoticed. The two build scripts now share
  `packaging/build-arch.zsh`.
- CI runs `cargo fmt --all -- --check`, and the Swift type check now passes the
  engine header (`macos/opencast.h`), so `opencast_call` is actually checked.
- Typing inside a panel-side list (menu items, spaces, schedule, Script Filters)
  is fuzzy rather than a substring test: `nbrw` finds **New Browser Window**, the
  best match moves to the top, and a list the author already ranked (spaces,
  Recents) keeps its order when nothing is typed.

### Fixed
- **MCP resources and prompts never reached the panel.** Core serialized their
  kinds as `mcpresource` and `mcpprompt`, and the panel's kind enum could not
  decode either one, so those rows were dropped before display: the engine had
  them, the launcher showed none. Both kinds decode now, share the MCP glyph, and
  get their own actions (Read Resource, Open in Chat).
- **The launcher sometimes never appeared**: the clipboard history indexes the
  screenshot folders, and Desktop is behind a permission prompt that an accessory
  app may never get to answer. Listing and opening those folders happened on the
  main thread, which is also where hotkey registration waits — so the app started,
  showed nothing and answered nothing. The folder work moved off the main thread,
  and the wait for it now has a timeout instead of lasting forever.
- **Hotkeys went dead after a reload**: Carbon refuses a second event handler on a
  target while the previous one is alive, and the manager was built before the old
  one was torn down. Every re-registration after the first — **Reload Commands**, a
  recorded hotkey, a config edit — left the keys doing nothing. The old manager is
  retired first now, and the panel re-registers once more a few seconds after
  launch, so a key that another app had at startup gets picked up when it lets go.
- **A crash on the way up**: the clipboard history asked for the config directory
  before the engine existed, which panicked on some machines.
- **Bookmarks that did nothing**: Chrome, Brave, Edge and Safari keep their
  bookmark stores behind Full Disk Access. The refresh skipped them, the cache
  stayed empty and the launcher showed a search with no results and no reason.
  The **Bookmarks** command now says which stores exist and that Full Disk Access
  is missing, Doctor reports it as a warning, and the refresh logs the blocked
  store names at startup.
- `cargo test -p opencast-ffi --lib` failed on any machine with more built-in
  commands than the 60-row search limit.
- Typing a snippet keyword could expand a *shorter* keyword that matched its
  tail: with `;a` and `;ia` defined, `;ia` sometimes expanded `;a` and left an
  `i` behind, because a dictionary decided which one won. The longest match wins
  now.
