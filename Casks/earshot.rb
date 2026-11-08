# Homebrew cask for Earshot. The release workflow fills in the version and
# checksum and commits the result to Casks/earshot.rb, so this repository
# works as a tap:
#   brew tap benwbooth/earshot https://github.com/benwbooth/earshot
#   brew install --cask earshot
cask "earshot" do
  version "0.1.3"
  sha256 "8dbd4b8a9980ba939e05df30276a7ed310c273c55c04d3ed3f9522f8dd896efc"

  url "https://github.com/benwbooth/earshot/releases/download/v#{version}/Earshot-#{version}-macos-arm64.dmg"
  name "Earshot"
  desc "Music player for YouTube, Bandcamp, SoundCloud, Mixcloud and the Internet Archive"
  homepage "https://benwbooth.github.io/earshot/"

  depends_on arch: :arm64
  depends_on macos: ">= :ventura"

  app "Earshot.app"

  # Earshot isn't notarized by Apple; let it open without the "damaged app" warning.
  postflight do
    system_command "/usr/bin/xattr", args: ["-dr", "com.apple.quarantine", "#{appdir}/Earshot.app"]
  end

  zap trash: [
    "~/Library/Application Support/earshot",
    "~/Library/Preferences/com.earshot.Earshot.plist",
  ]
end
