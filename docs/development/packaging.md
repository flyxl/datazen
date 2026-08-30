# Packaging and release channels

Templates live in-repo; publishing the tap / winget-pkgs PR is a release ops step.

## Templates

| Channel | Template |
|---------|----------|
| Homebrew Cask (Basic DMG) | [`packaging/homebrew/datazen.rb`](../../packaging/homebrew/datazen.rb) |
| WinGet singleton (Basic NSIS) | [`packaging/winget/Flyxl.DataZen.yaml`](../../packaging/winget/Flyxl.DataZen.yaml) |

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

---

## macOS: Gatekeeper, quarantine, and notarization

### User workaround (unsigned / not-notarized builds)

Current public releases are **code-signed** but **not Apple-notarized**. macOS Gatekeeper may show “已损坏” / “无法验证开发者” on first launch.

**Option A — clear quarantine (recommended for Homebrew/manual install):**

```bash
xattr -cr /Applications/DataZen.app
```

**Option B — one-time open:** Right-click the app → **Open** → confirm in the dialog.

The Homebrew cask template runs `xattr -cr` in `postflight` (see [`packaging/homebrew/datazen.rb`](../../packaging/homebrew/datazen.rb)). GitHub release bodies include the same instructions (`.github/workflows/release.yml`).

### Notarization checklist (release ops — docs only)

Do **not** commit signing secrets. When enabling notarization in CI or manual release:

| Step | Action |
|------|--------|
| 1. Apple Developer | Enroll in Apple Developer Program; create **Developer ID Application** certificate |
| 2. Keychain / CI secret | Export cert + private key as `.p12`; store as GitHub **release** environment secret (e.g. `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`) |
| 3. Notary credentials | App-specific password or App Store Connect API key (`APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_API_KEY_*`) |
| 4. Sign app bundle | `codesign --force --deep --options runtime --sign "Developer ID Application: …" DataZen.app` |
| 5. Notarize | `xcrun notarytool submit DataZen.dmg --apple-id … --team-id … --password … --wait` (or API key variant) |
| 6. Staple | `xcrun stapler staple DataZen.dmg` (and/or staple the `.app` before bundling) |
| 7. Verify | `spctl -a -vv -t install /Applications/DataZen.app` → `accepted` / `source=Notarized Developer ID` |
| 8. User docs | Once notarized, update release notes to **remove** mandatory `xattr` steps; keep as fallback for side-loaded copies |

Tauri v2 signing hooks live in `tauri.conf.json` (`bundle.macOS.signingIdentity`, etc.). Updater minisign keys ([`docs/updater.md`](updater.md)) are **independent** of Apple code signing.

---

## Linux package channels

GitHub Releases ship **x86_64** artifacts for each SKU:

| Format | Install | Notes |
|--------|---------|-------|
| **AppImage** | `chmod +x DataZen_*.AppImage && ./DataZen_*.AppImage` | May require `libfuse2` on some distros |
| **deb** | `sudo apt install ./DataZen_*_amd64.deb` | Debian/Ubuntu derivatives |
| **rpm** | `sudo rpm -i ./DataZen_*_x86_64.rpm` | RHEL/Fedora/openSUSE |

Runtime dependency: **WebKitGTK** (e.g. `libwebkit2gtk-4.1-0` / `webkit2gtk4.1`).

There is **no** official distro repository yet — install from [GitHub Releases](https://github.com/flyxl/datazen/releases). In-app auto-update ([`docs/updater.md`](updater.md)) applies to **Basic** builds only (AppImage + `.sig`); `.deb` / `.rpm` users reinstall from releases or future package repos.

Flatpak / Snap / AUR are not maintained in-repo; community packages should track the same release URLs and SHA256.

---

## Windows

- **NSIS `.exe`** installers and **portable `.zip`** archives on GitHub Releases (Basic / All / Akulaku suffixes).
- Portable archives are installation-free and contain `DataZen.exe` plus the required prompt resources. Extract the whole archive before running. User data is still stored in the system application-data directory, not beside the executable.
- Windows requires the Microsoft Edge WebView2 Runtime. It is normally present on supported Windows 10/11 systems; install it separately if the portable build cannot start because the runtime is missing.
- Portable archives are not Tauri updater bundles. The Windows entry in `latest.json` continues to reference the signed Basic NSIS `.exe`.
- **WinGet:** manifest template in [`packaging/winget/`](../../packaging/winget/); prefer WinGet over Scoop (Scoop not maintained).

---

## Notes

- Updater (`docs/updater.md`) is independent of brew/winget and only serves signed Basic artifacts via `latest.json`.
- Optional drivers and SKU matrix: [`optional-drivers.md`](optional-drivers.md).
