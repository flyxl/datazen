# i18n Translation Sync Skill

Sync non-English locale files with the English dictionary by translating missing or changed keys.

## When to Use

- Before a release (git tag) to fill in translations for the locales shipped by the app
- When `node scripts/i18n-sync-check.mjs` reports missing or stale keys
- When the user asks to "sync translations", "补齐翻译", or "check i18n"

## Workflow

1. **Run the check script** to identify what needs translation:

```bash
node scripts/i18n-sync-check.mjs --verbose
```

2. **Read the English values** for the changed/added keys. Where they live depends
   on the scope — never assume a flat `en.ts` (see step 3).

3. **For each locale** that has missing or stale keys, write into the **same file
   layout the English key already lives in**:
   - **Host** (`src/locales/`) → `src/locales/<locale>/<domain>.ts`, using the
     domain of the English key. ⚠️ `src/locales/<locale>.ts` is a 2-line re-export
     shim (`export { default } from './<locale>/index'`) — writing there has **no
     effect**. All 10 host locales are domain-split.
   - **Driver pack** (`crates/drivers/<id>/locales/`) → `<locale>.ts`; driver
     packs are genuinely flat monoliths, so `en.ts` there is correct.
   - Translate only the changed English values; preserve the existing key order
     and file structure.

4. **Run the locale test** to verify built-in host translations:

```bash
pnpm exec vitest run src/locales/locales.test.ts
```

## Translation Guidelines

| Locale | Language | Notes |
|--------|----------|-------|
| en     | English | Source language |
| zh-CN  | Simplified Chinese | Only non-English locale currently shipped |
| de, es, fr, ja, ko, pt-BR, ru, zh-TW | Source-only translations | Kept in source; not checked or bundled for release |

- Keep interpolation placeholders like `{count}`, `{name}` unchanged
- Keep technical terms (SQL, JSON, YAML, etc.) untranslated
- Match the tone and style of existing translations in each locale
- Do NOT modify the English keys — they are the source of truth

## Important Rules

- **During development**: Only add keys to the host's English **domain packs**
  (`src/locales/en/<domain>.ts`; `src/locales/en.ts` is a shim). Driver packs use
  `crates/drivers/<id>/locales/en.ts`, which is a real file.
- **Before release**: Run this skill to translate missing/stale keys for `zh-CN`;
  source-only locale dictionaries do not block or enter the release bundle.
- The `scripts/i18n-sync-check.mjs` script returns exit code 1 if there are
  outstanding translations, making it suitable for CI checks.
