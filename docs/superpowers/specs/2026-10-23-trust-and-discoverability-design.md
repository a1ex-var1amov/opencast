# Trust and discoverability (2026-10-23)

Improve OpenCast where silent failures and TOML-only power features hurt daily use. Engineering cleanups that unblock that work ride along at the end.

## Goals

1. Dead or rejected hotkeys are visible in Settings (not only `NSLog`).
2. Unknown command `kind` strings never vanish from search.
3. Screen Recording loss after a rebuild is nudged, not discovered mid-OCR.
4. Settings deep-links into the real config surface without a full Settings rewrite.
5. Browser history joins bookmarks as a search surface.
6. Remote MCP can use OAuth tokens stored in Keychain (minimal flow).
7. Snippets can be added/edited without hand-writing TOML.
8. Hyper-key Caps Lock remap is restored even after hard crashes when possible.
9. `Model` / `engine.rs` god files are split so further work stays cheap.
10. Seek OCR uses in-process Vision (same path as clipboard).
11. One shared `url_encode`.
12. Swift tests cover Kind fallback and a thin `handleUi` smoke path.

## Non-goals

- Full Raycast React/Node extension host.
- Themes gallery.
- Universal Clipboard / Paste-as suite.
- Expanding Settings to mirror every `config.toml` key.
- New crate dependencies.

## Approach (ponytail ceilings)

| Item | Ceiling | Upgrade path |
|---|---|---|
| Hotkeys | Persist last `opencast_hotkeys_fix` lines on `SettingsState`; General pane list + flash on show if non-empty | Conflict scanner vs other apps |
| Kind | `case unknown` + keep row; raw kind in subtitle | Doctor count of unknowns |
| Screen Recording | On panel show / Settings open: if OCR features on and `!CGPreflightScreenCaptureAccess()`, one flash + offer Permissions tab | TCC watch |
| Settings discoverability | General: "Edit config.toml" (exists) + buttons that open file and `flash` the section name (`[[hotkeys]]`, `[[snippets]]`, …). Permissions: note rebuild clears Screen Recording | Real editors per section |
| Browser history | Mirror `bookmarks.rs`: Chrome-family History SQLite (copy then query) + Safari if readable; `CommandKind::Bookmark`-style or reuse File/Quicklink with subtitle `History · Chrome` | Incremental watcher |
| MCP OAuth | Config: `oauth_client_id`, `token_url`, `authorize_url`, optional secret via Keychain; browser open + local loopback callback; refresh via existing `headers_cmd` or in-process Bearer | Full MCP OAuth 2.1 + DCR |
| Snippets | Settings → General: list + add/edit/delete writing `[[snippets]]` via existing configSet/toml helpers | Dedicated Snippets tab |
| Hyper key | Already: quit, SIGTERM/INT/HUP, launch restore. Add: marker file under config dir while remapped; `atexit`; launch always clears marker+remap | LaunchAgent watchdog |
| Seek OCR | Replace JXA `osascript` with FFI/Swift Vision helper shared with clipboard | AX tree search |
| url_encode | `pipeline` calls `engine::url_encode` (or tiny `util`); Spotify keeps `%20` for spaces (URI, not form) | — |
| Model / engine split | Move WindowAction+tests out of engine; peel Chat/Seek/clipboard UI from OpenCastApp into sibling Swift files; no behavior change | Further peels |
| Tests | macos-tests: Kind unknown keeps row; one pure routing table for Ui action ids if extractable without AppKit | XCTest later |

## Success

- `cargo test` green; `./test-swift.command` green.
- Hotkey problems appear in Settings after a bad binding.
- A forged kind string still shows a search row.
- Seek no longer shells `osascript` for OCR.
- Browser history rows appear for at least Chrome Default profile when History file exists.
- MCP server with `oauth_*` can obtain and refresh a Bearer token into Keychain.
- Snippet add/delete works from Settings without opening an editor.
- `OpenCastApp.swift` and `engine.rs` line counts drop with behavior unchanged.
