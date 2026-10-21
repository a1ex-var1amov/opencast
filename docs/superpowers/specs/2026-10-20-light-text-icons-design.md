# Light text colors and icons

## Problem

1. Appearance: Light still shows some labels in dark-mode gray / off ink (AppKit vs hex drift).
2. App/file icons look soft (cached at 24pt). SF Symbols look plain (outline, secondary ink).

## Approach

Semantic `NSColor` labels under aqua / darkAqua. Retina app icons. Filled SF Symbols on primary ink.

## Text

- `Theme.primary` / `secondary` / `tertiary` → `labelColor` / `secondaryLabelColor` / `tertiaryLabelColor` sampled with `NSAppearance.performAsCurrentDrawingAppearance` for the scheme.
- Keep panel `appearance` forced from Appearance setting.
- On search-field focus, set field-editor appearance to match so caret/typed text are not dark ink leftovers.

## Icons

- Cache `NSWorkspace.icon(forFile:)` at 64×64 (not 24). Draw at 28×28 with high interpolation.
- SF Symbols: prefer `.fill` in `byId` / kind defaults; size ~17 medium; hierarchical (or palette) with `Theme.primary`.
- Emoji / center-third literals unchanged. Clipboard thumbs 28pt to match.

## Out of scope

Custom asset catalog art; Stay Awake menu-bar redesign; menu chrome restyle.
