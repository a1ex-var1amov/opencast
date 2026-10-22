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
# Do NOT export MACOSX_DEPLOYMENT_TARGET for cargo: with Rust 1.91 it makes release
# builds reject their own proc-macro dylibs ("can't find crate for `zerocopy_derive`").
# Rust's default (macOS 11) links fine into the app; swiftc's -target sets the app's minimum.
unset MACOSX_DEPLOYMENT_TARGET
# C/asm in dependencies (ring) still needs a minimum, or ld warns "built for newer macOS".
export CFLAGS_aarch64_apple_darwin="-mmacosx-version-min=14.0"
# One cargo invocation: build and print the native libs the Swift link needs.
# (A second `cargo rustc` used to rebuild the crate; this does not.)
cargo rustc --release -p opencast-ffi -- --print native-static-libs 2>&1 | tee build-app.log | grep -v '^note: native-static-libs' | tail -15
BUILD_OK=${pipestatus[1]}
if [ "$BUILD_OK" != 0 ]; then
  echo "== Build failed, see build-app.log ==" | tee -a build-app.log
  if grep -q "can't find crate for .*_derive\|can't find crate for .*_impl\|can't find crate for .*_macro" build-app.log; then
    echo "   rustc could not load proc-macro crates. Is MACOSX_DEPLOYMENT_TARGET set? Unset it, run" | tee -a build-app.log
    echo "   cargo clean --release, and retry (./diagnose-build.command narrows it down)." | tee -a build-app.log
  fi
  exit 1
fi
FLAGS=$(awk -F'native-static-libs: ' '/native-static-libs:/ { print $2; exit }' build-app.log)
APP=target/release/OpenCast.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp macos/Info.plist "$APP/Contents/Info.plist"
cp macos/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
eval swiftc -parse-as-library -O -target arm64-apple-macos14.0 \
  -import-objc-header macos/opencast.h \
  -framework AppKit -framework SwiftUI -framework IOKit -framework ApplicationServices \
  macos/*.swift \
  -L target/release -lopencast_ffi $FLAGS \
  -o "$APP/Contents/MacOS/OpenCast" 2>&1 | tee -a build-app.log | tail -15
BUILD_OK=${pipestatus[1]}
if [ "$BUILD_OK" != 0 ]; then
  echo "== Build failed, see build-app.log ==" | tee -a build-app.log
  exit 1
fi
# Strip local symbols from the final binary (what `strip = true` was meant to do).
strip -x "$APP/Contents/MacOS/OpenCast" 2>/dev/null
if [ -n "$APPLE_SIGNING_IDENTITY" ]; then
  codesign --force --deep --sign "$APPLE_SIGNING_IDENTITY" "$APP" || exit 1
fi
rm -rf "$HOME/Applications/OpenCast.app"
ditto "$APP" "$HOME/Applications/OpenCast.app" || exit 1
echo "== Installed to ~/Applications/OpenCast.app ==" | tee -a build-app.log
open "$HOME/Applications/OpenCast.app"
