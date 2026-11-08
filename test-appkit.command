#!/bin/zsh -l
# Live AppKit tests: actually create windows. Panel-logic Bools are not enough —
# Tahoe Settings broke while those stayed green. CI and ./release.command run this.
cd "$(dirname "$0")"
command -v swiftc >/dev/null || { echo "swiftc not found (Xcode command-line tools)"; exit 1; }
ARCH=$(uname -m)
mkdir -p target/tests
swiftc -O -parse-as-library -target "${ARCH}-apple-macos14.0" \
  macos/SettingsPresentation.swift macos/MenuBarSymbol.swift macos/OverlayPanelPolicy.swift \
  macos-tests/appkit.swift \
  -o target/tests/appkit-settings || exit 1
./target/tests/appkit-settings
