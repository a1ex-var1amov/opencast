# Island, app index, and click session

Three additions to the SwiftUI panel. The island wraps a camera notch. The launcher lists every app macOS knows about. Click Text stays one overlay session, and Caps Lock keeps its normal jobs outside that session.

## Island

On a display with no notch, the island is unchanged: 760pt wide, centered on the screen under the pointer, top flush with that screen.

On a notched display the island is one shape. The left and right wings fill the menu-bar areas beside the camera (`NSScreen.auxiliaryTopLeftArea` and `auxiliaryTopRightArea`). The body hangs from the bottom of those areas, so the camera stays clear, and it spans from the outer edge of the left wing to the outer edge of the right wing. Search, rows, and the footer sit in the body only. The wings are the same fill as the island, not a second field. Bottom corners stay 18pt. The top edge, including the wings, stays flush with the screen. Shadow room stays 30pt on the left, right, and bottom.

`[general]` gains two optional point widths:

```toml
# How far each wing reaches out from the camera. Omit to fill the area beside it.
# notch_left = 160
# notch_right = 160
```

Omitted uses the full auxiliary area on that side. A set value is the wing width outward from the camera, clamped to that area. `0` draws no wing on that side. The body is at least 760pt, centered under the camera, and widens so its sides reach the outer edge of each wing that exists. If both wings are `0`, that is the usual 760pt island dropped below the camera.

The frame math is a pure function of the screen frame, the two auxiliary rects, the two config values, and the body height. A small assert in the Swift file checks one notched case and one plain case, same style as `seekSpanCheck`.

## Apps

`opencast-core` stays portable. It already turns an app list into launchable commands (`app:<path>`, no subtitle, real icon from the bundle path). Linux keeps the directory scan.

On macOS the ffi host builds that list from Spotlight instead of one directory level. If Spotlight returns nothing, it falls back to the directory scan so a fresh Mac still lists `/Applications`.

```
mdfind 'kMDItemContentTypeTree == "com.apple.application-bundle"'
```

A pure filter in `opencast-core` decides what is shown. It is unit tested.

- Keep a bundle only if its path ends in `.app` and no earlier path component ends in `.app`. That drops helpers nested inside another app.
- Display name is `CFBundleDisplayName`, then `CFBundleName`, then the file name.
- Skip `LSBackgroundOnly` apps. Keep menu-bar apps (`LSUIElement`).
- One row per path. Reload Commands rebuilds the list.

Launching is unchanged: choosing the row opens that bundle.

## Click

Click Text is the click session. Its id stays `seek`. Its title stays `Click Text`, so the query `click` already matches the first word. A core test fixes that: with a fixture of a few apps, `click` returns `seek` first. No second command.

Enter the session by running that command, from the list or from a hotkey. `examples/config.toml` keeps `Hyper+C` as a commented example. It does not ship enabled.

The session is the overlay. It lasts until Esc or a click. There is no sticky mode after it closes.

Caps Lock is unchanged outside the session: a tap toggles the launcher, a hold adds Control, Option, Command, and Shift.

While the overlay is up, the hyper tap must not toggle the launcher. The tap callback emits `{"type":"tap"}` and returns. It does not call into Swift work itself. Swift handles `tap` on the main queue, like every callback except `hide`:

- If the click overlay is up, move to the next match. Tab, Shift-Tab, and `;` stay as they are.
- Otherwise toggle the launcher, as a tap does today.

Return still clicks. Esc still closes the overlay and hides the app. The next tap toggles the launcher. Holding Caps Lock still adds the four modifiers, including while the overlay is up.
