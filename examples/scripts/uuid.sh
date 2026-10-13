#!/bin/bash
# @opencast.title Generate UUID
# @opencast.mode compact
# @opencast.icon 🆔
# @opencast.argument1 { "type": "text", "placeholder": "lowercase? (y/n)", "optional": true }

id=$(uuidgen)
[ "$1" = "y" ] && id=$(echo "$id" | tr '[:upper:]' '[:lower:]')
printf '%s' "$id" | (pbcopy 2>/dev/null || wl-copy 2>/dev/null || xclip -selection clipboard 2>/dev/null)
echo "Copied $id"
