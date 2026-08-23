# Auto-update (Tauri Updater)

DataZen Basic builds support in-app updates via [tauri-plugin-updater](https://v2.tauri.app/plugin/updater/). Updates are fetched from GitHub Releases (`latest.json` + signed bundles).

## Key generation

Generate a minisign key pair once (keep the private key secret):

```bash
pnpm tauri signer generate -w ~/.tauri/datazen.key --ci -p ""
```

This writes:

- **Private key** — e.g. `~/.tauri/datazen.key` (never commit)
- **Public key** — e.g. `~/.tauri/datazen.key.pub`; paste into `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`

Upload secrets to the GitHub **release** environment:

```bash
gh secret set TAURI_SIGNING_PRIVATE_KEY --env release < ~/.tauri/datazen.key
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --env release --body ""
```

## CI / release signing

Store secrets in the GitHub **release** environment:

| Secret | Purpose |
|--------|---------|
| `TAURI_SIGNING_PRIVATE_KEY` | Private key contents or path (CI uses contents) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Optional minisign password |

The release workflow enables `createUpdaterArtifacts` only for **Basic** matrix jobs when `TAURI_SIGNING_PRIVATE_KEY` is set. If the secret is missing, the build continues without updater artifacts and logs a warning.

Local signed build (Basic):

```bash
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/datazen.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""   # if applicable
pnpm tauri:build:minimal -- --config '{"bundle":{"createUpdaterArtifacts":true}}'
```

Updater bundles:

| OS | Artifacts |
|----|-----------|
| macOS | `*.app.tar.gz` + `.sig` |
| Windows | NSIS `*.exe` + `.sig` (also MSI) |
| Linux | AppImage + `.sig` |

Upload `.sig` files and updater archives to the GitHub release. The release workflow job **`release-updater-json`** runs `scripts/generate-updater-latest-json.mjs` and uploads `latest.json` to the draft release (Basic platforms only).

Verify after publish:

```bash
curl -sfL https://github.com/flyxl/datazen/releases/latest/download/latest.json | jq .
```

## App configuration

- **Endpoint**: `https://github.com/flyxl/datazen/releases/latest/download/latest.json`
- **Settings → General**: “Check for updates” (manual) and optional “Check on startup” (default off)

Only **Basic** SKU builds include the updater; All / Akulaku variants are installed separately.

## Linux and other install channels

- **In-app updater:** Basic Linux builds publish **AppImage** + `.sig` in `latest.json`. Install or replace the AppImage when an update is offered.
- **deb / rpm:** Not served by the updater; download new packages from [GitHub Releases](https://github.com/flyxl/datazen/releases). See [`packaging.md`](packaging.md) for install commands and dependencies.
- **Homebrew / WinGet:** Package managers track release tags separately; they do not use `latest.json`. After upgrading via brew/winget, the in-app updater may still report a newer GitHub Basic build — pick one channel and stick to it, or disable “Check on startup” in Settings.

## macOS install vs updater

Release DMGs may be unsigned with respect to **notarization** even when updater artifacts are minisign-signed. If Gatekeeper blocks launch, see [`packaging.md`](packaging.md) (`xattr -cr` workaround). Notarization is a release-ops checklist item, not required for the updater signature chain.

## Troubleshooting

- **Update check fails in dev**: `createUpdaterArtifacts` is off by default; use a release build.
- **Signature invalid**: pubkey in `tauri.conf.json` must match the private key used to sign the release.
- **Lost private key**: generate a new pair, update pubkey, and users on old keys cannot receive signed updates until they reinstall manually.
