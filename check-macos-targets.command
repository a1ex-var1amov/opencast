#!/bin/zsh -l
# Cross-macOS readiness using the active Xcode toolchain (swiftc / SDK).
# Typechecks the panel against every deployment target we claim to support, then
# runs AppKit-free panel logic tests. There is no macOS Simulator for older
# desktop OS versions — live Tahoe GUI still needs a Tahoe machine — but this
# is the Xcode-side gate CI and local "does it compile for 26 and 27" check.
set -e
cd "$(dirname "$0")"
command -v swiftc >/dev/null || { echo "swiftc missing (install Xcode)"; exit 1; }

SDK=$(xcrun --show-sdk-path)
SDK_VER=$(xcrun --show-sdk-version)
ARCH=$(uname -m)
echo "== $(xcodebuild -version 2>/dev/null | tr '\n' ' ') =="
echo "== Xcode SDK $SDK_VER ($SDK) =="
echo "== Host $ARCH, checking deploy targets =="
echo "== Note: typecheck ≠ runtime; Tahoe UI still needs a Tahoe box =="

# Targets we ship for (min is 14.0 in Info.plist / build-arch.zsh).
TARGETS=(macos14.0 macos15.0 macos26.0 macos27.0)
fail=0
for t in $TARGETS; do
  echo "== typecheck arm64-apple-$t =="
  if ! swiftc -typecheck -parse-as-library \
      -sdk "$SDK" \
      -target "arm64-apple-$t" \
      -import-objc-header macos/opencast.h \
      macos/*.swift 2>target/typecheck-$t.log; then
    echo "FAIL arm64-apple-$t (see target/typecheck-$t.log)"
    tail -20 target/typecheck-$t.log
    fail=1
  else
    echo "ok   arm64-apple-$t"
    rm -f target/typecheck-$t.log
  fi
done

echo "== panel logic tests =="
./test-swift.command || fail=1

if [ $fail -eq 0 ]; then
  echo "\n== All deploy-target typechecks + panel tests passed =="
else
  echo "\n== Failures above =="
fi
exit $fail
