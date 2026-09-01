# Homebrew cask template for DataZen Basic (macOS).
# Replace VERSION and SHA256 placeholders after each release.
# Tap: brew tap flyxl/datazen && brew install --cask datazen
#
# After installing, if macOS blocks launch (unsigned app):
#   xattr -cr /Applications/DataZen.app

cask "datazen" do
  version "0.1.1"

  on_arm do
    sha256 "dff00775becbf42161d551022a4b71958e4d5b24b37c086c47364a4ed9d3311b"
    url "https://github.com/flyxl/datazen/releases/download/v#{version}/DataZen_#{version}_aarch64-macos-arm64.dmg"
  end
  on_intel do
    sha256 "f7d459322899ca3f03210211ec15bb883ad79bbc0f70172eab61494b51d05dee"
    url "https://github.com/flyxl/datazen/releases/download/v#{version}/DataZen_#{version}_x64-macos-x64.dmg"
  end

  name "DataZen"
  desc "Cross-platform desktop database client"
  homepage "https://github.com/flyxl/datazen"

  app "DataZen.app"

  zap trash: [
    "~/Library/Application Support/com.tbeasy.datazen",
  ]

  postflight do
    # Clear quarantine xattr so the app opens without "damaged" warnings.
    system_command "/usr/bin/xattr",
                   args: ["-cr", "#{appdir}/DataZen.app"],
                   print_stderr: false
  end
end
