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
  echo "Rust is missing. Easiest path (no compiler needed):"
  echo "  brew tap a1ex-var1amov/tap && brew trust a1ex-var1amov/tap && brew install --cask opencast"
  echo "Or install Rust from https://rustup.rs and re-run this script."
  exit 1
fi
if ! command -v swiftc >/dev/null; then
  echo "swiftc is missing. Install the Xcode command-line tools:"
  echo "  xcode-select --install"
  echo "Or use the prebuilt cask: brew install --cask a1ex-var1amov/tap/opencast"
  exit 1
fi

./build-app.command
open "$HOME/Applications/OpenCast.app"
echo "== Opened OpenCast =="
