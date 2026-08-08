# Database badge icons (legacy host copy)

Default per-driver database badge SVGs now live alongside each path driver:

`packages/drivers/{driverId}/ui/icons/{dbType}.svg`

At build time, `scripts/resolve-drivers.mjs` scans those files and injects `DRIVER_ICON_ENTRIES` into `src/plugins/generated.ts`. UI code resolves icons via `getDriverIconMap()` (theme pack → driver map → shortLabel fallback).

The SVG files in this directory are **legacy copies** kept temporarily for reference. They are no longer the source of truth for `getDriverIconMap()`. After Task 2 (resolve-drivers icon scanning) is verified, these host SVGs may be deleted; only this README needs to remain as a pointer.

New icons follow a 24×24 viewBox with `rx="5"` rounded rect background and a centered mark (Simple Icons CC0 paths or hand-drawn geometry where no slug exists).
