# Homebrew cask template for DataZen Basic (macOS).
# Replace VERSION and SHA256 placeholders after each release.
# Tap: brew tap flyxl/datazen && brew install --cask datazen
#
# After installing, if macOS blocks launch (unsigned app):
#   xattr -cr /Applications/DataZen.app

cask "datazen" do
  version "VERSION"

  on_arm do
    sha256 "ARM64_SHA256"
    url "https://github.com/flyxl/datazen/releases/download/v#{version}/DataZen_#{version}_aarch64-macos-arm64.dmg"
  end
  on_intel do
    sha256 "X64_SHA256"
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
