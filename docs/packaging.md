# Packaging checklist (Homebrew / WinGet)

Templates live in-repo; publishing the tap / winget-pkgs PR is a release ops step.

## Templates

| Channel | Template |
|---------|----------|
| Homebrew Cask (Basic DMG) | [`packaging/homebrew/datazen.rb`](../packaging/homebrew/datazen.rb) |
| WinGet singleton (Basic NSIS) | [`packaging/winget/Flyxl.DataZen.yaml`](../packaging/winget/Flyxl.DataZen.yaml) |

## After each Basic release

1. Download the published Basic macOS `.dmg` / Windows `.exe` assets from GitHub Releases.
2. Compute SHA256 for each asset (also printed in the release body by `release-checksums`).
3. Update `VERSION` / `sha256` placeholders in both templates (or the live tap / winget PR).
4. Homebrew: push to `flyxl/homebrew-datazen` (`Casks/datazen.rb`), then verify:
   ```bash
   brew tap flyxl/datazen
   brew install --cask datazen
   ```
5. WinGet: open a PR against [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) with the updated manifest.
6. Document any All-SKU packages separately — templates currently target **Basic** only (matches updater scope).

## Notes

- macOS Gatekeeper: cask `postflight` runs `xattr -cr` on the app bundle.
- Updater (`docs/updater.md`) is independent of brew/winget and only serves signed Basic artifacts via `latest.json`.
- Scoop is not maintained yet; prefer WinGet on Windows.
