# Auto-update (Tauri Updater)

DataZen ships several release SKUs (**Basic**, **All**, **Akulaku**), each with **its own** in-app update channel: signed bundles plus a matching manifest.

> **Why one manifest per SKU.** `tauri.conf.json` compiles a single updater endpoint into every build, and the Tauri updater picks a manifest entry by _platform_ alone (`darwin-aarch64`, `windows-x86_64`, …) — it has no notion of SKU. With one shared manifest, an `-all` / `-akulaku` install would be handed Basic's bundle and silently replaced by a Basic build, losing every driver Basic does not ship. So each SKU is built against its own endpoint (`scripts/ci-tauri-build.mjs`), publishes its own manifest (`scripts/generate-updater-latest-json.mjs`), and only self-updates when `DATAZEN_UPDATER_CHANNEL` is true (`src/lib/updater.ts`). The SKU list, manifest names and platform sets live in one place: `scripts/release-variants.mjs`.

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

| Secret                               | Purpose                                         |
| ------------------------------------ | ----------------------------------------------- |
| `TAURI_SIGNING_PRIVATE_KEY`          | Private key contents or path (CI uses contents) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Optional minisign password                      |

**Every** matrix job receives the signing key and enables `createUpdaterArtifacts` when it is present; the pubkey compiled into `tauri.conf.json` is identical across SKUs, so one key signs them all. If the secret is missing, builds continue without updater artifacts and log a warning (that SKU then has no channel to publish).

Local signed build (Basic):

```bash
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/datazen.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""   # if applicable
DATAZEN_VARIANT=basic pnpm tauri:build:minimal -- --config '{"bundle":{"createUpdaterArtifacts":true}}'
```

Updater bundles:

| OS      | Artifacts               |
| ------- | ----------------------- |
| macOS   | `*.app.tar.gz` + `.sig` |
| Windows | NSIS `*.exe` + `.sig`   |
| Linux   | AppImage + `.sig`       |

Upload `.sig` files and updater archives to the GitHub release. The release workflow job **`release-updater-json`** runs `scripts/generate-updater-latest-json.mjs` once per SKU and uploads `latest.json` (Basic), `latest-all.json` and `latest-akulaku.json`. Basic is uploaded first so a variant problem can never stall the default channel.

A variant manifest only lists the platforms that SKU actually builds (Akulaku has no Linux leg). For non-Basic SKUs the generator **fails** rather than publishing a partial manifest: `check()` finds no entry for a missing platform and reports "up to date" forever, so those users would silently stop updating.

Verify after publish:

```bash
for m in latest latest-all latest-akulaku; do
  curl -sfL "https://github.com/flyxl/datazen/releases/latest/download/$m.json" \
    | jq '{variant, version, platforms: (.platforms | keys)}'
done
```

## App configuration

- **Endpoints** — `tauri.conf.json` defines the default (`latest.json`); `scripts/ci-tauri-build.mjs` overrides `plugins.updater.endpoints` per SKU at build time. Basic gets no override, so its compiled endpoint is byte-identical to what earlier releases shipped.
- **Variant identity** — `scripts/resolve-drivers.mjs` bakes `DATAZEN_VARIANT` and `DATAZEN_UPDATER_CHANNEL` into the gitignored `src/extensions/generated.ts`, from `--variant=<sku>` / `DATAZEN_VARIANT` (default `custom`).
- **Main window**: builds with a signed updater channel check once at launch and every 24 hours while open. When an update is available, an **Update** button appears above Settings in the left rail; clicking it downloads and installs the update, then restarts DataZen. Settings → General also keeps manual check and install actions. Builds with no published channel (`custom`, private SKUs) show a **manual download** card instead and do not run scheduled checks.
- **Sleep / resume**: `updateReminderStore` records the wall-clock time of each actual check attempt. Focus, becoming visible, and network restoration check whether 24 hours have elapsed. A one-minute local deadline poll also handles resume without browser events; it does not send a network request before the deadline. Busy checks/installations leave the deadline unchanged. Failed requests count as attempts to avoid repeated network requests on focus/online events; the next scheduled attempt is 24 hours later. A backward clock adjustment re-establishes the deadline with one check. The timestamp lives in process memory, so a fresh launch checks again; effect remounts share the same timestamp and do not duplicate a check. Main-window unmount removes listeners and the timer.
- `getUpdateChannel()` in `src/lib/updater.ts` is the single runtime gate: `auto` (has a channel) / `manual` (desktop build without one) / `none` (not a desktop build).
- **Manifest SKU re-check** — before anything is downloaded or installed, `src/lib/updater.ts` compares the manifest's own `variant` field with the build's SKU (`manifestBelongsToBuild()`). The plugin exposes the manifest verbatim as `Update.rawJson`, so this is a second line of defence that does not depend on the endpoint having been configured correctly: a manifest naming another SKU is refused instead of installed, and the refusal is logged as `[updater] refusing update: manifest variant=… build variant=…`. A manifest _without_ the field is accepted for Basic only — Basic's manifest name is inherited from earlier releases, so pre-field manifests are legitimate there, while variant manifests only exist because of per-SKU channels.

## Consistency guard

`pnpm test:release-variants` (`scripts/check-release-variants.mjs`) checks that the four files which must agree about SKUs still do: the release matrix, `tauri.conf.json`, the manifest job in `release.yml`, and the Homebrew / WinGet templates. It covers the failure shapes that are otherwise silent — a variant reading Basic's manifest, a SKU that lost a platform leg, a missing `plugins.updater.pubkey` (which breaks signature verification for every SKU at once), and a packaging template naming an artifact that does not exist. `scripts/__tests__/check-release-variants.test.ts` re-runs it against mutated copies of the real files, so each check is proven to fire. The frozen `0.1.1` manifests under `packaging/winget/manifests/` are intentionally excluded: they record what was submitted for that version.

## Update reminder verification

### Implemented: scheduling regression tests

`packages/frontend/src/lib/__tests__/updateReminderSchedule.test.ts` exercises the real reminder store with the host updater mocked. Fake timers verify launch and the 24-hour boundary. `vi.setSystemTime` jumps two days without advancing JS timers to reproduce suspension; focus, visibility, online, and the first resumed one-minute poll each recover an overdue check. The tests also cover simultaneous events, pending requests, installation in progress, failure recovery, effect cleanup/remount, and backward clock changes. `MainPage.test.tsx` verifies that the actual main-window consumer starts and stops the schedule. `WorkspaceModeSidebar.test.tsx` connects the real scheduler, updater result, Zustand store, and sidebar: it verifies automatic startup discovery, correct button placement, download progress, duplicate-click blocking, removal on success, and retry availability after failure.

Run the focused regression set without retry masking:

```bash
pnpm exec vitest run \
  packages/frontend/src/lib/__tests__/updateReminderSchedule.test.ts \
  packages/frontend/src/windows/connection/__tests__/WorkspaceModeSidebar.test.tsx \
  packages/frontend/src/windows/main/__tests__/MainPage.test.tsx \
  packages/frontend/src/lib/__tests__/updater.test.ts \
  packages/frontend/src/lib/__tests__/updaterVariant.test.ts \
  packages/desktop/src/platform/__tests__/updaterFlow.test.ts \
  --retry=0 --maxWorkers=2
```

### Planned: controlled desktop UI journey

The following harness and UI journey are implementation steps, not existing commands or completed validation:

1. Add `scripts/updater-test-server.mjs` to serve a test-only HTTPS endpoint and update archives on localhost. Configure its certificate as trusted by the test OS. Switch scenarios via a local configuration file, with no production endpoint changes. Log request time, manifest SKU, and artifact requests without credentials.
2. Add fixtures for `available`, `up-to-date`, `wrong-variant`, `server-error`, `invalid-signature`, and `interrupted-download`. Each successful manifest carries the real target version, `variant`, platform URL and exact `.sig` contents. Invalid-signature fixtures deliberately change the signature or archive bytes; they never disable verification.
3. Add a host journey at `e2e/specs/update-reminder.ts`, built via `pnpm tauri:build:webdriver` / the repository E2E runner. Use Basic initially, with the updater endpoint overridden in that test build. Keep the ordinary production interval; the unit tests above cover the accelerated 24-hour boundary.
4. Exercise the whole interaction: launch → fixture reports a newer version → locate `workspace-nav-update` above `workspace-nav-settings` → inspect both collapsed and expanded sidebars in English and Simplified Chinese → click once → show progress and disable duplicate clicks → show failure and allow retry. Verify that no artifact is requested before the click. Drive fixture responses through the actual updater check rather than directly setting `availableVersion`.
5. Run `manual` / `none` builds and a mismatched-SKU fixture: no automatic network check for unsupported channels, and no download/install for a foreign SKU. Reuse the same journey for All and Akulaku with their own manifests.

### Planned: real signed installation

Use a disposable OS user or test machine and the same implementation commit for both source and target builds. This avoids an older released target removing the reminder implementation during the test.

| Input              | Test value / requirement                                                                         |
| ------------------ | ------------------------------------------------------------------------------------------------ |
| Installed source   | `0.3.1-test.1`, containing the reminder changes                                                  |
| Target             | `0.3.1-test.2`, containing the same reminder changes                                             |
| Identity           | Same app identifier, architecture and SKU for both versions                                      |
| Endpoint           | Dedicated trusted HTTPS test endpoint for that SKU                                               |
| Updater signing    | Matching public key in the source build; target archive and `.sig` produced by the signing build |
| macOS distribution | Developer ID signature and accepted notarization for the target app                              |
| Windows / Linux    | NSIS updater on Windows; AppImage on Linux, not deb/rpm                                          |

Build these versions in a temporary checkout with config overlays, without changing the official release tag or production update manifest. The manifest's version must match the actual signed target build. A GitHub Draft release is not an anonymous update feed: download its assets with authenticated `gh` first if needed, then serve them from the isolated test endpoint.

Acceptance sequence:

1. Create a non-sensitive test connection, a favorite SQL query, and a recognizable UI setting in the source build. Record version, SKU, architecture and install path.
2. Start with the `up-to-date` fixture and confirm no reminder; restart with `available` and confirm the reminder appears without downloading anything.
3. Click Update once. Record manifest/archive request counts, visible progress, signature verification, installation and relaunch. Confirm the running binary is `0.3.1-test.2`, the SKU/architecture are unchanged, and the test connection, favorite and setting survive.
4. Return the target's own version from the feed and confirm the reminder disappears on the next check. Repeat invalid signature, interrupted download and foreign-SKU cases from a fresh source installation: each must leave the old app usable, and restoring the valid fixture must permit retry.
5. For a real long-sleep acceptance run, keep the app open, sleep beyond the last attempt's 24-hour deadline, then wake. Confirm one overdue request on focus/visibility or after the poll resumes. Correlate the client attempt and server log, since OS/WebView throttling can delay timer execution. Do not change the machine's global clock as a substitute for this physical sleep test.

Start with macOS arm64 Basic, then cover macOS x64 and the All/Akulaku SKU gates, followed by Windows NSIS and Linux AppImage. Preserve test logs and screenshots in the test run output; do not report the signed-install journey as passed based on mocked tests alone.

## Linux and other install channels

- **In-app updater:** every SKU that builds for Linux publishes **AppImage** + `.sig` in its own manifest (`latest.json` / `latest-all.json`); Akulaku has no Linux leg by design. Install or replace the AppImage when an update is offered.
- **deb / rpm:** Not served by the updater; download new packages from [GitHub Releases](https://github.com/flyxl/datazen/releases). See [`packaging.md`](packaging.md) for install commands and dependencies.
- **Homebrew / WinGet:** Package managers track release tags separately and follow the **Basic** artifacts only; they do not use `latest.json`. Pick one update channel and avoid mixing package-manager updates with in-app installs. If you installed the All/Akulaku installer by hand, keep updating that SKU from Releases — the updater will only ever offer a build of the SKU you installed.

## macOS install vs updater

Release DMGs may be unsigned with respect to **notarization** even when updater artifacts are minisign-signed. If Gatekeeper blocks launch, see [`packaging.md`](packaging.md) (`xattr -cr` workaround). Notarization is a release-ops checklist item, not required for the updater signature chain.

## Troubleshooting

- **Update check fails in dev**: local builds are `DATAZEN_VARIANT=custom` (no channel) and `createUpdaterArtifacts` is off by default; use a release build or pass `--variant=basic` explicitly.
- **Update UI shows a manual download card**: expected — this SKU has no published manifest.
- **Update refused as "belongs to a different DataZen build"**: the manifest named another SKU, so the SKU check refused it (see “Manifest SKU re-check” above). Confirm the build's endpoint points at its own `latest-<sku>.json` and that the manifest's `variant` matches `currentVariant()`. `pnpm test:release-variants` catches the configuration side of this.
- **Signature invalid**: pubkey in `tauri.conf.json` must match the private key used to sign the release.
- **A variant was replaced by Basic**: check that the build really is the SKU it claims (`currentVariant()`), that asset names carry the `-<sku>` suffix, and that the matching `latest-<sku>.json` was uploaded. The per-SKU manifest generator refuses to build a manifest from another SKU's artifacts, so this now fails the release job instead of reaching users.
- **Lost private key**: generate a new pair, update pubkey, and users on old keys cannot receive signed updates until they reinstall manually.
