#!/bin/zsh -l
# Builds a release OpenCast.app and installs it to ~/Applications, then launches it.
#
# Arch: this Mac's by default (fastest, reuses target/release). Build both slices
# with `OC_ARCH=universal ./build-app.command` — or OC_ARCH="arm64 x86_64" — which
# is what ./release.command does for the dmg.
cd "$(dirname "$0")"
command -v cargo >/dev/null || { [ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"; }
echo "== Stopping OpenCast (if any) =="
pkill -x OpenCast 2>/dev/null
sleep 1
# Sign with any local code-signing identity (a self-signed one is enough): an
# ad-hoc signature changes every build, and macOS then forgets the app's
# Accessibility / Screen Recording grants.
if [ -z "$APPLE_SIGNING_IDENTITY" ]; then
  # Prefer a self-signed "OpenCast" identity when present (else first valid one).
  IDS=$(security find-identity -v -p codesigning 2>/dev/null | sed -n 's/.*"\(.*\)".*/\1/p')
  ID=$(print -l -- ${(f)IDS} | grep -Fx 'OpenCast' | head -1)
  [ -z "$ID" ] && ID=$(print -l -- ${(f)IDS} | head -1)
  [ -n "$ID" ] && export APPLE_SIGNING_IDENTITY="$ID" && echo "== Signing as $ID =="
fi
ARCHS=(${(s: :)${OC_ARCH:-$(uname -m)}})
[ "$ARCHS[1]" = "universal" ] && ARCHS=(arm64 x86_64)
echo "== Building $ARCHS =="
SLICES=()
for a in "${ARCHS[@]}"; do
  # cargo/swiftc output goes to build-app-<arch>.log; failures print their tail.
  ./packaging/build-arch.zsh "$a" "build-app-$a.log" "target/slices-$a/OpenCast" || exit 1
  SLICES+=("target/slices-$a/OpenCast")
done
APP=target/release/OpenCast.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cp macos/Info.plist "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" -c "Set :CFBundleVersion $VERSION" "$APP/Contents/Info.plist"
cp macos/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
if [ "${#SLICES[@]}" -gt 1 ]; then
  lipo -create "${SLICES[@]}" -output "$APP/Contents/MacOS/OpenCast" || exit 1
  rm -rf target/slices-*
else
  cp "${SLICES[1]}" "$APP/Contents/MacOS/OpenCast"
fi
# A dmg that says "universal" must contain both slices; check instead of trusting.
echo "== Binary archs: $(lipo -archs "$APP/Contents/MacOS/OpenCast") =="
# Strip local symbols from the final binary (what `strip = true` was meant to do).
strip -x "$APP/Contents/MacOS/OpenCast" 2>/dev/null
if [ -n "$APPLE_SIGNING_IDENTITY" ]; then
  # No secure timestamp: codesign would phone Apple's timestamp server and stall
  # (indefinitely on a sick network). A dev build is not notarized anyway; the
  # release script signs with a timestamp and notarizes.
  codesign --force --deep --timestamp=none --sign "$APPLE_SIGNING_IDENTITY" "$APP" || exit 1
fi
rm -rf "$HOME/Applications/OpenCast.app"
ditto "$APP" "$HOME/Applications/OpenCast.app" || exit 1
# Drop the debug bundle so Launch Services does not keep routing opencast:// to it (P11-3).
rm -rf target/debug/OpenCast.app
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$HOME/Applications/OpenCast.app"
echo "== Installed to ~/Applications/OpenCast.app =="
