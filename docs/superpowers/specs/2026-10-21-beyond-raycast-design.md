# Beyond Raycast: ranking, actions, AI tools, and the daily-driver gaps

Follows `2026-10-20-speed-and-customization-design.md`. Goal: close the gaps
with Raycast Pro and Spotlight (macOS Tahoe) that matter day to day, keep the
advantages (any AI provider, local-first, file-based config), and ship it so
other people can install it. Themes are out of scope for this pass.

## What shipped

| Area | Feature | Where |
|------|---------|-------|
| Ranking | Learns which result you pick for a query (Alfred-style), frecency (recency-faded counts), acronyms (`vsc`), Return waits for the in-flight search | `core/usage.rs`, `core/commands.rs`, Swift `pendingReturn` |
| Actions | Per-type ⌘K actions with their own shortcuts that also work straight from the list (⌘⇧F Show in Finder, ⌘Y Quick Look, ⌘O Open With…, ⌘⌫ Trash, ⌘T Terminal, ⌘↩ Run in Chat…) | `OpenCastApp.swift` `actionRows` |
| Preview | Right-hand pane for files, clipboard, snippets, notes, chats, AI commands, scripts, MCP tools, quicklinks; ⌘I toggles; ⌘Y Quick Look | `Preview.swift`, `Engine::preview` |
| Shortcuts.app | Every shortcut is a command; run with clipboard as input; output shown | `ffi/shortcuts_mac.rs`, `sc:` ids |
| MCP | `[[mcp_servers]]` (stdio + Streamable HTTP). Tools become launcher commands (argument form from the JSON schema) and AI-chat tools | `core/mcp.rs`, `ffi/agent.rs` |
| AI tools | Tool calling in all three wire formats (OpenAI-compatible, Anthropic, Gemini); multi-round loop (max 8); approval prompt unless `auto_approve` | `providers/*.rs`, `agent.rs` |
| Chats | Saved after every answer, `chats …` to search, Return resumes with provider/model; preview shows transcript | `core/chats.rs` |
| Attachments | ⌘O file (text, PDF, images via OCR), ⌘⇧V clipboard, ⌘⇧S screen text; on-device extraction | `Attachments.swift` |
| Snippets | Expand while typing in any app (`;sig`), placeholders `{date} {time} {datetime} {clipboard} {cursor}` | `SnippetExpander.swift`, `exec::expand_with_cursor` |
| Menu search | "Search Menu Items" lists the front app's menus with their shortcuts; Return presses | `MenuSearch.swift` |
| Dictation | ⌘D in the field or chat; on-device recognition when supported; stops after 2.5 s of silence | `Dictation.swift` |
| Calendar | "My Schedule" + a Join/Next row on the empty launcher when a meeting is within the hour; detects Zoom/Meet/Teams/Webex… links | `Schedule.swift` |
| Sync | "Sync Config with iCloud Drive" moves config.toml, scripts, notes, layouts, pins to iCloud and symlinks them; private/machine files stay local; second Mac adopts | `core/sync.rs` |
| Auto-reload | config.toml / scripts changes reload automatically (watches through symlinks) | `ConfigWatcher.swift` |
| Settings | Window with General, AI (add providers; keys into the Keychain readable by `security` without prompts), Extensions (MCP, scripts, sync), Permissions (status + request), About (updates) | `SettingsWindow.swift`, ops `configGet/configSet/providerAdd/providerRemove` |
| Distribution | `release.command`: Developer ID signing with hardened runtime + entitlements, notarization, stapling, dmg + zip, cask sha; Homebrew cask template; Check for Updates (manual, or weekly opt-in) | `release.command`, `packaging/` |

## Design decisions

- **MCP instead of an extension store.** The ecosystem already exists and
  fits bring-your-own-AI. Tool lists are cached (`mcp-tools.json`) so the
  launcher never waits for servers at startup; servers start on first use and
  are restarted when they die. Tools are namespaced `server__tool` for models.
- **Approval by default.** MCP tools can write. The chat asks before every
  call unless the server's `auto_approve` lists it (prefix globs); a declined
  call tells the model not to retry. Launcher runs are explicit, so they do
  not ask.
- **Ranking signals are capped** so a favourite never beats a precise match:
  learned picks up to +800 for an exact query, +300 for a remembered longer
  query that starts with what you typed; frecency up to +240.
- **Snippet expansion only for symbol-prefixed keywords** by default
  (`;sig`), so plain words never get replaced. The pasted text is marked
  `TransientType` so clipboard history skips it, and the clipboard is restored.
- **Keychain items are created with an access list** that includes
  `/usr/bin/security`, so the `api_key_cmd` the settings window writes reads
  the key without a prompt.
- **Nothing phones home.** Update checks run only when asked or when
  `check_updates = true`; the background check never opens anything.

## Config surface added

```toml
[general]
shortcuts = true
snippet_expansion = true
calendar = true
check_updates = false
update_repo = "owner/repo"

[keys]
dictate = "Cmd+D"
quick_look = "Cmd+Y"
preview = "Cmd+I"

[[mcp_servers]]
id = "github"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
env_cmd = { GITHUB_PERSONAL_ACCESS_TOKEN = "security find-generic-password -s opencast-github -w" }
tools = []                 # allowlist, "prefix*" ok; empty = all
in_launcher = true
in_chat = true
auto_approve = ["get_*", "list_*"]
timeout_secs = 60

[[ai_commands]]            # unchanged keys, plus the existing output/temperature/max_tokens

[[snippets]]
keyword = ";date"
text = "{date}"
auto_expand = true         # override the symbol-prefix default
```

## Not in this pass

- OAuth for remote MCP servers (static headers / `headers_cmd` only).
- MCP resources and prompts (tools only); sampling requests are refused.
- Images to vision models (attachments are text; images go through OCR).
- Themes (explicitly deferred).
- Intel/universal builds (arm64 only, matching the existing scripts).

## Verification

- `cargo test` covers ranking, tool-call parsing and request bodies for all
  three providers, an end-to-end stdio MCP round trip against a fake server,
  chats, sync (two simulated Macs), update parsing, provider add/remove with
  comments kept, engine wiring for Shortcuts/MCP/chats/UI builtins.
- On a Mac: see the checklist in the README "Trying the new features" section.
