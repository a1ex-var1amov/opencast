#!/bin/bash
# @opencast.title Network Addresses
# @opencast.mode fullOutput
# @opencast.icon 🌐
# @opencast.description Local and public IP addresses

echo "Local:"
if command -v ipconfig >/dev/null 2>&1 && [ "$(uname)" = "Darwin" ]; then
  for i in en0 en1; do
    a=$(ipconfig getifaddr "$i" 2>/dev/null) && echo "  $i  $a"
  done
else
  hostname -I 2>/dev/null | tr ' ' '\n' | sed '/^$/d;s/^/  /'
fi
echo
echo "Public:"
echo "  $(curl -fsS --max-time 4 https://api.ipify.org || echo unavailable)"
