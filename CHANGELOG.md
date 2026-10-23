# Changelog

Roughly [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions are
git tags and match `Cargo.toml` plus `macos/Info.plist`. History before the
first tag is in `git log`.

## Unreleased

### Added
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

### Fixed
- `cargo test -p opencast-ffi --lib` failed on any machine with more built-in
  commands than the 60-row search limit.
- Typing a snippet keyword could expand a *shorter* keyword that matched its
  tail: with `;a` and `;ia` defined, `;ia` sometimes expanded `;a` and left an
  `i` behind, because a dictionary decided which one won. The longest match wins
  now.
