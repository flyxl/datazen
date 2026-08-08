# Runtime Theme Packs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Users can install a declarative ThemePack from a local ZIP into `{appData}/themes/{id}/`, enable it in Settings, and see CSS tokens, fonts, semantic UI icons, and DB badges update without recompiling — while driver plugins stay completely independent.

**Architecture:** Host hardens semantic tokens / fonts / IconResolver first; then a Rust `theme` IPC module validates and installs packs under `{appData}/themes`; a frontend ThemeService applies `mode × pack` by injecting `<style id="datazen-theme-pack">`, registering icon URLs, and optionally reconfiguring CodeMirror / chart palettes from `editor.json` / `charts.json`. Store/CDN browsing is **out of scope** (separate plan).

**Tech Stack:** Tauri v2, Rust (`zip`, existing `app_data_archive` guards), React 18, Zustand, CSS variables, Lucide, CodeMirror 6, Vitest, `cargo test -p datazen --lib`.

**Spec:** `docs/superpowers/specs/2026-08-08-runtime-theme-packs-design.md`

## Global Constraints

- Pack model is declarative ThemePack only (CSS / JSON / SVG|PNG|WebP / WOFF2|WOFF). **No theme JS / WASM / ICO / ICNS.**
- Driver plugins and theme packs must not share install paths, registries, or lifecycle; drivers must work with `packId: null`.
- Appearance **mode** (`light` | `dark` | `system`) remains; packs compose with mode, they do not replace it.
- Icon format priority: `.svg` → `.webp` → `.png`. Reject `.ico` / `.icns`.
- Font whitelist: `.woff2`, `.woff` only. `@font-face` `src` must be pack-relative (no remote URLs).
- User-explicit `editorFontFamily` (and any future UI font preference) **wins over** theme `--font-*`.
- Theme packs must never change OS / tray / bundle application icons.
- Reuse zip path validation / bomb limits patterns from `src-tauri/src/app_data_archive.rs`.
- IPC invoke keys use camelCase args matching Rust `serde(rename_all = "camelCase")`.
- Branch / worktree: `feat/runtime-theme-packs` at `.worktrees/runtime-theme-packs`.
- v1 surface for IconResolver migration is **curated** (connection list DB badge, settings section icons, main ActionPanel, ThemeToggle). Full-app Lucide rewrite is out of scope.
- Wholesale migration of every hardcoded `text-red-400` etc. is out of scope; add token aliases + wire CM/charts; migrate only surfaces that block pack visibility.

## File map

| File | Responsibility |
|------|----------------|
| `src/types/theme.ts` | `ThemeMode`, `ThemePreference`, normalize/migrate helpers |
| `src/types/index.ts` | `AppSettings.theme` becomes `ThemePreference` |
| `src-tauri/src/store/mod.rs` | Nested `ThemePreference` + untagged deserialize for legacy string |
| `src/styles/themes.css` | `--font-*`, ensure status tokens; pack-friendly defaults |
| `tailwind.config.ts` | `success` / `warning` / `danger` + fontFamily CSS var bridge |
| `src/lib/iconIds.ts` | Stable semantic ID catalog (v1 subset) |
| `src/lib/iconResolver.ts` | Resolve theme → host/driver → placeholder |
| `src/components/ThemedIcon.tsx` | Renders resolved icon (img / inline / Lucide) |
| `src/components/DbTypeBadge.tsx` | DB badge using `db.<type>` resolution |
| `src/lib/themePackApply.ts` | Inject/remove pack `<style>`, font faces, notify listeners |
| `src/lib/themeEditorColors.ts` | Read CM colors from CSS vars / `editor.json` |
| `src/lib/chart/colors.ts` | Optional `charts.json` override + `--c-*` fallbacks |
| `src/stores/settingsStore.ts` | `applyTheme(mode × packId)`; pack apply hooks |
| `src/commands/theme.ts` | Frontend IPC wrappers |
| `src-tauri/src/theme/mod.rs` | Pack validate / install / list / read file helpers |
| `src-tauri/src/theme/validate.rs` | Extension whitelist, path guards, size limits |
| `src-tauri/src/commands/theme.rs` | Tauri commands |
| `src/windows/settings/SettingsWindow.tsx` | Appearance: mode + pack picker + Import |
| `fixtures/themes/community.fixture-dark/` | Checked-in sample pack for tests |
| Tests under `src/lib/__tests__/`, `src-tauri/src/theme/` | Unit coverage |

**Deferred (separate plan):** theme store browse / CDN download / updates / signatures.

---

### Task 1: Theme preference shape + migration

**Files:**
- Create: `src/types/theme.ts`
- Create: `src/types/__tests__/theme.test.ts`
- Modify: `src/types/index.ts` (`AppSettings`)
- Modify: `src-tauri/src/store/mod.rs` (`AppSettings`, defaults, tests)
- Modify: `src/stores/settingsStore.ts` (read `theme.mode`)
- Modify: all compile-break sites that treat `settings.theme` as a string (ThemeToggle, SettingsWindow, MenuBar, useThemeListener, index.html localStorage still stores **mode** only)

**Interfaces:**
- Produces:
  - `export type ThemeMode = 'light' | 'dark' | 'system'`
  - `export interface ThemePreference { mode: ThemeMode; packId: string | null }`
  - `export function normalizeThemePreference(input: unknown): ThemePreference`
  - `export const DEFAULT_THEME_PREFERENCE: ThemePreference = { mode: 'dark', packId: null }`
  - Rust: `ThemePreference { mode: String, pack_id: Option<String> }` with custom deserializer accepting legacy `"dark"` string

- [ ] **Step 1: Write failing frontend tests**

```ts
import { describe, expect, it } from 'vitest';
import { normalizeThemePreference, DEFAULT_THEME_PREFERENCE } from '../theme';

describe('normalizeThemePreference', () => {
  it('migrates legacy string', () => {
    expect(normalizeThemePreference('dark')).toEqual({ mode: 'dark', packId: null });
    expect(normalizeThemePreference('light')).toEqual({ mode: 'light', packId: null });
    expect(normalizeThemePreference('system')).toEqual({ mode: 'system', packId: null });
  });

  it('keeps object shape', () => {
    expect(normalizeThemePreference({ mode: 'light', packId: 'community.dracula' })).toEqual({
      mode: 'light',
      packId: 'community.dracula',
    });
  });

  it('falls back on garbage', () => {
    expect(normalizeThemePreference(null)).toEqual(DEFAULT_THEME_PREFERENCE);
    expect(normalizeThemePreference({ mode: 'neon' })).toEqual(DEFAULT_THEME_PREFERENCE);
  });
});
```

- [ ] **Step 2: Run test — expect FAIL**

```bash
pnpm exec vitest run src/types/__tests__/theme.test.ts
```

Expected: FAIL (module not found)

- [ ] **Step 3: Implement `src/types/theme.ts` + update `AppSettings`**

```ts
export type ThemeMode = 'light' | 'dark' | 'system';

export interface ThemePreference {
  mode: ThemeMode;
  packId: string | null;
}

export const DEFAULT_THEME_PREFERENCE: ThemePreference = {
  mode: 'dark',
  packId: null,
};

const MODES = new Set<ThemeMode>(['light', 'dark', 'system']);

export function normalizeThemePreference(input: unknown): ThemePreference {
  if (typeof input === 'string' && MODES.has(input as ThemeMode)) {
    return { mode: input as ThemeMode, packId: null };
  }
  if (input && typeof input === 'object') {
    const obj = input as { mode?: unknown; packId?: unknown };
    if (typeof obj.mode === 'string' && MODES.has(obj.mode as ThemeMode)) {
      const packId =
        typeof obj.packId === 'string' && obj.packId.length > 0 ? obj.packId : null;
      return { mode: obj.mode as ThemeMode, packId };
    }
  }
  return { ...DEFAULT_THEME_PREFERENCE };
}
```

In `src/types/index.ts`, change `theme: 'light' | 'dark' | 'system'` → `theme: ThemePreference` (import type).

- [ ] **Step 4: Rust nested theme + legacy string deserialize**

In `src-tauri/src/store/mod.rs`, add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThemePreference {
    pub mode: String,
    #[serde(default)]
    pub pack_id: Option<String>,
}

impl Default for ThemePreference {
    fn default() -> Self {
        Self {
            mode: "dark".into(),
            pack_id: None,
        }
    }
}

fn deserialize_theme<'de, D>(deserializer: D) -> Result<ThemePreference, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(s) if matches!(s.as_str(), "light" | "dark" | "system") => {
            Ok(ThemePreference {
                mode: s,
                pack_id: None,
            })
        }
        other => serde_json::from_value(other).map_err(serde::de::Error::custom),
    }
}
```

Change `AppSettings.theme` to:

```rust
#[serde(deserialize_with = "deserialize_theme", default)]
pub theme: ThemePreference,
```

Update `Default` / `default_for_first_run` accordingly. Add unit test that `{"theme":"dark"}` and `{"theme":{"mode":"dark","packId":null}}` both deserialize.

Fix all Rust call sites that passed `&settings.theme` into menu setup to use `&settings.theme.mode`.

- [ ] **Step 5: Update frontend store / UI call sites**

- `DEFAULT_SETTINGS.theme = DEFAULT_THEME_PREFERENCE`
- `applyTheme(pref.mode)` (localStorage still stores mode string under `datazen-theme`)
- `updateSettings` / ThemeToggle / Settings select write `{ ...settings.theme, mode }` or full preference
- `loadSettings` runs `normalizeThemePreference(settings.theme)` defensively
- Menu / listeners: payload remains mode string for `menu:theme-change`; pack changes go through settings

- [ ] **Step 6: Verify**

```bash
pnpm exec vitest run src/types/__tests__/theme.test.ts
cargo test -p datazen --lib store::tests -q
```

Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/types/theme.ts src/types/__tests__/theme.test.ts src/types/index.ts \
  src/stores/settingsStore.ts src/components/ThemeToggle.tsx \
  src/windows/settings/SettingsWindow.tsx src/hooks/useThemeListener.ts \
  src/components/MenuBar.tsx src-tauri/src/store/mod.rs src-tauri/src/lib.rs
# add any other compile-fix files from tsc/cargo
git commit -m "$(cat <<'EOF'
feat(theme): migrate settings theme to mode + packId preference

EOF
)"
```

---

### Task 2: Host font CSS variables

**Files:**
- Modify: `src/styles/themes.css`
- Modify: `tailwind.config.ts`
- Modify: `src/styles/globals.css` (ensure body uses `font-sans`)
- Modify: `src/components/SqlEditor.tsx` (font family resolution)
- Create: `src/lib/resolveEditorFontFamily.ts`
- Create: `src/lib/__tests__/resolveEditorFontFamily.test.ts`

**Interfaces:**
- Produces:
  - CSS vars on `:root`: `--font-sans`, `--font-mono`, `--font-editor`
  - `export function resolveEditorFontFamily(userSetting: string, computedEditorVar: string, fallback: string): string`
  - Rule: if `userSetting` is non-empty and not equal to the Host built-in default constant, use userSetting; else use `var(--font-editor)` resolved value / fallback

- [ ] **Step 1: Failing test for priority**

```ts
import { describe, expect, it } from 'vitest';
import { resolveEditorFontFamily, HOST_DEFAULT_EDITOR_FONT } from '../resolveEditorFontFamily';

describe('resolveEditorFontFamily', () => {
  it('prefers explicit user setting over theme', () => {
    expect(
      resolveEditorFontFamily('Comic Sans MS', '"Theme Mono"', HOST_DEFAULT_EDITOR_FONT),
    ).toBe('Comic Sans MS');
  });

  it('uses theme when user setting is host default or empty', () => {
    expect(resolveEditorFontFamily(HOST_DEFAULT_EDITOR_FONT, '"Theme Mono"', HOST_DEFAULT_EDITOR_FONT)).toBe(
      '"Theme Mono"',
    );
    expect(resolveEditorFontFamily('', '"Theme Mono"', HOST_DEFAULT_EDITOR_FONT)).toBe('"Theme Mono"');
  });

  it('falls back to host default when theme empty', () => {
    expect(resolveEditorFontFamily(HOST_DEFAULT_EDITOR_FONT, '', HOST_DEFAULT_EDITOR_FONT)).toBe(
      HOST_DEFAULT_EDITOR_FONT,
    );
  });
});
```

- [ ] **Step 2: Run — expect FAIL**

```bash
pnpm exec vitest run src/lib/__tests__/resolveEditorFontFamily.test.ts
```

- [ ] **Step 3: Implement helper + CSS/Tailwind**

`themes.css` add under `:root`:

```css
  --font-sans: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", sans-serif;
  --font-mono: Menlo, Monaco, Consolas, "Courier New", ui-monospace, monospace;
  --font-editor: var(--font-mono);
```

`tailwind.config.ts`:

```ts
fontFamily: {
  sans: ['var(--font-sans)'],
  mono: ['var(--font-mono)'],
},
```

Wire `SqlEditor` `currentThemeConfig().fontFamily` through `resolveEditorFontFamily` reading `getComputedStyle(document.documentElement).getPropertyValue('--font-editor').trim()`.

Keep `DEFAULT_SETTINGS.editorFontFamily` equal to `HOST_DEFAULT_EDITOR_FONT` so fresh installs pick up theme fonts.

- [ ] **Step 4: Tests pass + commit**

```bash
pnpm exec vitest run src/lib/__tests__/resolveEditorFontFamily.test.ts
git add src/lib/resolveEditorFontFamily.ts src/lib/__tests__/resolveEditorFontFamily.test.ts \
  src/styles/themes.css tailwind.config.ts src/components/SqlEditor.tsx
git commit -m "$(cat <<'EOF'
feat(theme): add font CSS variables with user-setting priority

EOF
)"
```

---

### Task 3: Token hardening (status colors + chart CSS vars)

**Files:**
- Modify: `tailwind.config.ts` (add success/warning/danger)
- Modify: `src/lib/chart/colors.ts` (document default palette still hex; add `readCssColor` helper used by export)
- Modify: chart renderers that use wrong var names (`--border-edge` → `--c-edge`, etc.) — grep and fix under `src/components/chart/`
- Modify: `src/lib/chart/export.ts` (`--c-surface` instead of `--bg-base`)
- Create: `src/lib/__tests__/themeTokens.test.ts` (static assert themes.css contains required vars)

**Interfaces:**
- Produces: Tailwind classes `text-success`, `bg-danger`, etc. mapping to `var(--c-success)` / `var(--c-warning)` / `var(--c-danger)`
- Produces: chart chrome reads `--c-edge` / `--c-fg-secondary` / `--c-surface-alt` / `--c-fg`

- [ ] **Step 1: Write token presence test**

```ts
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

describe('themes.css tokens', () => {
  const css = readFileSync(resolve(__dirname, '../../styles/themes.css'), 'utf8');
  for (const token of [
    '--c-surface',
    '--c-success',
    '--c-warning',
    '--c-danger',
    '--font-sans',
    '--font-mono',
    '--font-editor',
  ]) {
    it(`defines ${token}`, () => {
      expect(css).toContain(`${token}:`);
    });
  }
});
```

- [ ] **Step 2: Run — FAIL until Task 2 fonts landed; then extend Tailwind + fix chart vars**

```bash
pnpm exec vitest run src/lib/__tests__/themeTokens.test.ts
rg -n '--border-edge|--text-secondary|--bg-surface|--bg-base|--text-primary' src/components/chart src/lib/chart
```

Replace orphans with `--c-*` equivalents.

- [ ] **Step 3: Tailwind status colors**

```ts
success: { DEFAULT: 'var(--c-success)' },
warning: { DEFAULT: 'var(--c-warning)' },
danger: { DEFAULT: 'var(--c-danger)' },
```

- [ ] **Step 4: Verify + commit**

```bash
pnpm exec vitest run src/lib/__tests__/themeTokens.test.ts
git add tailwind.config.ts src/lib/__tests__/themeTokens.test.ts src/components/chart src/lib/chart
git commit -m "$(cat <<'EOF'
feat(theme): wire status tokens and fix chart CSS variable names

EOF
)"
```

---

### Task 4: CodeMirror / SqlCodeBlock CSS-variable colors

**Files:**
- Create: `src/lib/themeEditorColors.ts`
- Create: `src/lib/__tests__/themeEditorColors.test.ts`
- Modify: `src/components/SqlEditor.tsx`
- Modify: `src/components/SqlCodeBlock.tsx`
- Modify: `src/styles/themes.css` (add `--cm-keyword`, `--cm-string`, … defaults for light/dark)

**Interfaces:**
- Produces:
  - CSS vars: `--cm-keyword`, `--cm-string`, `--cm-number`, `--cm-comment`, `--cm-operator`, `--cm-punctuation`, `--cm-foreground`, `--cm-background`, `--cm-selection`, `--cm-cursor`
  - `export interface EditorColorContract { keyword: string; string: string; /* … */ background: string; foreground: string }`
  - `export function readEditorColors(getVar: (name: string) => string): EditorColorContract`
  - `export function editorColorsFromJson(json: unknown, base: EditorColorContract): EditorColorContract` (merge optional pack `editor.json`)

- [ ] **Step 1: Failing tests**

```ts
import { describe, expect, it } from 'vitest';
import { editorColorsFromJson, readEditorColors } from '../themeEditorColors';

describe('readEditorColors', () => {
  it('reads cm vars with fallbacks', () => {
    const colors = readEditorColors((name) =>
      name === '--cm-keyword' ? ' #c678dd ' : '',
    );
    expect(colors.keyword).toBe('#c678dd');
    expect(colors.string.length).toBeGreaterThan(0); // fallback hex
  });
});

describe('editorColorsFromJson', () => {
  it('overlays pack editor.json keys', () => {
    const base = readEditorColors(() => '');
    const next = editorColorsFromJson({ keyword: '#ff00ff' }, base);
    expect(next.keyword).toBe('#ff00ff');
    expect(next.string).toBe(base.string);
  });
});
```

- [ ] **Step 2: Run — FAIL**

```bash
pnpm exec vitest run src/lib/__tests__/themeEditorColors.test.ts
```

- [ ] **Step 3: Implement + wire editors**

Add light/dark defaults in `themes.css` matching current hardcoded One-Dark-ish / light palettes.

Replace `darkHighlight` / `lightHighlight` construction to call `readEditorColors` from `getComputedStyle` inside `themeExtensions()` / equivalent. Keep MutationObserver reconfigure.

Do **not** load pack `editor.json` in this task (Task 9/10); only the CSS-var path.

- [ ] **Step 4: Verify + commit**

```bash
pnpm exec vitest run src/lib/__tests__/themeEditorColors.test.ts
git add src/lib/themeEditorColors.ts src/lib/__tests__/themeEditorColors.test.ts \
  src/styles/themes.css src/components/SqlEditor.tsx src/components/SqlCodeBlock.tsx
git commit -m "$(cat <<'EOF'
feat(theme): drive CodeMirror colors from CSS variables

EOF
)"
```

---

### Task 5: IconResolver core

**Files:**
- Create: `src/lib/iconIds.ts`
- Create: `src/lib/iconResolver.ts`
- Create: `src/lib/__tests__/iconResolver.test.ts`
- Create: `src/components/ThemedIcon.tsx`

**Interfaces:**
- Produces:
  - `export type IconKind = 'lucide' | 'url' | 'placeholder'`
  - `export type ResolvedIcon = { kind: 'lucide'; name: string } | { kind: 'url'; href: string } | { kind: 'placeholder'; label: string; bgClass: string }`
  - `export type IconSourceMap = Record<string, string>` // semanticId → blob:/asset url
  - `export function createIconResolver(opts: { packIcons: IconSourceMap; driverIcons: IconSourceMap; lucideById: Record<string, string>; placeholderForDb: (dbType: string) => { label: string; bgClass: string } })`
  - `resolver.resolve(semanticId: string): ResolvedIcon`
  - Resolution order for `db.*`: pack → driver → placeholder; for UI ids: pack → lucide → placeholder `{ label: '?' }`

v1 `iconIds.ts` catalog (minimum):

```ts
export const UI_ICON_IDS = [
  'nav.settings',
  'nav.connections',
  'query.run',
  'query.stop',
  'ai.chat',
  'action.backup',
  'action.sync',
  'action.refresh',
  'action.newConnection',
  'action.workflow',
  'theme.light',
  'theme.dark',
  'theme.system',
] as const;
```

- [ ] **Step 1: Failing tests**

```ts
import { describe, expect, it } from 'vitest';
import { createIconResolver } from '../iconResolver';

describe('createIconResolver', () => {
  const resolver = createIconResolver({
    packIcons: { 'nav.settings': 'blob:pack-settings', 'db.postgresql': 'blob:pack-pg' },
    driverIcons: { 'db.postgresql': 'asset:driver-pg', 'db.mysql': 'asset:driver-mysql' },
    lucideById: { 'nav.settings': 'Settings', 'query.run': 'Play' },
    placeholderForDb: (t) => ({ label: t.slice(0, 2), bgClass: 'bg-slate-600' }),
  });

  it('prefers pack over lucide for UI icons', () => {
    expect(resolver.resolve('nav.settings')).toEqual({ kind: 'url', href: 'blob:pack-settings' });
  });

  it('falls back to lucide', () => {
    expect(resolver.resolve('query.run')).toEqual({ kind: 'lucide', name: 'Play' });
  });

  it('resolves db pack → driver → placeholder', () => {
    expect(resolver.resolve('db.postgresql')).toEqual({ kind: 'url', href: 'blob:pack-pg' });
    expect(resolver.resolve('db.mysql')).toEqual({ kind: 'url', href: 'asset:driver-mysql' });
    expect(resolver.resolve('db.unknown')).toEqual({
      kind: 'placeholder',
      label: 'un',
      bgClass: 'bg-slate-600',
    });
  });
});
```

- [ ] **Step 2: Run — FAIL**

```bash
pnpm exec vitest run src/lib/__tests__/iconResolver.test.ts
```

- [ ] **Step 3: Implement resolver + `ThemedIcon`**

`ThemedIcon` props: `{ id: string; className?: string; resolver?: IconResolver }`. Default resolver from a small module-level registry updated by ThemeService later (`setActiveIconResolver` / `getActiveIconResolver`).

For `kind: 'lucide'`, map name → lucide-react component via a local `LUCIDE_MAP` limited to v1 catalog (not dynamic string index of entire lucide).

- [ ] **Step 4: Verify + commit**

```bash
pnpm exec vitest run src/lib/__tests__/iconResolver.test.ts
git add src/lib/iconIds.ts src/lib/iconResolver.ts src/lib/__tests__/iconResolver.test.ts \
  src/components/ThemedIcon.tsx
git commit -m "$(cat <<'EOF'
feat(theme): add IconResolver and ThemedIcon for semantic IDs

EOF
)"
```

---

### Task 6: DB badge + driver default icon hook

**Files:**
- Create: `src/components/DbTypeBadge.tsx`
- Create: `src/components/__tests__/DbTypeBadge.test.tsx` (optional light RTL; or pure resolve test only)
- Modify: `src/lib/databaseMeta.ts` — optional `defaultIcon?: string` (URL or import path string for built-ins)
- Modify: `src/lib/databaseTypes.ts` — helper `getDriverIconMap(): IconSourceMap`
- Create: `src/assets/db-icons/` with SVG placeholders for built-in types (`postgresql.svg`, `mysql.svg`, `mariadb.svg`, `sqlite.svg`, `redis.svg`) — simple geometric marks OK
- Modify: `src/windows/main/ConnectionItem.tsx` to use `DbTypeBadge`
- Modify: backup compact badge site similarly if trivial

**Interfaces:**
- Produces:
  - `export function DbTypeBadge(props: { databaseType: string; className?: string; size?: number })`
  - `getDriverIconMap()` returns `{ 'db.postgresql': <url>, ... }` from Vite `?url` imports
  - Placeholder still uses `shortLabel` + `iconBg` when no URL

- [ ] **Step 1: Unit test driver map keys**

```ts
import { describe, expect, it } from 'vitest';
import { getDriverIconMap } from '../databaseTypes';

describe('getDriverIconMap', () => {
  it('exposes db.* keys for built-in SQL/KV types', () => {
    const map = getDriverIconMap();
    expect(map['db.postgresql']).toMatch(/postgresql/i);
    expect(map['db.mysql']).toBeTruthy();
    expect(map['db.redis']).toBeTruthy();
  });
});
```

- [ ] **Step 2: Implement assets + badge + wire ConnectionItem**

Keep visual size parity with current 20–24px squares. If URL present, render `<img>`; else text badge.

Seed default `createIconResolver` in app bootstrap (`main.tsx` or settings load) with `driverIcons: getDriverIconMap()` and empty pack map.

- [ ] **Step 3: Verify + commit**

```bash
pnpm exec vitest run src/lib/__tests__/databaseTypes.test.ts src/lib/__tests__/iconResolver.test.ts
# if new test file:
pnpm exec vitest run src/lib/__tests__/driverIconMap.test.ts
git add src/components/DbTypeBadge.tsx src/assets/db-icons src/lib/databaseMeta.ts \
  src/lib/databaseTypes.ts src/windows/main/ConnectionItem.tsx src/lib/__tests__/driverIconMap.test.ts
git commit -m "$(cat <<'EOF'
feat(theme): add DB type badge with driver default SVG hooks

EOF
)"
```

---

### Task 7: Migrate curated UI surfaces to ThemedIcon

**Files:**
- Modify: `src/components/ThemeToggle.tsx` → `theme.light|dark|system`
- Modify: `src/windows/main/ActionPanel.tsx` → action.* ids
- Modify: `src/windows/settings/SettingsWindow.tsx` section icons → `nav.settings` etc. where applicable
- Create: `src/lib/hostLucideMap.ts` mapping semantic id → lucide export name used by default resolver

**Interfaces:**
- Consumes: `ThemedIcon`, `UI_ICON_IDS`, default resolver
- Produces: no new public API

- [ ] **Step 1: Replace imports in the three surfaces**

Example ActionPanel:

```tsx
<ThemedIcon id="action.backup" className="h-4 w-4" />
```

Ensure hover/disabled styles still apply via `className` on wrapper.

- [ ] **Step 2: Manual smoke (dev)**

```bash
pnpm tauri:dev
```

Expected: main window actions + theme toggle + settings sections still show icons (Lucide fallback).

- [ ] **Step 3: Commit**

```bash
git add src/components/ThemeToggle.tsx src/windows/main/ActionPanel.tsx \
  src/windows/settings/SettingsWindow.tsx src/lib/hostLucideMap.ts
git commit -m "$(cat <<'EOF'
feat(theme): route curated UI chrome through ThemedIcon

EOF
)"
```

---

### Task 8: Rust theme pack validation + install helpers

**Files:**
- Create: `src-tauri/src/theme/mod.rs`
- Create: `src-tauri/src/theme/validate.rs`
- Create: `src-tauri/src/theme/install.rs`
- Modify: `src-tauri/src/lib.rs` — `mod theme;`
- Fixture: `fixtures/themes/community.fixture-dark/` (manifest + tokens.css + one svg + optional font omitted)

**Interfaces:**
- Produces:
  - `pub struct ThemeManifest { id, name, version, api_version, modes, author, description }`
  - `pub const THEME_API_VERSION: u32 = 1`
  - `pub fn allowed_theme_extension(ext: &str) -> bool` — css/svg/png/webp/json/woff2/woff only
  - `pub fn validate_theme_zip_path(name: &str) -> Result<(), String>` — reuse rules akin to `validate_zip_entry_path`; reject absolute, `..`, symlinks names
  - `pub fn validate_pack_dir(dir: &Path) -> Result<ThemeManifest, String>` — require manifest+tokens.css; reject forbidden files if present on disk
  - `pub fn install_theme_zip(zip_path: &Path, themes_root: &Path) -> Result<ThemeManifest, String>` — extract to staging, validate, atomic rename to `themes_root/{id}/`
  - Limits: e.g. `MAX_THEME_UNCOMPRESSED = 16 * 1024 * 1024`, `MAX_THEME_FILES = 500`, `MAX_THEME_FONT_BYTES = 4 * 1024 * 1024`

- [ ] **Step 1: Write Rust unit tests (in `validate.rs` `mod tests`)**

```rust
#[test]
fn rejects_js_extension() {
    assert!(!allowed_theme_extension("js"));
    assert!(!allowed_theme_extension("wasm"));
    assert!(!allowed_theme_extension("ico"));
    assert!(allowed_theme_extension("svg"));
    assert!(allowed_theme_extension("woff2"));
}

#[test]
fn rejects_path_traversal_entry() {
    assert!(validate_theme_zip_path("../evil.css").is_err());
    assert!(validate_theme_zip_path("icons/../../x.css").is_err());
    assert!(validate_theme_zip_path("tokens.css").is_ok());
}

#[test]
fn validate_pack_dir_requires_manifest_and_tokens() {
    let dir = tempfile::tempdir().unwrap();
    assert!(validate_pack_dir(dir.path()).is_err());
    // write minimal manifest + tokens, assert Ok and id match folder policy
}
```

If `tempfile` is not already a dev-dependency, use `std::env::temp_dir` + random folder instead (prefer no new deps).

- [ ] **Step 2: Run — FAIL**

```bash
cargo test -p datazen --lib theme:: -q
```

- [ ] **Step 3: Implement validate + install**

Manifest id must match `^[a-z0-9]+([.-][a-z0-9]+)*$` and install directory name == `manifest.id`.

Strip / ignore any field that looks like app icon paths; if `manifest` contains keys `appIcon` / `trayIcon` / `bundleIcon`, return validation error.

Scan extracted files: every file extension must pass whitelist; reject `.js` even inside nested dirs.

For each `.svg`, read UTF-8 (cap 256 KiB) and reject if it contains `<script` (case-insensitive), `javascript:`, or `onload=` / `onerror=` attribute patterns (v1 deny-list; full SVG sanitizer not required).

Reuse zip bomb protection approach from `app_data_archive` (`MAX_COMPRESSION_RATIO`, limited reader) — either call shared helpers or copy the minimal subset into `theme/install.rs` with theme-specific caps.

- [ ] **Step 4: Add fixture pack directory**

```
fixtures/themes/community.fixture-dark/
  manifest.json
  tokens.css
  icons/nav.settings.svg
  icons/db.postgresql.svg
```

`tokens.css` overrides `--c-accent` and `--font-mono` for easy assertion later.

- [ ] **Step 5: Tests pass + commit**

```bash
cargo test -p datazen --lib theme:: -q
git add src-tauri/src/theme fixtures/themes/community.fixture-dark src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(theme): add ThemePack zip validation and install helpers

EOF
)"
```

---

### Task 9: Theme IPC commands + frontend wrappers

**Files:**
- Create: `src-tauri/src/commands/theme.rs`
- Modify: `src-tauri/src/commands/mod.rs` — `mod theme; pub use theme::*`
- Modify: `src-tauri/src/lib.rs` — register handlers
- Create: `src/commands/theme.ts`
- Create: `src/types/themePack.ts` (`InstalledThemePack` summary type)

**Interfaces:**
- Produces IPC:
  - `list_theme_packs() -> Vec<ThemePackSummary>`
  - `install_theme_pack_with_dialog() -> ThemePackSummary` (file dialog, zip filter)
  - `remove_theme_pack(id: String) -> ()` (refuse if currently enabled — or auto-disable first; **choose auto-disable**)
  - `read_theme_pack_file(id: String, relativePath: String) -> Vec<u8>` (path must normalize under pack root; used for CSS/icons/fonts)
  - `themes_root_path()` not exposed to UI (keep server-side)
- Frontend:

```ts
export const themeCommands = {
  listThemePacks: () => invoke<ThemePackSummary[]>('list_theme_packs'),
  installThemePackWithDialog: () => invoke<ThemePackSummary>('install_theme_pack_with_dialog'),
  removeThemePack: (id: string) => invoke<void>('remove_theme_pack', { id }),
  readThemePackFile: (id: string, relativePath: string) =>
    invoke<number[]>('read_theme_pack_file', { id, relativePath }),
};
```

`ThemePackSummary`: `{ id, name, version, apiVersion, modes, author, description }`

- [ ] **Step 1: Rust command smoke tests for path join safety**

```rust
#[test]
fn read_path_rejects_escape() {
    assert!(safe_pack_rel_path("tokens.css").is_ok());
    assert!(safe_pack_rel_path("../settings.json").is_err());
    assert!(safe_pack_rel_path("icons/../../x").is_err());
}
```

- [ ] **Step 2: Implement commands**

Themes root: `state.store.data_dir().join("themes")`.

`install_theme_pack_with_dialog`: use Tauri dialog plugin (mirror `file.rs` / `config.rs` patterns), then `install_theme_zip`.

`remove_theme_pack`: if `settings.theme.pack_id == Some(id)`, clear pack_id and save settings, then `fs::remove_dir_all`.

- [ ] **Step 3: Frontend wrappers + commit**

```bash
cargo test -p datazen --lib theme:: -q
git add src-tauri/src/commands/theme.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs \
  src/commands/theme.ts src/types/themePack.ts
git commit -m "$(cat <<'EOF'
feat(theme): expose ThemePack list/install/remove/read IPC

EOF
)"
```

---

### Task 10: ThemeService apply (mode × pack)

**Files:**
- Create: `src/lib/themePackApply.ts`
- Create: `src/lib/__tests__/themePackApply.test.ts` (jsdom style tag)
- Modify: `src/stores/settingsStore.ts` — on load/update call apply
- Modify: `src/hooks/useThemeListener.ts` — re-apply on settings change
- Modify: `src/lib/themeEditorColors.ts` usage when `editor.json` present
- Modify: `src/lib/chart/colors.ts` — `setChartPaletteOverride` / clear

**Interfaces:**
- Produces:
  - `export async function applyThemePack(packId: string | null): Promise<void>`
  - `export function clearThemePack(): void` — removes `#datazen-theme-pack` style, clears icon pack map, chart override, editor.json overlay
  - When `packId` set:
    1. `readThemePackFile(id, 'tokens.css')` → text
    2. optional `fonts.css`
    3. Rewrite `@font-face` `url(...)` to `blob:` URLs from pack font bytes (reject if url is `http:`)
    4. Inject/replace `<style id="datazen-theme-pack">`
    5. Scan `icons/` via summary or fixed list from a new IPC `list_theme_pack_icons(id)` **or** derive from known semantic ids by probing `read` for `.svg|.webp|.png` in priority order (probe is OK for v1 curated ids + `db.*` for known registry keys)
    6. `setActiveIconResolver` with merged pack icons
    7. optional `editor.json` / `charts.json` parse → overlays
  - `applyTheme` sets html class from mode AND calls `applyThemePack(packId)`
  - Webview background: continue setting `documentElement.style.backgroundColor` from computed `--c-surface` after pack inject (not hardcoded hex only)

- [ ] **Step 1: jsdom test for style inject**

```ts
import { describe, expect, it, beforeEach } from 'vitest';
import { injectThemePackCss, clearThemePackDom } from '../themePackApply';

describe('injectThemePackCss', () => {
  beforeEach(() => {
    clearThemePackDom();
  });

  it('upserts style#datazen-theme-pack', () => {
    injectThemePackCss(':root { --c-accent: #ff00ff; }');
    const el = document.getElementById('datazen-theme-pack');
    expect(el?.tagName).toBe('STYLE');
    expect(el?.textContent).toContain('--c-accent: #ff00ff');
    injectThemePackCss(':root { --c-accent: #00ff00; }');
    expect(document.querySelectorAll('#datazen-theme-pack')).toHaveLength(1);
    expect(el?.textContent).toContain('#00ff00');
  });
});
```

- [ ] **Step 2: Implement apply pipeline + wire settings store**

Also update `index.html` boot script? Keep mode-only boot; pack applies after settings load (accept brief default flash for v1).

Emit `datazen:theme-pack-changed` cross-window after apply so other windows re-fetch packId from settings event (settings-changed already carries full settings — re-apply there).

- [ ] **Step 3: Verify + commit**

```bash
pnpm exec vitest run src/lib/__tests__/themePackApply.test.ts
git add src/lib/themePackApply.ts src/lib/__tests__/themePackApply.test.ts \
  src/stores/settingsStore.ts src/hooks/useThemeListener.ts src/lib/chart/colors.ts
git commit -m "$(cat <<'EOF'
feat(theme): apply installed ThemePack CSS, icons, and overlays

EOF
)"
```

---

### Task 11: Settings UI — pack picker + import

**Files:**
- Modify: `src/windows/settings/SettingsWindow.tsx` (Appearance subsection)
- Modify: `src/locales/en.ts`, `src/locales/zh-CN.ts` (and zh-TW if keys required by CI) — new i18n keys
- Optional: `src/windows/settings/ThemePackSection.tsx` extract if file grows

**Interfaces:**
- UI:
  - Mode select (existing) bound to `theme.mode`
  - Pack `<Select>`: `Default (Built-in)` + list from `listThemePacks()`
  - Buttons: `Import…` → `installThemePackWithDialog` then refresh list + optionally enable
  - `Remove` for selected non-null pack
- On pack change: `updateSettings({ theme: { ...theme, packId } })`

- [ ] **Step 1: Add i18n keys**

```ts
'settings.theme.pack': 'Theme pack',
'settings.theme.packDefault': 'Built-in default',
'settings.theme.import': 'Import pack…',
'settings.theme.remove': 'Remove pack',
```

Mirror in zh-CN.

- [ ] **Step 2: Implement Theme pack controls in General / new Appearance group**

Load packs on section mount. Handle errors with existing toast/error patterns in SettingsWindow.

- [ ] **Step 3: Manual verify**

```bash
pnpm tauri:dev
```

Import `fixtures/themes/community.fixture-dark` zipped manually (or add a zip in fixtures). Enable → accent/icon change; Disable → restore; Remove → files gone.

- [ ] **Step 4: Commit**

```bash
git add src/windows/settings/SettingsWindow.tsx src/locales/en.ts src/locales/zh-CN.ts src/locales/zh-TW.ts
git commit -m "$(cat <<'EOF'
feat(theme): add Settings UI to import and enable local theme packs

EOF
)"
```

---

### Task 12: End-to-end fixture test + docs touch-up

**Files:**
- Create: `scripts/pack-theme-fixture.mjs` (zips `fixtures/themes/community.fixture-dark` → `fixtures/themes/community.fixture-dark.zip`)
- Create: `src-tauri/src/theme/install_tests.rs` or extend install tests to use the zip fixture
- Create: `e2e/specs/theme-pack-local.ts` **or** defer E2E if environment heavy — minimum for this task is Rust install+validate against zip + Vitest apply; add E2E only if `pnpm e2e:skip-build` already workable
- Modify: `docs/superpowers/specs/2026-08-08-runtime-theme-packs-design.md` — add “Implementation plan” link
- Modify: `AGENTS.md` only if a durable new module convention is needed (skip if not)

**Interfaces:**
- Produces: green unit/integration proof of success criteria (local install, enable tokens, icon override, disable restore)

- [ ] **Step 1: Script + Rust integration test**

```bash
node scripts/pack-theme-fixture.mjs
cargo test -p datazen --lib theme:: -q
```

Test installs zip into temp `themes_root`, asserts `tokens.css` exists, `validate_pack_dir` Ok.

- [ ] **Step 2: Vitest icon priority with pack map simulating fixture ids**

Ensure `nav.settings` + `db.postgresql` pack URLs win.

- [ ] **Step 3: Optional E2E**

If added: install via dialog helper or IPC test harness; assert `document.getElementById('datazen-theme-pack')` and one DB badge `img` src.

- [ ] **Step 4: Final verification suite**

```bash
pnpm exec vitest run src/types/__tests__/theme.test.ts \
  src/lib/__tests__/resolveEditorFontFamily.test.ts \
  src/lib/__tests__/themeTokens.test.ts \
  src/lib/__tests__/themeEditorColors.test.ts \
  src/lib/__tests__/iconResolver.test.ts \
  src/lib/__tests__/themePackApply.test.ts
cargo test -p datazen --lib theme:: -q
cargo test -p datazen --lib store:: -q
```

- [ ] **Step 5: Commit**

```bash
git add scripts/pack-theme-fixture.mjs fixtures/themes docs/superpowers/specs/2026-08-08-runtime-theme-packs-design.md e2e/specs/theme-pack-local.ts
git commit -m "$(cat <<'EOF'
test(theme): add fixture pack coverage for local ThemePack install

EOF
)"
```

---

## Self-review (spec coverage)

| Spec requirement | Task |
|------------------|------|
| Runtime install to `{appData}/themes` | 8, 9 |
| Independent of driver plugins | 6, 8, 10 (separate paths/APIs) |
| Declarative pack (no JS) | 8 validation |
| UI semantic icons SVG/PNG/WebP | 5, 7, 10 |
| DB badges theme → driver → placeholder | 5, 6, 10 |
| Fonts `--font-*` + user priority | 2, 10 |
| No app icon modification | 8 reject fields / formats |
| Host token / CM / charts pre-hardening | 3, 4 |
| `applyTheme(mode × pack)` | 1, 10 |
| Settings `mode` + `packId` + migration | 1, 11 |
| Security whitelist / zip / SVG policy | 8 (SVG script strip: implement basic reject-if-contains `<script` / `javascript:` in validate) |
| Unit tests listed in spec | 5, 8, 10, 12 |
| Store CDN | **Deferred** — not in this plan |
| E2E from fixture zip | 12 (optional if harness heavy) |

**Placeholder scan:** none intentional; store deferred explicitly.

**Type consistency:** `ThemePreference.packId` (TS) ↔ `pack_id` / serde `packId` (Rust); style element id always `datazen-theme-pack`; IPC names `list_theme_packs`, `install_theme_pack_with_dialog`, `remove_theme_pack`, `read_theme_pack_file`.

---

### Task 13: Update system documentation

**Files:**
- Modify: `AGENTS.md` — theme packs module, `{appData}/themes`, IPC surface, independence from driver plugins
- Modify: `README.md` — brief user-facing note on local theme packs / Appearance settings (if README has Features section)
- Modify: `docs/architecture/README.md` and any linked frontend/backend architecture docs that describe theming/settings/plugins
- Modify: `docs/frontend-architecture.md` and/or `docs/backend-architecture.md` if present — ThemeService, settings shape, IconResolver
- Modify: `docs/superpowers/specs/2026-08-08-runtime-theme-packs-design.md` — status → Implemented (link plan + note store deferred)

**Interfaces:**
- Consumes: final IPC names, settings shape, install path from Tasks 1–12
- Produces: docs that match shipped behavior (no aspirational store features)

- [ ] **Step 1: Inventory docs that mention theme / plugins / settings**

```bash
rg -n 'theme|ThemePack|plugins-registry|AppSettings|外观' AGENTS.md README.md docs/architecture docs/frontend-architecture.md docs/backend-architecture.md docs/plugin-development.md 2>/dev/null | head -80
```

- [ ] **Step 2: Update AGENTS.md directory map + conventions**

Document:
- `{appData}/themes/{id}/` runtime packs (separate from `.plugins` / `plugins-registry.json`)
- Settings `theme: { mode, packId }`
- IPC: `list_theme_packs`, `install_theme_pack_with_dialog`, `remove_theme_pack`, `read_theme_pack_file`
- Frontend: `src/lib/themePackApply.ts`, `iconResolver.ts`, `ThemedIcon`, `DbTypeBadge`

- [ ] **Step 3: Update architecture / README sections**

Keep store/CDN as future work. Match code paths exactly.

- [ ] **Step 4: Commit**

```bash
git add AGENTS.md README.md docs/
git commit -m "$(cat <<'EOF'
docs: document runtime theme packs in AGENTS and architecture

EOF
)"
```

---

## Execution handoff

Plan saved to `docs/superpowers/plans/2026-08-09-runtime-theme-packs.md`.

**Chosen:** Subagent-Driven Development + Task 13 docs after feature complete.
