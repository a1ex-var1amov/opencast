#!/bin/zsh -l
# What Click Text would offer on the screen in front of you, from the same scan the
# app ships (macos/SeekAX.swift). The terminal that runs this needs Accessibility.
# Counts and timings only; add --labels to list the text it found.
cd "$(dirname "$0")"
command -v swiftc >/dev/null || { echo "swiftc not found (Xcode command-line tools)"; exit 1; }
ARCH=$(uname -m)
mkdir -p target/tests
swiftc -O -parse-as-library -target "${ARCH}-apple-macos14.0" \
  macos/SeekAX.swift macos-tests/seek-probe.swift \
  -o target/tests/seek-probe || exit 1
./target/tests/seek-probe "$@"
