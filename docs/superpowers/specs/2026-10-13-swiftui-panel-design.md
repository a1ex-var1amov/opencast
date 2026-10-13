# SwiftUI panel, Rust engine

OpenCast's window becomes a SwiftUI app. The Rust core stays the engine. Node, React, Vite, and Tauri leave the build.

## Goal

`OpenCast.app` is a SwiftUI accessory app. It links one static library, `libopencast_ffi.a`. That library is the current shell without Tauri: config load, hotkeys, hyper key, clipboard, paste, selection grab, window snapping, OCR, and AI streaming. `opencast-core` stays portable and keeps its tests.

The launcher keeps the island it has now. It hangs from the top edge of the screen under the cursor, rounds only the bottom corners, and is pure black in dark mode so it meets the notch. The orange accent stays `#ff6b3d`.

## Non-goals

- Rewriting `opencast-core` in Swift.
- A Linux or Windows shell. The core crate still builds and tests anywhere.
- Desktop vibrancy. A blur through the panel fights the notch.
- A new accent, a new layout, or a settings window.
- An Xcode project. The app is `swiftc` plus an app bundle assembled by `build-app.command`.
- Sandboxing.

## Process

Swift owns `NSApplication` and the two panels. Rust owns every decision that is not drawing.

`opencast_start` runs on a background thread at launch. It loads the engine, starts a Tokio runtime, registers hotkeys, and installs the hyper-key tap when `hyperkey = true`. The first event is `show`, same as today's launch.

Swift calls Rust only through `opencast_call`. Rust calls Swift only through the callback passed to `opencast_start`. Both sides speak JSON. Strings Rust returns are malloc'd; Swift copies them and calls `opencast_free`. The callback's pointer is valid only until the callback returns; Swift copies it immediately.

## C ABI

Declared in `macos/opencast.h` and exported from crate `opencast-ffi` (`crate-type = ["staticlib"]`). No cbindgen.

```c
typedef void (*opencast_callback)(const char *json);
void opencast_start(opencast_callback cb);
char *opencast_call(const char *json);
void opencast_free(char *p);
```

`opencast_call` takes one object with an `op` field and returns one JSON object. Unknown `op` returns `{"error":"..."}`.

| op | fields | returns |
| --- | --- | --- |
| `search` | `query` | `{"commands":[Command...]}` using the existing camelCase `Command` serde |
| `execute` | `id`, `args` | one `ExecResult` (the same tagged enum the UI uses today) |
| `warnings` | | `{"warnings":[string]}` |
| `providers` | | `{"providers":[ProviderInfo...]}` |
| `stream` | `requestId`, `provider`, `model`, `system`, `messages` | `{"ok":true}` or `{"error":"..."}`. Tokens arrive on the callback |
| `cancel` | `requestId` | `{"ok":true}` |
| `copy` | `text` | `{"ok":true}` or `{"error":"..."}` |
| `paste` | `text` | `{"ok":true}` or `{"error":"..."}` |
| `seekClick` | `x`, `y` | `{"ok":true}` or `{"error":"..."}` |
| `seekClose` | | `{"ok":true}` |
| `reload` | | `{"text":"Reloaded · N commands"}` (same sentence as today) |

`ExecResult` stays `hide`, `toast`, `output`, `chat`, `error`, `args`, with the current camelCase fields.

Callback payloads, one JSON object each:

| type | when | payload |
| --- | --- | --- |
| `show` | hotkey toggle, hyper-key tap, launch | `{}` |
| `hide` | hotkey toggle while visible, and any Rust path that must return focus before continuing (selection grab, paste) | `{}` |
| `seek` | Click Text finished OCR | `{"scan": SeekScan}` with the existing `monitor` / `hits` / `words` shape |
| `result` | a hotkey ran a command | `{"result": ExecResult}` |
| `ai` | a stream token | `{"requestId","event"}` where `event` is the existing `ChatEvent` (`delta` / `done` / `error`) |

`hide` is the only callback that must finish its UI work before returning. Swift runs that work with `DispatchQueue.main.sync` when the callback is off the main thread, and inline when it is already on the main thread. Every other callback is `DispatchQueue.main.async`.

Swift never calls `opencast_start` or `opencast_call` on the main thread. Rust never invokes the callback while holding a lock the main thread needs. That is what keeps `hide` from deadlocking: the main thread is free to hide the panel and `NSApp`, then the Rust caller sleeps and copies the selection, as it does today.

A stream still ends with `done` or `error`. Cancelling aborts that task. If the provider stops without a terminal event, the library emits `done`, as the Tauri shell does now.

## What stays in Rust

Move `src-tauri/src/{hyperkey,platform,window_mgmt}.rs` and the command execution in `main.rs` into `opencast-ffi`. Delete the Tauri types on the way.

- Hotkeys use the `global-hotkey` crate (the library `tauri-plugin-global-shortcut` already wraps). Same config strings, including rewriting `Hyper+` to `Ctrl+Alt+Cmd+Shift+`. Registration problems are printed and do not abort launch.
- The hyper-key tap stays as it is. A bare tap emits `show` or `hide`.
- Clipboard, paste, and selection grab stay in Rust. They call the `hide` callback instead of hiding a webview.
- Snapping and layouts stay on `osascript` / JXA. Monitor frames come from Core Graphics (`CGDisplayBounds`) instead of Tauri's monitor list. Seek hit coordinates stay in the point space the OCR script already emits; the Seek panel is positioned from `scan.monitor` in that same space.
- `OPENCAST_KEEP_ON_BLUR` still means "do not dismiss when focus leaves". Swift reads it once and skips the resign-key hide.

`opencast-core` changes are limited to two subtitles, so the list stops repeating itself:

- Application commands get `subtitle: None`. The row's kind label already says Application. The id stays `app:` plus the bundle path.
- Window actions get `subtitle: None`. The title already names the action.

No current test asserts those two subtitles. Leave the tests alone unless one starts failing.

## SwiftUI

Two borderless `NSPanel`s. The app is `LSUIElement` (no Dock icon), activation policy accessory. Bundle id stays `dev.opencast.launcher`. Deployment target is macOS 14. That is a bump from the Tauri app's 11.0, so the panel can use current SwiftUI without availability branches.

The window background is clear. SwiftUI draws the island.

### Launcher panel

- Window level 25 (`NSStatusWindowLevel`), so it can sit over the menu bar.
- Width 760. Top edge flush with the top of the screen that contains the cursor. Horizontally centered on that screen.
- Bottom corners 18pt. Top corners square.
- Appears with a 0.16s move down from above the top edge. Height changes use the same duration.
- Light background `#fafafb`, dark background `#000000`. Hairline 0.5pt (black 18% light, white 14% dark) and the existing drop shadow.
- Becomes key on show, so the field takes typing. On hide, and on resign-key unless `OPENCAST_KEEP_ON_BLUR` is set, hide the panel and call `NSApp.hide()` so the previous app is frontmost. Do not hide the app while the Seek panel is visible.
- Esc on an empty search field hides. Esc with a query clears the query. Same as today.

Modes, same state machine as `App.tsx`: search, args, output, chat. A toast overlays the panel (1.3s, 4s on error) and can hide the app after a command toast.

Heights:

- Search: search bar 58 + list padding 12 + `min(max(rowCount, 1), 8) * 44` + footer 38. The Ask AI row counts. Past 8 rows the list scrolls.
- Args, output, and chat: 480.

### Search

- Plain text field, 19pt, no focus ring. Placeholder: `Search apps and commands, or ask AI…`
- Rows are 44pt, corner radius 9, icon column 24pt. Title, then subtitle when present, then a 12pt kind label.
- The selected row uses the accent at 12% opacity in light mode and 18% in dark mode (today's `--accent-soft`), not the flat gray.
- Arrow keys and Ctrl-N / Ctrl-P move the selection. Return runs. Tab asks AI with the query. Mouse move selects, click runs.
- Footer: `OpenCast` on the left, action hints on the right (`Return`, `Tab`). Kind labels stay: Application, AI Command, Script, Quicklink, Command, Calculator, Window, Layout.
- Empty query with no commands, and a query with no hits, show `No results`.

### Icons

- `kind == app`: the id is `app:` plus a bundle path. `NSWorkspace.shared.icon(forFile:)` at 24pt. Cache by path.
- Otherwise, if `icon` is one of the built-in strings below, draw that SF Symbol at 24pt in the secondary label color. Script, quicklink, and AI icons that the user set in config are not in this table and render as text.

The key is the exact string literal in `engine.rs`. Code points are listed so this file does not depend on the glyph. Center Third has no SF Symbol; that row draws U+2153 as text.

| code points | command | symbol |
| --- | --- | --- |
| U+1F4AC | AI Chat | `bubble.left` |
| U+1F504 | Reload Commands | `arrow.clockwise` |
| U+2699 U+FE0F | Open Config Folder | `gearshape` |
| U+1F4C2 | Open Scripts Folder | `folder` |
| U+23FB | Quit OpenCast | `power` |
| U+25E7 | Left Half | `rectangle.lefthalf.filled` |
| U+25E8 | Right Half | `rectangle.righthalf.filled` |
| U+2B12 | Top Half | `rectangle.tophalf.filled` |
| U+2B13 | Bottom Half | `rectangle.bottomhalf.filled` |
| U+25F0 | Top Left Quarter | `rectangle.inset.topleft.filled` |
| U+25F3 | Top Right Quarter | `rectangle.inset.topright.filled` |
| U+25F1 | Bottom Left Quarter | `rectangle.inset.bottomleft.filled` |
| U+25F2 | Bottom Right Quarter | `rectangle.inset.bottomright.filled` |
| U+2153 | First Third, Last Third | `rectangle.leadingthird.inset.filled`, `rectangle.trailingthird.inset.filled` |
| U+2153 | Center Third | none; draw the stored character |
| U+26F6 | Maximize | `arrow.up.left.and.arrow.down.right` |
| U+25CE | Center | `circle.circle` |
| U+1F3AF | Click Text | `viewfinder` |
| U+1F4F8 | Remember Layout | `camera` |
| U+1F5C2 | Restore Layout | `square.stack` |
| U+1F5D1 | Forget Layout | `trash` |
| U+2728 | AI command fallback | `sparkles` |
| U+1F4DC | script fallback | `scroll` |
| U+1F517 | quicklink fallback | `link` |

First Third and Last Third share U+2153 with Center Third. Distinguish them by command id (`window:first-third`, `window:center-third`, `window:last-third`), not by the icon string. A symbol `NSImage(systemSymbolName:accessibilityDescription:)` cannot resolve falls back to drawing the stored string, so a missing name never leaves a blank row.

### Args, output, chat

- Args: back button, command title, one field per argument, secure fields use a secure field. Return submits. Esc or Backspace in an empty first field goes back. Missing required field focuses that field.
- Output: back button, title, monospaced selectable text. Return or Cmd-C copies and toasts `Copied output`.
- Chat: back button, title `AI Chat`, two native menus (provider, model). Messages scroll. User text sits in an accent-tinted bubble. Assistant text uses `AttributedString` markdown. Fenced code blocks are split out first so each one keeps a copy button; the rest of the text goes through `AttributedString`. No markdown library. Composer is a plain field. Return sends, Shift-Return inserts a newline, Esc stops a stream or goes back, Cmd-Return pastes the last answer, Cmd-Shift-C copies it, Cmd-N clears the thread. Footer hints match today's chat footer. Streaming shows a caret block in the accent color where the answer is still empty.
- No providers: the same sentence as today, pointing at Open Config Folder and Reload Commands.

### Seek

Second transparent panel, same window level, frame equal to `scan.monitor`. Black pill, 480pt wide, 48pt tall, bottom radius 24, hanging from the top center. Accent `#ff6b3d` on the mark and on the hits.

Hits: accent fill at 50% opacity, 1.5pt accent stroke, 4pt corner radius. The current match is full opacity plus a 3pt accent glow. A 2pt accent line marks the matched span, using word boxes when present and an even split of the line when not (the `charX` logic in `Seek.tsx`).

Typing filters. Tab or `;` cycles. Return clicks: hide the Seek panel, call `NSApp.hide()` so the previous app is frontmost, wait 120ms, then `seekClick`. Esc closes the same way without a click. The count sits on the right of the pill.

## Build

`build-app.command` keeps the signing-identity lookup and the install to `~/Applications/OpenCast.app`. It stops calling npm.

1. `cargo build --release -p opencast-ffi`
2. `swiftc -parse-as-library -O -target arm64-apple-macos14.0 -import-objc-header macos/opencast.h` on `macos/**/*.swift`, linking `libopencast_ffi.a` and the native libs from `cargo rustc --print native-static-libs`.
3. Assemble `OpenCast.app` with `Info.plist` (`LSUIElement` true, bundle id, minimum system version 14.0, `NSScreenCaptureUsageDescription`, `NSAppleEventsUsageDescription`), and the existing `icon.icns`.
4. `codesign` with `APPLE_SIGNING_IDENTITY` when one exists, then copy into `~/Applications` and open it.

Dev run is `cargo build -p opencast-ffi` and a debug `swiftc` into `target/debug/OpenCast.app`. No browser mock. No `npm run dev`.

Workspace: `default-members` stays `opencast-core`, so `cargo test` at the root still does not need macOS frameworks. `opencast-ffi` is a member. On non-macOS its lib is an empty stub so `cargo test --workspace` still configures. The dispatch test is `cfg(target_os = "macos")`.

## Deletions

Remove after the Swift panel is the one `build-app.command` launches:

- `src/`, `index.html`, `seek.html`, `vite.config.ts`, `package.json`, `package-lock.json`, `tsconfig*.json`
- `src-tauri/` (Tauri config, capabilities, icons copied into the new bundle first)
- The `dist/` build output is already generated; stop producing it

Update `README.md` and `CLAUDE.md`: requirements are Rust and the Xcode command-line tools. Drop Node. Describe the Swift panel and the macOS-only shell.

## Tests

- Existing `opencast-core` tests, plus the subtitle assertions updated for the two `None` subtitles.
- One `opencast-ffi` test: `dispatch` of `{"op":"search","query":""}` against an engine built from `Engine::assemble` returns a JSON object with a `commands` array. That covers the call parser without a window.
- No Swift test target. The panel is checked by running the app: open with the hotkey, type, run a command, open chat, open Seek.

## Error handling

- Bad hotkey strings are warnings on stderr, launch continues, Alt+Space is the fallback for the toggle when the configured hotkey does not parse. Same as today.
- `opencast_call` never panics across the FFI. A Rust error becomes `{"error":"..."}` or an `ExecResult` of type `error`.
- AI and script failures surface as the toast or the chat error line, same copy as today.
- Accessibility and Screen Recording stay user-granted. The plist usage strings are what macOS shows for Apple Events and screen capture. A failed keystroke still tells the user to allow OpenCast under Accessibility.
