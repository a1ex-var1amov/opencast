#!/bin/zsh -l
# Builds a release OpenCast.app and installs it to ~/Applications, then launches it.
cd "$(dirname "$0")"
command -v cargo >/dev/null || { [ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"; }
echo "== Stopping OpenCast (if any) =="
pkill -x OpenCast 2>/dev/null
sleep 1
# Sign with any local code-signing identity (a self-signed one is enough): an
# ad-hoc signature changes every build, and macOS then forgets the app's
# Accessibility / Screen Recording grants.
if [ -z "$APPLE_SIGNING_IDENTITY" ]; then
  ID=$(security find-identity -v -p codesigning 2>/dev/null | sed -n 's/.*"\(.*\)".*/\1/p' | head -1)
  [ -n "$ID" ] && export APPLE_SIGNING_IDENTITY="$ID" && echo "== Signing as $ID =="
fi
echo "== Building release bundle =="
export MACOSX_DEPLOYMENT_TARGET=14.0
cargo build --release -p opencast-ffi 2>&1 | tee build-app.log | tail -15
BUILD_OK=${pipestatus[1]}
if [ "$BUILD_OK" != 0 ]; then
  echo "== Build failed, see build-app.log ==" | tee -a build-app.log
  exit 1
fi
FLAGS=$(cargo rustc -p opencast-ffi --release -- --print native-static-libs 2>&1 | awk -F'native-static-libs: ' '/native-static-libs:/ { print $2; exit }')
APP=target/release/OpenCast.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp macos/Info.plist "$APP/Contents/Info.plist"
cp macos/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
eval swiftc -parse-as-library -O -target arm64-apple-macos14.0 \
  -import-objc-header macos/opencast.h \
  -framework AppKit -framework SwiftUI \
  macos/*.swift \
  -L target/release -lopencast_ffi $FLAGS \
  -o "$APP/Contents/MacOS/OpenCast" 2>&1 | tee -a build-app.log | tail -15
BUILD_OK=${pipestatus[1]}
if [ "$BUILD_OK" != 0 ]; then
  echo "== Build failed, see build-app.log ==" | tee -a build-app.log
  exit 1
fi
if [ -n "$APPLE_SIGNING_IDENTITY" ]; then
  codesign --force --deep --sign "$APPLE_SIGNING_IDENTITY" "$APP" || exit 1
fi
rm -rf "$HOME/Applications/OpenCast.app"
ditto "$APP" "$HOME/Applications/OpenCast.app" || exit 1
echo "== Installed to ~/Applications/OpenCast.app ==" | tee -a build-app.log
open "$HOME/Applications/OpenCast.app"
