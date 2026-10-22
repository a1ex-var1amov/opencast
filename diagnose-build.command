#!/bin/zsh -l
# Narrows down "can't find crate for `*_derive`" in release builds.
# Each variant uses its own target dir. Prints PASS/FAIL per variant; full logs in target/diag-*.log
cd "$(dirname "$0")"
command -v cargo >/dev/null || { [ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"; }
echo "== toolchain =="
which -a cargo rustc | uniq
rustc -vV | grep -E "release|host"
cargo -V
echo "== env =="
env | grep -iE '^(cargo|rust|macosx|sdkroot|cc|cflags|ldflags)' | sed 's/=.*TOKEN.*/=<redacted>/'
echo "== cargo config files =="
for f in ~/.cargo/config.toml ~/.cargo/config .cargo/config.toml .cargo/config; do [ -f "$f" ] && { echo "--- $f"; cat "$f"; }; done
echo

run() { # name, then env assignments, then cargo args
  local name=$1; shift
  local log=target/diag-$name.log
  mkdir -p target
  ( env "$@" ) > "$log" 2>&1
  if grep -q "can't find crate for" "$log"; then echo "FAIL  $name  (proc-macro not found)"
  elif grep -q "^error" "$log"; then echo "FAIL  $name  (other error: $(grep -m1 '^error' "$log"))"
  else echo "PASS  $name"; fi
}

# The crate that fails is zerocopy (via reqwest) — opencast-core is enough to reproduce, and faster.
run release-plain          CARGO_TARGET_DIR=target/diag-a cargo build --release -p opencast-core
run release-deploy-target  CARGO_TARGET_DIR=target/diag-b MACOSX_DEPLOYMENT_TARGET=14.0 cargo build --release -p opencast-core
run release-no-lto         CARGO_TARGET_DIR=target/diag-c MACOSX_DEPLOYMENT_TARGET=14.0 CARGO_PROFILE_RELEASE_LTO=false cargo build --release -p opencast-core
run release-j1             CARGO_TARGET_DIR=target/diag-d MACOSX_DEPLOYMENT_TARGET=14.0 cargo build --release -p opencast-core -j1
run debug-deploy-target    CARGO_TARGET_DIR=target/diag-e MACOSX_DEPLOYMENT_TARGET=14.0 cargo build -p opencast-core

echo
echo "== failing rustc invocation (first one) =="
touch ~/.cargo/registry/src/*/zerocopy-0.8.62/src/lib.rs 2>/dev/null   # force zerocopy to recompile
CARGO_TARGET_DIR=target/diag-a cargo build --release -p opencast-core -v 2>&1 \
  | grep -m1 -E "Running .*--crate-name zerocopy " | cut -c1-3000
