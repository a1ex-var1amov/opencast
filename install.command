#!/bin/zsh -l
# Build and install OpenCast to ~/Applications, then launch it.
#
#   git clone git@github.com:a1ex-var1amov/opencast.git
#   cd opencast
#   ./install.command
#
# Needs Rust (stable) and the Xcode command-line tools. Signs with any local
# identity (dev / ad-hoc is enough); not notarized — Gatekeeper may ask you to
# right-click → Open the first time.
set -e
cd "$(dirname "$0")"

command -v cargo >/dev/null || { [ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"; }
if ! command -v cargo >/dev/null; then
  echo "Rust is missing. Install it from https://rustup.rs then re-run."
  exit 1
fi
if ! command -v swiftc >/dev/null; then
  echo "swiftc is missing. Install the Xcode command-line tools:"
  echo "  xcode-select --install"
  exit 1
fi

./build-app.command
open "$HOME/Applications/OpenCast.app"
echo "== Opened OpenCast =="
