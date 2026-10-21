# Stay Awake (Amphetamine-lite)

Prevent Mac idle sleep from OpenCast, with a menu-bar state indicator. First cut of the Raycast-parity backlog; clipboard and the rest follow as separate specs.

## Commands

Searchable builtins (keywords include `awake`, `caffeine`, `sleep`, `amphetamine`, `keepingyouawake`):

| id | title |
|----|-------|
| `builtin:stay-awake-off` | Stay Awake: Off |
| `builtin:stay-awake-indefinite` | Stay Awake: Indefinite |
| `builtin:stay-awake-30m` | Stay Awake: 30 minutes |
| `builtin:stay-awake-1h` | Stay Awake: 1 hour |
| `builtin:stay-awake-3h` | Stay Awake: 3 hours |

Running one mode replaces any previous mode. Off clears the assertion.

## Mechanism

Swift owns the process and the status item (AppKit). Exec returns `StayAwake` with a mode string; the panel applies it on the main actor.

- Tool: `/usr/bin/caffeinate`
- Assertion: idle sleep prevention (`-i`)
- Timed: `caffeinate -i -t <seconds>` (1800 / 3600 / 10800)
- Indefinite: `caffeinate -i` until killed
- Off / Quit OpenCast / new mode: terminate the child process
- When a timed `caffeinate` exits on its own, Swift returns to Off and updates the menu bar

No new crates. No TOML for v1 (in-memory only).

## Menu bar

Always-visible `NSStatusItem` (variable length).

- **Off:** dim / hollow circle
- **On:** filled circle using the system accent color
- **Click:** menu with the five modes; timed modes show remaining time in the menu title or a disabled subtitle line when active

## Out of scope (v1)

Display-sleep-only modes, per-app rules, coffee-cup animation, persist-across-relaunch, “show only when active”.
