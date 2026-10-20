# Adaptive glass panel

Polish the SwiftUI launcher so it follows macOS appearance settings and looks like a system glass panel. Island keeps a flush top; Spotlight is fully rounded.

## Material

The island fill is no longer a flat solid. Default is a translucent material with backdrop blur (Golden Gate–style glass).

Sources of truth:

- **Appearance** — existing `theme` / Appearance commands and `preferredColorScheme` (system, light, dark).
- **Accent** — existing `NSColor.controlAccentColor` for selection and accents.
- **Reduce transparency** — when `NSWorkspace.shared.accessibilityDisplayShouldReduceTransparency` is true (and on change notification), use an opaque island fill instead of blur. Same light/dark colors as today’s solid theme.

Implementation stays in Swift (`Theme` + panel chrome). No new TOML keys.

## Shapes

- **Island** (`presentation == "island"`): top edge flush; bottom corners only, radius 18pt (unchanged geometry).
- **Spotlight** (`presentation == "spotlight"`): continuous rounded rectangle, all four corners, radius 22pt. Centered placement unchanged.

Chat, args, and output panes use the same material and the shape for the current presentation.

## Polish (same pass)

- Hairline stroke on the glass edge (slightly softer than today).
- Selected row: accent wash + optional thin accent edge; readable on glass.
- Footer keycaps use the same material language (subtle fill, not heavy chips).
- Search field and row spacing stay close to today; no redesign of information architecture.

## Out of scope

- Notch wing layout
- Custom user glass tint beyond system accent
- Separate “glass intensity” config
