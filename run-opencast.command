#!/bin/zsh -l
# Builds and launches the debug OpenCast.app. Double-click from Finder, or run in a terminal.
cd "$(dirname "$0")"
command -v cargo >/dev/null || { [ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"; }
command -v cargo >/dev/null || { echo "Missing cargo — install Rust and re-run."; read -k1; exit 1; }
command -v swiftc >/dev/null || { echo "Missing swiftc — install the Xcode command-line tools and re-run."; read -k1; exit 1; }
echo "== Building debug library =="
export MACOSX_DEPLOYMENT_TARGET=14.0
cargo build -p opencast-ffi || { echo "Build failed."; read -k1; exit 1; }
FLAGS=$(cargo rustc -p opencast-ffi -- --print native-static-libs 2>&1 | awk -F'native-static-libs: ' '/native-static-libs:/ { print $2; exit }')
APP=target/debug/OpenCast.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp macos/Info.plist "$APP/Contents/Info.plist"
cp macos/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
eval swiftc -parse-as-library -g -target arm64-apple-macos14.0 \
  -import-objc-header macos/opencast.h \
  -framework AppKit -framework SwiftUI \
  macos/*.swift \
  -L target/debug -lopencast_ffi $FLAGS \
  -o "$APP/Contents/MacOS/OpenCast" || { echo "swiftc failed."; read -k1; exit 1; }
pkill -x OpenCast 2>/dev/null
open "$APP"
