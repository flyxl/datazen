# Database badge icons

Per-driver database badge SVGs live alongside each path driver:

`packages/drivers/{driverId}/ui/icons/{dbType}.svg`

At build time, `scripts/resolve-drivers.mjs` scans those files and injects `DRIVER_ICON_ENTRIES` into `src/plugins/generated.ts`. Protocol-reuse types without their own SVG get `DRIVER_ICON_PARENTS` instead of a silent parent-file alias; `DbTypeBadge` composites parent icon + `shortLabel`. UI resolves icons via `getDriverIconMap()` / `getDriverIconParents()` (theme pack → driver map → parent+shortLabel → shortLabel fallback).

Icons follow a 24×24 viewBox with `rx="5"` rounded rect background and a centered mark (Simple Icons CC0 paths, official brand marks, or parent+shortLabel when no mark exists).

See also: [`docs/architecture/frontend/components.md`](../../../docs/architecture/frontend/components.md).
