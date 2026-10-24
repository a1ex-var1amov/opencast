# Spotify search mode Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Config `[spotify] search = "open"|"list"` with Keychain secret; list mode searches Web API and plays via `spotify:` URI.

**Architecture:** `opencast-core` owns config + token/search; FFI `exec` turns list-mode search into `ExecResult::List`; selecting a row re-runs `builtin:spotify-search` with `spotify:…` arg → `OpenUrl`.

**Tech stack:** Existing reqwest, toml config, ExecResult::List, Keychain via `client_secret_cmd`.

## File map

- `crates/opencast-core/src/config.rs` — `SpotifyConfig`
- `crates/opencast-core/src/spotify.rs` — token + search (new)
- `crates/opencast-core/src/lib.rs` — mod
- `crates/opencast-core/src/engine.rs` — plan branch
- `crates/opencast-core/src/doctor.rs` — warn if list without creds
- `crates/opencast-ffi/src/exec.rs` — UiArgs `spotify-search-list` → List
- `examples/config.toml` — commented block

## Tasks

### Task 1: Config + spotify module + plan

- Add `SpotifyConfig { search, client_id, client_secret_cmd }` default search open
- `spotify::search(cfg, query) -> Result<Vec<ListItem>>` with in-process token cache
- Plan: URI arg → OpenUrl; list+creds → UiArgs spotify-search-list; else open deep link
- Doctor warning; unit tests for URL encode / plan branch / config parse
- `cargo test -p opencast-core`

### Task 2: FFI list + docs

- exec special-case UiArgs spotify-search-list → search → ExecResult::List
- examples/config.toml comment
- `./build-app.command`
