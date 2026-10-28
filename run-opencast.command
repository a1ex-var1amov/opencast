#!/bin/zsh -l
# Builds and launches the debug OpenCast.app. Double-click from Finder, or run in a terminal.
cd "$(dirname "$0")"
command -v cargo >/dev/null || { [ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"; }
command -v cargo >/dev/null || { echo "Missing cargo — install Rust and re-run."; read -k1; exit 1; }
command -v swiftc >/dev/null || { echo "Missing swiftc — install the Xcode command-line tools and re-run."; read -k1; exit 1; }
echo "== Building debug library =="
# Do NOT export MACOSX_DEPLOYMENT_TARGET for cargo: with Rust 1.91 it makes release
# builds reject their own proc-macro dylibs ("can't find crate for `zerocopy_derive`").
# Rust's default (macOS 11) links fine into the app; swiftc's -target sets the app's minimum.
unset MACOSX_DEPLOYMENT_TARGET
# C/asm in dependencies (ring) still needs a minimum, or ld warns "built for newer macOS".
export CFLAGS_aarch64_apple_darwin="-mmacosx-version-min=14.0"
cargo rustc -p opencast-ffi -- --print native-static-libs 2>&1 | tee build-debug.log | grep -v '^note: native-static-libs'
[ "${pipestatus[1]}" = 0 ] || { echo "Build failed."; read -k1; exit 1; }
FLAGS=$(awk -F'native-static-libs: ' '/native-static-libs:/ { print $2; exit }' build-debug.log)
APP=target/debug/OpenCast.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cp macos/Info.plist "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" -c "Set :CFBundleVersion $VERSION" "$APP/Contents/Info.plist"
cp macos/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
eval swiftc -parse-as-library -g -target arm64-apple-macos14.0 \
  -import-objc-header macos/opencast.h \
  -framework AppKit -framework SwiftUI -framework IOKit -framework ApplicationServices \
  macos/*.swift \
  -L target/debug -lopencast_ffi $FLAGS \
  -o "$APP/Contents/MacOS/OpenCast" || { echo "swiftc failed."; read -k1; exit 1; }
pkill -x OpenCast 2>/dev/null
open "$APP"
