#!/bin/zsh -l
# Build a signed, notarized OpenCast.app and package it as a .dmg and .zip
# for GitHub Releases / Homebrew.
#
# One-time setup (needs an Apple Developer account):
#   1. A "Developer ID Application" certificate in your login keychain.
#   2. Store notarization credentials once:
#        xcrun notarytool store-credentials opencast-notary \
#          --apple-id you@example.com --team-id TEAMID --password <app-specific password>
#
# Then:  ./release.command            (uses the version in Cargo.toml)
# Env:   DEVELOPER_ID="Developer ID Application: Your Name (TEAMID)"   (auto-detected if unset)
#        NOTARY_PROFILE=opencast-notary
#        SKIP_NOTARIZE=1  to build and sign only
set -e
cd "$(dirname "$0")"
command -v cargo >/dev/null || { [ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"; }
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
NOTARY_PROFILE=${NOTARY_PROFILE:-opencast-notary}
if [ -z "$DEVELOPER_ID" ]; then
  DEVELOPER_ID=$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application:.*\)".*/\1/p' | head -1)
fi
[ -n "$DEVELOPER_ID" ] || { echo "No 'Developer ID Application' certificate found. Set DEVELOPER_ID."; exit 1; }
echo "== OpenCast $VERSION, signing as: $DEVELOPER_ID =="

# Same settings as build-app.command (see the notes there).
unset MACOSX_DEPLOYMENT_TARGET
export CFLAGS_aarch64_apple_darwin="-mmacosx-version-min=14.0"
cargo rustc --release -p opencast-ffi -- --print native-static-libs 2>&1 | tee build-release.log | grep -E "^(error|warning: unused)" || true
FLAGS=$(awk -F'native-static-libs: ' '/native-static-libs:/ { print $2; exit }' build-release.log)
[ -f target/release/libopencast_ffi.a ] || { echo "Rust build failed, see build-release.log"; exit 1; }

DIST=dist/$VERSION
APP=$DIST/OpenCast.app
rm -rf "$DIST"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp macos/Info.plist "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" -c "Set :CFBundleVersion $VERSION" "$APP/Contents/Info.plist"
cp macos/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
eval swiftc -parse-as-library -O -target arm64-apple-macos14.0 \
  -import-objc-header macos/opencast.h \
  -framework AppKit -framework SwiftUI -framework IOKit -framework ApplicationServices \
  macos/*.swift \
  -L target/release -lopencast_ffi $FLAGS \
  -o "$APP/Contents/MacOS/OpenCast"
strip -x "$APP/Contents/MacOS/OpenCast"

echo "== Signing (hardened runtime) =="
codesign --force --options runtime --timestamp \
  --entitlements packaging/OpenCast.entitlements \
  --sign "$DEVELOPER_ID" "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"

ZIP=$DIST/OpenCast-$VERSION.zip
DMG=$DIST/OpenCast-$VERSION.dmg
ditto -c -k --keepParent "$APP" "$ZIP"

if [ -z "$SKIP_NOTARIZE" ]; then
  echo "== Notarizing (a few minutes) =="
  xcrun notarytool submit "$ZIP" --keychain-profile "$NOTARY_PROFILE" --wait
  xcrun stapler staple "$APP"
  rm -f "$ZIP"
  ditto -c -k --keepParent "$APP" "$ZIP"
fi

echo "== DMG =="
STAGE=$DIST/dmg
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "OpenCast $VERSION" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"
codesign --force --timestamp --sign "$DEVELOPER_ID" "$DMG"
if [ -z "$SKIP_NOTARIZE" ]; then
  xcrun notarytool submit "$DMG" --keychain-profile "$NOTARY_PROFILE" --wait
  xcrun stapler staple "$DMG"
fi

echo
echo "== Done: $DIST =="
ls -lh "$DIST"
echo
echo "Homebrew cask (packaging/homebrew/opencast.rb):"
echo "  version \"$VERSION\""
echo "  sha256 \"$(shasum -a 256 "$DMG" | cut -d' ' -f1)\""
echo
echo "Publish: gh release create v$VERSION \"$DMG\" \"$ZIP\" --title \"OpenCast $VERSION\" --generate-notes"
