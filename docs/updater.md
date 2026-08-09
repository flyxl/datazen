# Auto-update (Tauri Updater)

DataZen Basic builds support in-app updates via [tauri-plugin-updater](https://v2.tauri.app/plugin/updater/). Updates are fetched from GitHub Releases (`latest.json` + signed bundles).

## Key generation

Generate a minisign key pair once (keep the private key secret):

```bash
pnpm tauri signer generate -w ~/.tauri/datazen.key
```

This writes:

- **Private key** — e.g. `~/.tauri/datazen.key` (never commit)
- **Public key** — printed to stdout; paste into `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`

The repo ships a **placeholder** public key until the first signed release. Replace it before publishing updates users can install.

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

Upload `.sig` files and updater archives to the GitHub release. [tauri-action](https://github.com/tauri-apps/tauri-action) can generate `latest.json` for the configured endpoint.

## App configuration

- **Endpoint**: `https://github.com/flyxl/datazen/releases/latest/download/latest.json`
- **Settings → General**: “Check for updates” (manual) and optional “Check on startup” (default off)

Only **Basic** SKU builds include the updater; All / Akulaku variants are installed separately.

## Troubleshooting

- **Update check fails in dev**: `createUpdaterArtifacts` is off by default; use a release build.
- **Signature invalid**: pubkey in `tauri.conf.json` must match the private key used to sign the release.
- **Lost private key**: generate a new pair, update pubkey, and users on old keys cannot receive signed updates until they reinstall manually.
