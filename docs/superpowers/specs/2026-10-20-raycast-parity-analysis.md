# Raycast OOTB parity analysis

Scope: Raycast **built-in** features (not the Extension Store). OpenCast should not chase store parity; scripts + config stay the extension path.

Legend: **Have** = usable parity or better. **Partial** = exists but thinner. **Gap** = missing.

## Core launcher

| Raycast | OpenCast | Status |
|---------|----------|--------|
| App search / launch | Spotlight/Launch Services index | **Have** |
| Fuzzy search + ranking | Fuzzy + category + usage | **Have** |
| File search | mdfind name (+ content) | **Have** |
| Calculator | Inline calc | **Have** |
| Quicklinks | `[[quicklinks]]` | **Have** |
| Fallback search (Google etc.) | `{query}` quicklinks when thin | **Have** |
| Root search aliases / navigation | Category keywords | **Partial** |
| Favorites / pinned | Cmd+Shift+P → pinned.json | **Have** |
| Recent commands / frequency | usage.json boost | **Have** |

## Clipboard & text

| Raycast | OpenCast | Status |
|---------|----------|--------|
| Clipboard History | Text + persist (clip:) | **Have** (text only) |
| Snippets / expansions | `[[snippets]]` paste | **Have** (no auto-expand) |
| Emoji picker | emoji / emo search | **Have** |
| Confetti / fun | — | Skip (YAGNI) |

## Window management

| Raycast | OpenCast | Status |
|---------|----------|--------|
| Halves / quarters / maximize / center | Yes | **Have** |
| Thirds | Yes | **Have** |
| Almost maximize / reasonablesize | Almost Maximize | **Have** |
| Restore / previous size | Restore Window Size | **Have** |
| Move to next/prev display | Next/Previous Display | **Have** |
| Saved layouts | Remember / Restore / Forget | **Have** (stronger than Raycast basic) |

## System / macOS

| Raycast | OpenCast | Status |
|---------|----------|--------|
| Quit / Kill / Force Quit apps | Front + Quit by name | **Have** |
| Eject / Empty Trash / Sleep / Lock | Builtins | **Have** |
| Toggle Dark Mode | System toggle + panel theme | **Have** |
| Volume / brightness | Builtins | **Have** |
| Network / IP / flush DNS | scripts possible | **Partial** |
| Open folder / preference panes | Open Config Folder | **Partial** |

## AI

| Raycast | OpenCast | Status |
|---------|----------|--------|
| AI Chat | Tab / AI Chat | **Have** |
| AI on selection | `[[ai_commands]]` | **Have** |
| Multiple providers | Any OpenAI/Anthropic/Gemini | **Better** |
| AI extensions store | — | Skip; scripts + config |

## Extensibility

| Raycast | OpenCast | Status |
|---------|----------|--------|
| Store extensions | — | Skip |
| Script Commands | Raycast headers accepted | **Have** |
| Deep links / URL scheme | `opencast://run/…` · `search?q=` | **Have** |
| Menubar extras | Stay Awake status item | **Partial** |

## UX chrome

| Raycast | OpenCast | Status |
|---------|----------|--------|
| Settings UI | Settings commands + TOML | **Partial** |
| Hotkey list in UI | Shortcuts + keys on rows | **Have** |
| Theme / appearance | Adaptive glass + Appearance | **Have** |
| Presentation modes | Island / Spotlight | **Have** (OpenCast-specific) |

## OpenCast-only (ahead of Raycast)

- Hyper key + Click Text (OCR)
- Any AI provider, no account
- Layout remember/restore with launch-missing
- MIT / local-first

## OpenCast extras (not Raycast)

| Feature | Status |
|---------|--------|
| Stay Awake (Amphetamine-lite) + menu-bar dot | Shipping — see `2026-10-20-stay-awake-design.md` |

## Recommended parity phases

0. **Stay Awake** — Amphetamine-lite + menu bar (OpenCast-first)  
1. **Daily habit gaps** — Clipboard History, then Snippets  
2. **Search feel** — usage/recents ranking; file search (mdfind)  
3. **Window polish** — next display, almost-maximize, restore previous  
4. **System commands** — quit app, kill process, eject, empty trash, lock, sleep  
5. **Emoji + deep link** — if still needed after 1–4  

Do not attempt store parity in one project.
