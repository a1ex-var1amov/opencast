#!/bin/zsh -l
# Compile the panel's pure logic (no AppKit, no engine, no @main) together with
# macos-tests/main.swift and run the checks. CI runs this: the panel used to have
# no tests at all, only a type check.
#
# Add a file to the list below when you extract logic worth testing; keep the
# extracted files free of Bridge/AppKit so this stays a two-second build.
cd "$(dirname "$0")"
command -v swiftc >/dev/null || { echo "swiftc not found (Xcode command-line tools)"; exit 1; }
ARCH=$(uname -m)
mkdir -p target/tests
swiftc -O -target "${ARCH}-apple-macos14.0" \
  macos/DeepLink.swift macos/MeetingLinks.swift macos/SnippetMatch.swift macos/SpaceLabel.swift \
  macos/ListFilter.swift macos/ShortcutApps.swift macos/SpotifyText.swift \
  macos/Models.swift macos/Glyphs.swift \
  macos-tests/main.swift \
  -o target/tests/panel-logic || exit 1
./target/tests/panel-logic
