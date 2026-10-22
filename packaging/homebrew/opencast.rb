# Homebrew cask template. `release.command` prints the version and sha256 to
# paste here; publish it in a tap repo (github.com/<you>/homebrew-tap) as
# Casks/opencast.rb, then: brew install --cask <you>/tap/opencast
cask "opencast" do
  version "0.2.0"
  sha256 "REPLACE_WITH_SHA256_FROM_release.command"

  url "https://github.com/OWNER/opencast/releases/download/v#{version}/OpenCast-#{version}.dmg"
  name "OpenCast"
  desc "Open-source keyboard launcher with bring-your-own AI"
  homepage "https://github.com/OWNER/opencast"

  depends_on macos: ">= :sonoma"

  app "OpenCast.app"

  uninstall quit: "dev.opencast.launcher"

  zap trash: [
    "~/.config/opencast",
    "~/Library/Preferences/dev.opencast.launcher.plist",
  ]
end
