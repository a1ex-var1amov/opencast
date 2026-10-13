#!/bin/zsh -l
# Builds a release OpenCast.app and installs it to ~/Applications, then launches it.
cd "$(dirname "$0")"
command -v cargo >/dev/null || { [ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"; }
echo "== Stopping dev instance (if any) =="
pkill -f "target/debug/opencast" 2>/dev/null; pkill -f "vite" 2>/dev/null; pkill -x opencast 2>/dev/null; sleep 1
# Sign with any local code-signing identity (a self-signed one is enough): an
# ad-hoc signature changes every build, and macOS then forgets the app's
# Accessibility / Screen Recording grants.
if [ -z "$APPLE_SIGNING_IDENTITY" ]; then
  ID=$(security find-identity -v -p codesigning 2>/dev/null | sed -n 's/.*"\(.*\)".*/\1/p' | head -1)
  [ -n "$ID" ] && export APPLE_SIGNING_IDENTITY="$ID" && echo "== Signing as $ID =="
fi
echo "== Building release bundle (a few minutes) =="
npm run tauri build -- --bundles app 2>&1 | tee build-app.log | tail -15
BUILD_OK=${pipestatus[1]}
APP=target/release/bundle/macos/OpenCast.app
if [ "$BUILD_OK" = 0 ] && [ -d "$APP" ]; then
  rm -rf "$HOME/Applications/OpenCast.app"
  ditto "$APP" "$HOME/Applications/OpenCast.app"
  echo "== Installed to ~/Applications/OpenCast.app ==" | tee -a build-app.log
  open "$HOME/Applications/OpenCast.app"
else
  echo "== Build failed, see build-app.log ==" | tee -a build-app.log
fi
