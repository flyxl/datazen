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

## Layout (per pack)

```
{packId}/
  manifest.json
  tokens.css
  editor.json      # optional CodeMirror overlay
  charts.json      # optional series palettes
  icons/
    nav.settings.svg
    query.run.svg
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
- Icon files use the pack accent color; Host still falls back to Lucide / driver defaults for other semantic ids.
