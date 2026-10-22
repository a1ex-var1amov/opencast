#!/bin/zsh -l
# Quick health check: core tests, lints, then OpenCast Doctor on your real
# config (read-only). Run before ./build-app.command when something feels off.
cd "$(dirname "$0")"
command -v cargo >/dev/null || { [ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"; }
unset MACOSX_DEPLOYMENT_TARGET
fail=0
echo "== Tests =="
cargo test -p opencast-core --quiet 2>&1 | grep -E "test result|FAILED|panicked|error" || fail=1
echo "== Lints =="
cargo clippy -p opencast-core -p opencast-ffi --quiet -- -D warnings 2>&1 | tail -20
[ ${pipestatus[1]} = 0 ] || fail=1
echo "== Doctor (your config) =="
cargo run --release --quiet -p opencast-core --example doctor || fail=1
if [ $fail = 0 ]; then echo "\n== All good =="; else echo "\n== Something needs attention (above) =="; fi
exit $fail
