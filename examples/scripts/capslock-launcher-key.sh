#!/bin/bash
# @opencast.title Use Caps Lock as Launcher Key
# @opencast.mode compact
# @opencast.icon ⇪
# @opencast.description Remap Caps Lock to F18 (until reboot), so hotkey = "F18" opens OpenCast

# 0x700000039 = Caps Lock, 0x70000006D = F18
hidutil property --set '{"UserKeyMapping":[{"HIDKeyboardModifierMappingSrc":0x700000039,"HIDKeyboardModifierMappingDst":0x70000006D}]}' >/dev/null
echo 'Caps Lock now sends F18 — set hotkey = "F18" in config.toml and run Reload Commands'
