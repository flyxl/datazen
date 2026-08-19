# DataZen Theme Packs

Source tree for declarative ThemePacks (v1). These are **not** compiled into the app; zip a pack directory and import it from **Settings → Theme pack → Import…**.

See [`docs/architecture/backend/theme.md`](../../docs/architecture/backend/theme.md).

## Packs

| Id | Name | Modes | Look |
|----|------|-------|------|
| `community.dracula` | Dracula | dark | Purple / pink accents on charcoal |
| `community.nord` | Nord | dark | Cool arctic blues & frost |
| `community.tokyo-night` | Tokyo Night | dark | Indigo storm + neon blue |
| `community.solarized-light` | Solarized Light | light | Warm paper Solarized |
| `community.paper` | Paper | light | Clean gray + ink-blue |

## Token contract

Theme packs must mirror Host semantic tokens in `src/styles/themes.css`:

- Surface / text: `--c-surface`, `--c-surface-alt`, `--c-surface-raised`, `--c-surface-inset`, `--c-edge`, `--c-fg`, `--c-fg-secondary`, `--c-fg-muted`, `--c-accent`, `--c-success`, `--c-warning`, `--c-danger`, `--c-titlebar`
- Fonts: `--font-sans`, `--font-mono`, `--font-editor`
- CodeMirror (also overridable via `editor.json`): `--cm-*` variables listed above

Dark packs target `.dark { … }`; light packs target `:root { … }`. Host applies packs at runtime via `applyThemePack()` (`src/lib/themePackApply.ts`).

## Icons (optional overrides)

Pack icons use semantic IDs from `src/lib/iconIds.ts` (`UI_ICON_IDS`). Host falls back to Lucide when a pack omits an asset.

### Shipped in community packs (full v1 UI sample, except per-driver `db.*`)

| Semantic id | Typical use |
|-------------|-------------|
| `nav.connections` | Unified workspace — connections nav |
| `action.workflow` | Unified workspace — workflow nav |
| `action.dashboard` | Unified workspace — dashboard nav |
| `nav.settings` | Settings sidebar |
| `query.run` / `query.stop` | Query toolbar (`ThemedIcon` when used) |
| `ai.chat` | AI settings section |
| `theme.light` / `theme.dark` / `theme.system` | Title bar theme toggle |
| `db.postgresql` | PostgreSQL driver tile (example driver icon) |

### Host catalog not yet sampled in community packs

Lucide fallback applies for: `action.backup`, `action.sync`, `action.refresh`, `action.newConnection`, and other `db.<driverType>` icons.

## Layout (per pack)

```
{packId}/
  manifest.json
  tokens.css
  editor.json      # optional CodeMirror overlay
  charts.json      # optional series palettes
  icons/
    nav.connections.svg   # unified workspace nav
    action.workflow.svg
    action.dashboard.svg
    nav.settings.svg
    query.run.svg
    query.stop.svg
    ai.chat.svg
    theme.light.svg
    theme.dark.svg
    theme.system.svg
    db.postgresql.svg
```

Allowed extensions: `.css` `.json` `.svg` `.png` `.webp` `.woff2` `.woff`  
Forbidden: `.js` / `.wasm` / `.ico` / `.icns`

## Zip for install

From this directory:

```bash
# one pack
node ../../scripts/pack-community-theme.mjs community.dracula

# all packs → packages/themes/dist/*.zip
node ../../scripts/pack-community-theme.mjs --all
```

Then in DataZen: **Settings → Theme pack → Import…** and select the zip.  
For dark packs, switch appearance mode to **Dark** (or System in dark OS); for light packs use **Light**.

## Notes

- `tokens.css` for dark packs targets `.dark`; light packs target `:root`.
- Font family names in CSS are stack preferences only (no bundled `.woff2` in these samples).
- Icon files use the pack accent color; Host still falls back to Lucide / driver defaults for other semantic ids listed in `src/lib/iconIds.ts`.
- CI validates community packs against the Host token contract via `src/lib/__tests__/communityThemePacks.test.ts` and `validates_all_community_theme_packs` (Rust, `src-tauri/src/theme/validate.rs`).
