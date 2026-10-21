# Clipboard + screenshots (Raycast/Alfred practices)

## Goal

Fast, durable clipboard history with images/files, thumbs, OCR search, ignore list, pin/delete/retention, plus watched screenshot folders and Paste Latest Screenshot.

## Store (`~/.config/opencast/`)

- `clipboard.json` — metadata (id, kind, preview, paths, pin, at, ocrText, contentHash)
- `clipboard-media/` — original pasteboard bytes
- `clipboard-thumbs/` — ≤128px JPEG for rows
- Default cap ~200; pinned exempt from retention (default 7 days)

## Capture

- Poll `changeCount` every 0.15s; work only on change
- Ignore: 1Password, Bitwarden, LastPass, Keychain Access (bundle IDs)
- Dedup by content hash
- Images: keep original PNG/TIFF; thumb + Vision OCR (fast) off main
- File URLs → kind `file`

## UI / commands

- `clip` browse; search text + OCR
- `shots` / Search Screenshots — screencapture dir + Desktop
- Actions: Paste, Copy, Paste as Plain Text, Copy Text from Image, Pin, Delete, Delete All
- Builtin Paste Latest Screenshot

## Out of scope

Universal Clipboard, rich Paste as…, rename, QR, cloud download.
