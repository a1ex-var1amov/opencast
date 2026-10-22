#!/bin/bash
# A Script Filter (Alfred-style): prints a JSON list, OpenCast shows the rows,
# Return re-runs this script with OPENCAST_ACTION=run and the chosen arg as $1.
#
# @opencast.title Recent Branches
# @opencast.mode list
# @opencast.icon 🌿
# @opencast.description Switch branches in the repo under REPO (default: ~/code)
# @opencast.action_mode compact

REPO="${REPO:-$HOME/code}"
cd "$REPO" 2>/dev/null || { echo '[{"title":"Set REPO in this script","subtitle":"'"$REPO"' is not a git repo"}]'; exit 0; }

if [ "$OPENCAST_ACTION" = "run" ]; then
  git checkout "$1" >/dev/null 2>&1 && echo "Switched to $1" || { echo "Could not switch to $1"; exit 1; }
  exit 0
fi

# Default action: list. $1 holds the query when @opencast.filter is false.
git branch --sort=-committerdate --format='%(refname:short)' 2>/dev/null | head -30 | while IFS= read -r b; do
  printf '{"title":"%s","subtitle":"git checkout","arg":"%s"}\n' "$b" "$b"
done | paste -sd, - | sed 's/^/[/; s/$/]/'
