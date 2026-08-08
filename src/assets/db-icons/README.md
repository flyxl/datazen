# Database badge icons

Per-driver database badge SVGs live alongside each path driver:

`packages/drivers/{driverId}/ui/icons/{dbType}.svg`

At build time, `scripts/resolve-drivers.mjs` scans those files and injects `DRIVER_ICON_ENTRIES` into `src/plugins/generated.ts`. UI code resolves icons via `getDriverIconMap()` (theme pack → driver map → shortLabel fallback).

Icons follow a 24×24 viewBox with `rx="5"` rounded rect background and a centered mark (Simple Icons CC0 paths or hand-drawn geometry where no slug exists).
