# Packaging and release channels

Templates live in-repo; publishing the tap / winget-pkgs PR is a release ops step.

## Templates

| Channel | Template |
|---------|----------|
| Homebrew Cask (Basic DMG) | [`packaging/homebrew/datazen.rb`](../../packaging/homebrew/datazen.rb) |
| WinGet singleton (Basic NSIS) | [`packaging/winget/Flyxl.DataZen.yaml`](../../packaging/winget/Flyxl.DataZen.yaml) |

Both name a **Basic** artifact, and the Windows installer is
`DataZen-{Version}-windows-x64.exe` — there is **no** `-nsis` segment;
`canonical_name()` in `release.yml` appends only the file extension (the `0.1.1`
snapshot under `packaging/winget/manifests/` predates that and keeps its name).
Run `pnpm test:release-variants` before publishing: it fails if a template names
a variant artifact, keeps a retired name, or drifts from the release matrix.

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
6. Document any All-SKU packages separately — templates currently target **Basic** only.
   The updater, by contrast, is per-SKU ([`updater.md`](./updater.md)): each of Basic / All /
   Akulaku publishes its own signed manifest, so no longer suspect an All-SKU package of
   being pulled in by Basic's channel.

---

## macOS: Gatekeeper, quarantine, and notarization

### User workaround (unsigned / not-notarized builds)

Previously published releases remain **code-signed** but **not Apple-notarized**. macOS Gatekeeper may show “已损坏” / “无法验证开发者” on first launch. Releases built after the mandatory notarization workflow is merged will be notarized and stapled.

**Option A — clear quarantine (recommended for Homebrew/manual install):**

```bash
xattr -cr /Applications/DataZen.app
```

**Option B — one-time open:** Right-click the app → **Open** → confirm in the dialog.

The Homebrew cask template runs `xattr -cr` in `postflight` (see [`packaging/homebrew/datazen.rb`](../../packaging/homebrew/datazen.rb)). GitHub release bodies include the same instructions (`.github/workflows/release.yml`).

### GitHub Actions signing and notarization

The macOS matrix in `.github/workflows/release.yml` imports the Developer ID identity from the `release` environment before building. Export the identity, including its private key, as a password-protected `.p12`; do **not** commit the file or any signing secrets.

| GitHub `release` environment secret | Contents |
|------------------------------------|----------|
| `APPLE_CERTIFICATE` | Base64-encoded `.p12` containing the Developer ID Application certificate and private key |
| `APPLE_CERTIFICATE_PASSWORD` | Password used when exporting the `.p12` |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | Required notarization credentials; use an app-specific password for `APPLE_PASSWORD` |

The workflow imports the certificate into a temporary macOS keychain and derives `APPLE_SIGNING_IDENTITY` from it. The macOS build fails if any notarization credential is missing. Tauri notarizes the app bundle during the build; after the workflow injects the first-launch README and rebuilds each DMG, it submits the final DMG to Apple, staples the ticket, and validates it. This ensures the published artifact itself carries a ticket. Verify a notarized app with `spctl -a -vv -t exec /Applications/DataZen.app` and `xcrun stapler validate /Applications/DataZen.app`.

Tauri v2 signing hooks live in `tauri.conf.json` (`bundle.macOS.signingIdentity`, etc.). Updater minisign keys ([`updater.md`](updater.md)) are **independent** of Apple code signing.

---

## Linux package channels

GitHub Releases ship **x86_64** artifacts for each SKU:

| Format | Install | Notes |
|--------|---------|-------|
| **AppImage** | `chmod +x DataZen_*.AppImage && ./DataZen_*.AppImage` | May require `libfuse2` on some distros |
| **deb** | `sudo apt install ./DataZen_*_amd64.deb` | Debian/Ubuntu derivatives |
| **rpm** | `sudo rpm -i ./DataZen_*_x86_64.rpm` | RHEL/Fedora/openSUSE |

Runtime dependency: **WebKitGTK** (e.g. `libwebkit2gtk-4.1-0` / `webkit2gtk4.1`).

There is **no** official distro repository yet — install from [GitHub Releases](https://github.com/flyxl/datazen/releases). In-app auto-update ([`updater.md`](./updater.md)) is per-SKU: every Basic / All / Akulaku release publishes signed AppImage artifacts plus its own manifest, so a Linux install updates within its own SKU. `.deb` / `.rpm` users reinstall from releases or future package repos.

Flatpak / Snap / AUR are not maintained in-repo; community packages should track the same release URLs and SHA256.

---

## Windows

- **NSIS `.exe`** installers and **portable `.zip`** archives on GitHub Releases (Basic / All / Akulaku variants).
- Portable archives are installation-free and contain `DataZen.exe`, prompt resources, and Pro extension resources (`builtin-ep/`). Extract the whole archive before running. User data is still stored in the system application-data directory, not beside the executable.
- Windows requires the Microsoft Edge WebView2 Runtime. It is normally present on supported Windows 10/11 systems; install it separately if the portable build cannot start because the runtime is missing.
- Portable archives are not Tauri updater bundles. Each SKU's manifest references that SKU's own signed NSIS `.exe` — `latest.json` for Basic, `latest-all.json` / `latest-akulaku.json` for the variants.
- **WinGet:** manifest template in [`packaging/winget/`](../../packaging/winget/); prefer WinGet over Scoop (Scoop not maintained).

---

## Notes

- Updater ([`updater.md`](./updater.md)) is independent of brew/winget. It serves one signed manifest per SKU (`latest.json` / `latest-all.json` / `latest-akulaku.json`), while the brew / winget templates below track **Basic** only.
- Optional drivers and SKU matrix: [`optional-drivers.md`](optional-drivers.md).
