#!/bin/zsh
# Build one arch slice of the engine + panel.
#
#   packaging/build-arch.zsh <arm64|x86_64> <log-file> <output-binary>
#
# The host arch is built without --target so it reuses target/release (a
# cross-compiled slice starts a fresh build directory). Prints a short line on
# success; cargo and swiftc output goes to the log, whose tail is shown on
# failure.
set -e
ARCH=$1
LOG=$2
OUT=$3
cd "$(dirname "$0")/.."

case $ARCH in
arm64) RT=aarch64-apple-darwin ;;
x86_64) RT=x86_64-apple-darwin ;;
*)
  echo "unknown arch '$ARCH' (arm64 or x86_64)"
  exit 1
  ;;
esac

LIBDIR=target/release
CARGO_TARGET=()
if [ "$ARCH" != "$(uname -m)" ]; then
  rustup target list --installed | grep -qx "$RT" || {
    echo "the $ARCH slice needs the Rust target: rustup target add $RT"
    exit 1
  }
  CARGO_TARGET=(--target "$RT")
  LIBDIR=target/$RT/release
fi

# Do NOT export MACOSX_DEPLOYMENT_TARGET for cargo: with Rust 1.91 it makes
# release builds reject their own proc-macro dylibs. swiftc's -target sets the
# app's minimum. C/asm in dependencies (ring) still needs a minimum, or ld warns
# "built for newer macOS".
unset MACOSX_DEPLOYMENT_TARGET
# Must build into this repo's target/ — a redirected CARGO_TARGET_DIR leaves
# packaging looking at a stale libopencast_ffi.a (wrong About version).
unset CARGO_TARGET_DIR
export "CFLAGS_${RT//-/_}"="-mmacosx-version-min=14.0"

# One cargo invocation: build and print the native libs the Swift link needs.
# (A second `cargo rustc` used to rebuild the crate; this does not.)
if ! cargo rustc --release -p opencast-ffi $CARGO_TARGET -- --print native-static-libs >"$LOG" 2>&1; then
  tail -20 "$LOG"
  echo "== Rust build failed for $ARCH (see $LOG) =="
  if grep -q "can't find crate for .*_derive\|can't find crate for .*_impl\|can't find crate for .*_macro" "$LOG"; then
    echo "   rustc could not load proc-macro crates. Is MACOSX_DEPLOYMENT_TARGET set? Unset it, run"
    echo "   cargo clean --release, and retry (./diagnose-build.command narrows it down)."
  fi
  exit 1
fi
FLAGS=$(awk -F'native-static-libs: ' '/native-static-libs:/ { print $2; exit }' "$LOG")
[ -f "$LIBDIR/libopencast_ffi.a" ] || {
  echo "no $LIBDIR/libopencast_ffi.a — see $LOG"
  exit 1
}

mkdir -p "$(dirname "$OUT")"
eval swiftc -parse-as-library -O -target "${ARCH}-apple-macos14.0" \
  -import-objc-header macos/opencast.h \
  -framework AppKit -framework SwiftUI -framework IOKit -framework ApplicationServices \
  macos/*.swift \
  -L "$LIBDIR" -lopencast_ffi $FLAGS \
  -o "$OUT" >>"$LOG" 2>&1 || {
  tail -20 "$LOG"
  echo "== panel build failed for $ARCH (see $LOG) =="
  exit 1
}
echo "== $ARCH slice built =="
