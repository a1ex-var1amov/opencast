# Spotlight habit pack + Spotify outline (2026-10-23)

Make OpenCast the daily ⌘Space replacement for Spotlight's core jobs. Spotify rich player is outlined here and ships in a later pass.

## Decision

- **Ship first:** Spotlight habit pack (approach 2 from brainstorming).
- **Ship later:** Native Spotify player v2 with album covers (no Raycast React host).
- Raycast script-commands remain installable; they are not the Spotify cover strategy.

## Spotlight habit pack (v1)

### Goals

1. Empty query feels like Spotlight home: frecency apps + pins lead; window/builtin spam does not own the first screen.
2. Short typed queries surface files/folders without requiring a `file` prefix, with clear path subtitles and folder affordance.
3. When command hits are weak, a configurable web fallback appears (default: one safe search quicklink pattern, opt-in or already via `fallbacks`).
4. Recent documents appear in main search (not only ⌘R on an app row).
5. Ranking regressions are caught by unit tests.

### Non-goals (v1)

- Siri / natural-language queries
- Replacing Spotlight indexing UI
- Raycast `@raycast/api` React runtime
- Spotify album art (phase 2)

### Current baseline

- Empty query: commands before apps (`commands.rs`); kind caps for Window/Builtin.
- Files: panel calls `searchFiles` → `files_mac` `mdfind`; fallbacks may include `"files"`.
- Recents: `Model.recentDocuments(appPath:)` + ⌘R action only.
- Fallbacks: `general.fallbacks` + `fallback_threshold` already exist.

### Design

**Empty query**
- Sort so: pins → high-frecency apps → other apps → capped commands (window/builtin caps stay).
- Keep `builtin:chat` boost only if it does not push apps off the first screen; prefer apps over chat when both unused.
- Update tests that currently assert "commands before apps".

**Files / folders**
- Name search already runs for soft expand; ensure folders are not filtered out, subtitle is abbreviated path, icon distinguishes folder vs file.
- Prefer earlier merge / slightly higher limit for folder hits when query looks like a path fragment (contains `/` or ends with known folder names) — keep it cheap.
- Return on a folder opens it in Finder (existing file open path if already correct).

**Web fallback**
- Prefer config: document that `fallbacks` should include a `{query}` quicklink (Kagi/DuckDuckGo/Google).
- Add token `"web"` that expands to a built-in `https://duckduckgo.com/?q={query}` quicklink if no explicit web quicklink is configured — only when strong hits &lt; threshold.
- Default `examples/config.toml`: keep existing fallbacks; comment shows `"web"` option.

**Recent documents in search**
- Cache a small global recent list (mdfind `kMDItemLastUsedDate` last N days, exclude Library), refresh off the hot path like bookmarks/history.
- Rows: kind File (or Quicklink), subtitle `Recent · ~/…`, id `recent:<path>`.
- Prefix `recent` / empty-ish short query can boost them; opening uses default app.

**Tests**
- Empty query: first non-pin rows are mostly apps when usage has app picks.
- Folder path survives filter; web token appears only under threshold.
- Recent cache parse / matching.

### Success

A week of daily use without opening Spotlight for apps, files, or calculator.

## Spotify player v2 (outline only — not this plan)

- Keep AppleScript control (`Spotify.swift`).
- Add artwork: try `artwork url of current track`; cache image; if missing, optional Spotify Web API + Keychain (same pattern as MCP OAuth).
- UI: cover + title/artist in the Spotify list header; scrub/progress already partially there.
- Do not build Raycast React host for this.
- Separate design doc when Spotlight pack ships.

## Verification

- `cargo test -p opencast-core`
- `./test-swift.command`
- Manual: empty hotkey → apps; type a Downloads filename → file row; weak query → web; `recent` → docs.
