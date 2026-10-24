# Spotify search mode (2026-10-23)

Config switch between opening Spotify’s search UI and listing Web API hits in OpenCast.

## Goal

`Spotify Search` stays one command. A `[spotify]` config block picks the path:

| `search` | Behavior |
|----------|----------|
| `"open"` (default) | `spotify:search:<query>` — today’s deep link, no credentials |
| `"list"` | Client-credentials Web API search → panel list → Return opens `spotify:track|album|artist:…` |

## Config

```toml
[spotify]
search = "open"   # or "list"
client_id = "…"
client_secret_cmd = "security find-generic-password -s opencast-spotify -w"
```

- Secret never in `config.toml` (same pattern as AI `api_key_cmd`).
- `search = "list"` without a resolvable `client_id` + secret → fall back to `"open"` and Doctor warns.
- Integration still gated by `general.integrations` / Spotify installed.

## Flow (`list`)

1. User runs `builtin:spotify-search` with a non-empty query (args field unchanged).
2. Engine plans `Ui("spotify-search-list")` with the query (or FFI runs search and returns a list plan).
3. Token: `POST https://accounts.spotify.com/api/token` (client_credentials); cache in process until near expiry.
4. `GET https://api.spotify.com/v1/search?q=…&type=track,album,artist&limit=8` (per type or combined).
5. Panel shows rows: track title / `Artist · Album`; albums and artists labeled in subtitle.
6. Return → `NSWorkspace` / `OpenUrl` on the item’s `spotify:…` URI (desktop app plays or shows it).

Empty query: still open the args field (app-row ⌘K path unchanged).

## Non-goals

User OAuth, library/playlists, album art, “lucky” auto-play, per-invocation mode override.

## Files (expected)

- `crates/opencast-core/src/config.rs` — `[spotify]` struct + parse
- `crates/opencast-core/src/spotify.rs` (new) — token + search (reqwest)
- `crates/opencast-core/src/engine.rs` — plan branch for `SpotifySearch`
- `crates/opencast-core/src/doctor.rs` — list-without-creds warning
- `crates/opencast-ffi` / `macos/OpenCastApp.swift` — list UI + open URI
- `examples/config.toml` — commented `[spotify]` block
