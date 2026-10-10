# i18n Translation Sync Skill

Sync non-English locale files with the English dictionary by translating missing or changed keys.

## When to Use

- Before a release (git tag) to fill in translations for all locales
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
   - **Driver pack** (`packages/drivers/<id>/locales/`) → `<locale>.ts`; driver
     packs are genuinely flat monoliths, so `en.ts` there is correct.
   - Translate only the changed English values; preserve the existing key order
     and file structure.

4. **Run the locale test** to verify all keys are in sync:

```bash
pnpm exec vitest run src/locales/locales.test.ts
```

## Translation Guidelines

| Locale | Language | Notes |
|--------|----------|-------|
| zh-CN  | Simplified Chinese | Primary Chinese locale |
| zh-TW  | Traditional Chinese | Use traditional characters (報表 not 报表) |
| de     | German | Formal register |
| es     | Spanish | Latin American Spanish |
| fr     | French | Standard French |
| ja     | Japanese | Use katakana for loan words |
| ko     | Korean | Standard Korean |
| pt-BR  | Brazilian Portuguese | |
| ru     | Russian | |

- Keep interpolation placeholders like `{count}`, `{name}` unchanged
- Keep technical terms (SQL, JSON, YAML, etc.) untranslated
- Match the tone and style of existing translations in each locale
- Do NOT modify the English keys — they are the source of truth

## Important Rules

- **During development**: Only add keys to the host's English **domain packs**
  (`src/locales/en/<domain>.ts`; `src/locales/en.ts` is a shim). Driver packs use
  `packages/drivers/<id>/locales/en.ts`, which is a real file. All other locales
  are synced before release.
- **Before release**: Run this skill to translate all missing/stale keys.
- The `scripts/i18n-sync-check.mjs` script returns exit code 1 if there are
  outstanding translations, making it suitable for CI checks.
