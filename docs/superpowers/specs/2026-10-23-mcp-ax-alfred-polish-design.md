# MCP OAuth, Dynamic Type, Alfred polish (2026-10-23)

Three parallel polish items: make remote MCP OAuth usable against real IdPs, scale the launcher with macOS text size, and sync Alfred import docs (Hotkey/Conditional already shipped).

## 1. MCP OAuth (remote HTTP)

### Problem
Ephemeral loopback ports could not be pre-registered; background connect opened the browser; `signedIn` ignored access-only tokens.

### Design
| Piece | Behavior |
|-------|----------|
| `oauth_redirect_uri` | Default `http://127.0.0.1:8765/callback`; bind that port; document registration with the IdP |
| Quiet token | Prefer Keychain access, else refresh — never interactive |
| Connect / Refresh tools | Quiet only; error tells user to Sign in… |
| Settings Sign in… / Re-auth… | Always interactive (`oauth_sign_in`) |
| Sign out | Deletes access + refresh Keychain items |
| `signedIn` | Access **or** refresh present |
| Doctor | Warn when OAuth configured but not signed in |
| Validate | `oauth_*` requires `url` |

### Non-goals
MCP OAuth 2.1 discovery / DCR, custom URL-scheme DeepLink, SecItem crate.

## 2. Dynamic Type (AX-2)

### Design
- `@ScaledMetric` on `LauncherView` for field / title / body / meta / caption / icon.
- `Model.typeScale` (body/14) scales row height, field chrome, footer, actions, island height.
- `theme_file.font_size` / `title_font_size` / `row_height` remain bases, multiplied by scale.
- Chat / Settings / Preview left for a later pass.

## 3. Alfred Hotkey + Conditional

### Status
Already implemented in `alfred.rs` (Hotkey → `# @opencast.hotkey` → in-memory `[[hotkeys]]`; Conditional → bash `_oc_cond`). Parity-status doc was stale.

### This pass
- Document as done in parity-status + README roadmap.
- Include `Fn` in `hotkey_string` modifier mask (`8_388_608`).

## Verification
- `cargo test -p opencast-core`
- `./test-swift.command`
- `./build-app.command`
