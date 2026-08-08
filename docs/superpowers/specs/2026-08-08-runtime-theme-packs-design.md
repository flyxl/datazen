# Design: Runtime Theme Packs (Appearance Plugins)

**Date:** 2026-08-08  
**Status:** Approved  
**Branch (implementation):** TBD (isolated worktree recommended)

## Goals

1. Allow users to **download and enable appearance packs from a store at runtime** (no app rebuild).
2. Keep **driver plugins and theme packs fully independent** (separate install paths, registries, lifecycles).
3. Support **customizable icons** for:
   - Functional UI icons (semantic IDs)
   - Database-type badges (`db.<databaseType>`)
4. **Never** allow theme packs to change OS / installer / tray **application icons**.

## Non-goals

- Compile-time theme packs via `plugins-registry.json` / Cargo features.
- Arbitrary JavaScript execution inside theme packs (v1).
- Replacing the light/dark/system **mode** axis (packs compose with mode).
- Theming native macOS menu chrome beyond what the OS allows.
- Driver protocol / IPC changes for theming.

## Decisions (approved)

| Topic | Choice |
|-------|--------|
| Distribution | Runtime store download → install under app data |
| Relation to drivers | Independent channels; no shared registry |
| Pack model | **Declarative ThemePack (Approach A)** |
| UI icons | Semantic ID → SVG (theme override) |
| DB badges | Driver-provided default → theme override if present |
| App icons | Permanently immutable by themes |
| Code execution | No theme JS in v1 (CSS + JSON + SVG only) |

## Icon resolution

### Functional UI

```
themePack.icons[<semanticId>]  →  Host built-in Lucide (or current component)  →  empty placeholder
```

Semantic IDs are stable host contracts (examples; full table maintained in host docs):

- `nav.settings`, `nav.connections`, `query.run`, `query.stop`, `ai.chat`, …

### Database type badges

```
themePack.icons["db." + databaseType]  →  driver-provided default icon  →  Host placeholder (shortLabel + colors)
```

- **Drivers** ship default SVG (or equivalent) with their frontend meta/assets.
- **Theme packs** may optionally override `db.postgresql`, `db.kiwi`, `db.superset`, …
- Unknown / new drivers without theme art still render via driver default.

### Forbidden

Any pack field targeting application / bundle / tray / `.icns` / `.ico` is ignored and rejected at validation.

## Pack format

```
{packId}/
  manifest.json
  tokens.css          # overrides --c-* (and documented aliases)
  icons/
    nav.settings.svg
    query.run.svg
    db.postgresql.svg
    db.kiwi.svg
    …
  editor.json         # optional CodeMirror token colors
  charts.json         # optional series palettes
  preview.png         # optional store/settings preview
```

### `manifest.json` (v1)

```json
{
  "id": "community.dracula",
  "name": "Dracula",
  "version": "1.0.0",
  "apiVersion": 1,
  "modes": ["dark"],
  "author": "…",
  "description": "…"
}
```

- `apiVersion`: host rejects incompatible packs.
- `modes`: which appearance modes this pack supplies tokens for (`light` | `dark`; both allowed).

## Host architecture

```
Store download
    → validate zip (size, extension allowlist, no path traversal, no .js/.wasm)
    → extract to {appData}/themes/{id}/
ThemeService
    → listInstalled() / enable(id) / disable()
    → apply(mode × pack):
         inject <style id="datazen-theme-pack"> from tokens.css
         register icon map for IconResolver
         reconfigure CodeMirror + chart palettes if files present
Settings
    → mode: light | dark | system  (existing)
    → packId: string | null        (new; null = built-in default)
```

### Independence from driver plugins

| | Drivers | Theme packs |
|--|---------|-------------|
| Install | Build-time / `plugins-registry` | Runtime `{appData}/themes` |
| Code | Rust + UI modules | CSS / SVG / JSON only |
| Discovery | `generated.ts` / inventory | `ThemeService` scan + settings |
| Failure | Missing driver = no connection type | Missing theme = fall back to built-in |

A theme pack must not import driver crates or require `DATAZEN_PLUGINS`. A driver must not require a theme pack to function.

## Prerequisites (host hardening before rich packs)

Without these, packs only partially apply:

1. Unify semantic CSS tokens; map status colors to `--c-success` / `--c-danger` / …
2. CodeMirror / SqlCodeBlock read from CSS vars or `editor.json` contract.
3. Chart/ER vars aligned to `--c-*`; optional `charts.json`.
4. `applyTheme()` supports `mode × pack`; sync webview background (not OS app icon).
5. Introduce `IconResolver` keyed by semantic ID for toolbar/nav; DB badge component uses resolver + driver default.

Phasing:

1. **Foundation** — tokens + IconResolver + driver default SVG hook  
2. **Local packs** — install from file / data dir, Settings picker  
3. **Store** — browse/download/update (separate product surface)

## Security

- Allowlist: `.css`, `.svg`, `.json`, `.png`, `.webp` (preview only).
- Reject: `.js`, `.mjs`, `.ts`, `.wasm`, native binaries, symlinks escaping root.
- Cap unpacked size and file count.
- Sanitize SVG (no `<script>`, no external URL fetches in v1).
- CSP: prefer inline injected CSS from local files; no remote stylesheet URLs by default.

## Settings / persistence

Extend settings (frontend + Rust `AppSettings`):

```ts
theme: {
  mode: 'light' | 'dark' | 'system';  // migrate from flat theme string
  packId: string | null;
}
```

Migration: existing `theme: 'dark'` → `{ mode: 'dark', packId: null }`.

## Testing

- Unit: icon resolution order (theme → driver → placeholder).
- Unit: pack validation (reject JS, path traversal, oversize).
- Unit: token application toggles CSS variables under `:root` / `.dark`.
- E2E (later): install pack from fixture zip, enable, assert semantic surface + one DB badge override.

## Success criteria

- User can install a pack without rebuilding the app.
- Enabling a pack changes CSS tokens and overrides listed semantic / `db.*` icons.
- Driver without theme art still shows its own default icon.
- Application icons unchanged under any pack.
- Disabling / removing a pack restores built-in appearance without touching driver installs.

## Open follow-ups (post-v1)

- Store CDN, signing, and update channels.
- Per-mode icon variants (`icons/dark/nav.settings.svg`) if needed.
- Community pack lint CLI matching host `apiVersion`.
