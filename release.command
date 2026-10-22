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

# Arch: both slices by default, so an Intel Mac gets the same dmg. Set
# OC_ARCH=arm64 for a single-slice build (faster; nothing else changes).
ARCHS=(${(s: :)${OC_ARCH:-arm64 x86_64}})
[ "$ARCHS[1]" = "universal" ] && ARCHS=(arm64 x86_64)

DIST=dist/$VERSION
APP=$DIST/OpenCast.app
rm -rf "$DIST"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp macos/Info.plist "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" -c "Set :CFBundleVersion $VERSION" "$APP/Contents/Info.plist"
cp macos/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"

SLICES=()
for a in "${ARCHS[@]}"; do
  ./packaging/build-arch.zsh "$a" "build-release-$a.log" "target/slices-$a/OpenCast"
  SLICES+=("target/slices-$a/OpenCast")
done
if [ "${#SLICES[@]}" -gt 1 ]; then
  lipo -create "${SLICES[@]}" -output "$APP/Contents/MacOS/OpenCast"
  rm -rf target/slices-*
else
  cp "${SLICES[1]}" "$APP/Contents/MacOS/OpenCast"
fi
# Notarization and the cask sha256 mean nothing for the wrong arches, so check
# the finished binary rather than trust the loop.
echo "== Binary archs: $(lipo -archs "$APP/Contents/MacOS/OpenCast") =="
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
SHA=$(shasum -a 256 "$DMG" | cut -d' ' -f1)
echo "Publish: gh release create v$VERSION \"$DMG\" \"$ZIP\" --title \"OpenCast $VERSION\" --generate-notes"
echo
# Print the cask with the three things only a real release knows filled in, so
# the copy that gets pasted cannot be stale or half a placeholder.
REMOTE=$(git remote get-url origin 2>/dev/null || true)
REPO=""
case "$REMOTE" in
  *github.com*)
    REPO=${REMOTE#*github.com}
    REPO=${REPO#:}
    REPO=${REPO#/}
    REPO=${REPO%.git}
    ;;
esac
case "$REPO" in
  */*) ;;
  *) REPO="" ;;
esac
if [ -n "$REPO" ]; then
  echo "Homebrew cask — paste this into your tap as Casks/opencast.rb (remote $REPO):"
  sed -e "s#^  version \".*\"#  version \"$VERSION\"#" \
      -e "s#^  sha256 \".*\"#  sha256 \"$SHA\"#" \
      -e "s#OWNER#$REPO#g" \
      packaging/homebrew/opencast.rb
else
  echo "Homebrew cask (packaging/homebrew/opencast.rb): set version \"$VERSION\","
  echo "sha256 \"$SHA\" and OWNER by hand — this checkout has no github.com remote to fill them from."
fi
