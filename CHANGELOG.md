# Changelog

Roughly [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions are
git tags and match `Cargo.toml` plus `macos/Info.plist`. History before the
first tag is in `git log`.

## Unreleased

### Added
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
