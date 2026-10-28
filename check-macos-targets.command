#!/bin/zsh -l
# Cross-macOS readiness using the active Xcode toolchain (swiftc / SDK).
# Typechecks the panel against every deployment target we claim to support.
# Does not run the app on older OSes — that needs a real Tahoe/Sonoma machine.
set -e
cd "$(dirname "$0")"
command -v swiftc >/dev/null || { echo "swiftc missing (install Xcode)"; exit 1; }

SDK=$(xcrun --show-sdk-path)
SDK_VER=$(xcrun --show-sdk-version)
ARCH=$(uname -m)
echo "== Xcode SDK $SDK_VER ($SDK) =="
echo "== Host $ARCH, checking deploy targets =="

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
