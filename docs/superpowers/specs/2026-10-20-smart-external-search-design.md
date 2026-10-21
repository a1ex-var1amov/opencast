# Smart external search

Approved: instant in-menu results; soft Spotlight expand when weak/path-like; force prefixes; never block first paint.

- `search` — registry only (+ existing emoji/notes/fallback quicklinks in engine).
- `searchFiles` — timed mdfind (name or content:), home-scoped, 400ms kill.
- Swift: deliver `search` immediately; after 180ms debounce, if expand criteria met, merge `searchFiles`; footer `Searching files…` while pending.
