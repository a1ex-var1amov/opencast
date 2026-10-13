#!/bin/zsh -l
# Builds and launches OpenCast in dev mode. Double-click from Finder, or run in a terminal.
cd "$(dirname "$0")"
echo "== OpenCast: checking toolchain =="
command -v cargo >/dev/null || { [ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"; }
for t in cargo node npm; do
  command -v $t >/dev/null || { echo "Missing $t — install it and re-run."; read -k1; exit 1; }
done
cargo --version; node --version
echo "== Core tests =="
cargo test -p opencast-core 2>&1 | tee build-tests.log | tail -25
echo "== npm install =="
npm install 2>&1 | tee build-npm.log | tail -8
echo "== Launching (first build compiles Tauri: a few minutes) =="
npm run tauri dev 2>&1 | tee build-tauri.log
