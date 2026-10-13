#!/bin/bash
# @opencast.title Toggle Dark Mode
# @opencast.mode silent
# @opencast.icon 🌗
# @opencast.description Switch macOS between light and dark appearance

osascript -e 'tell application "System Events" to tell appearance preferences to set dark mode to not dark mode'
