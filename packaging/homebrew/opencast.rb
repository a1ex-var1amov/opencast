# Homebrew cask template. `./release.command` prints this file with version,
# sha256 and OWNER filled in from your git remote — paste that output into a tap
# repo (github.com/<owner>/<repo>) as Casks/opencast.rb, then:
#   brew install --cask <owner>/tap/opencast
# OWNER stands for "owner/repo", so it replaces the whole path after github.com.
# The dmg is universal (arm64 + x86_64), so one sha256 covers every Mac.
cask "opencast" do
  version "0.2.0"
  sha256 "REPLACE_WITH_SHA256_FROM_release.command"

  url "https://github.com/OWNER/releases/download/v#{version}/OpenCast-#{version}.dmg"
  name "OpenCast"
  desc "Open-source keyboard launcher with bring-your-own AI"
  homepage "https://github.com/OWNER"

  depends_on macos: ">= :sonoma"

  app "OpenCast.app"

  uninstall quit: "dev.opencast.launcher"

  zap trash: [
    "~/.config/opencast",
    "~/Library/Preferences/dev.opencast.launcher.plist",
  ]
end
