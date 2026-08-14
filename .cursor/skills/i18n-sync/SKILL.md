# i18n Translation Sync Skill

Sync non-English locale files with `en.ts` by translating missing or changed keys.

## When to Use

- Before a release (git tag) to fill in translations for all locales
- When `node scripts/i18n-sync-check.mjs` reports missing or stale keys
- When the user asks to "sync translations", "补齐翻译", or "check i18n"

## Workflow

1. **Run the check script** to identify what needs translation:

```bash
node scripts/i18n-sync-check.mjs --verbose
```

2. **Read en.ts** to get the English values for changed/added keys.

3. **For each locale file** that has missing or stale keys:
   - Read the locale file
   - Translate only the changed English values into the target language
   - Use StrReplace to update the locale file with correct translations
   - Preserve the existing key order and file structure

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
- Do NOT modify `en.ts` — it is the source of truth

## Important Rules

- **During development**: Only modify `en.ts` (and optionally `zh-CN.ts`).
  Other locales are synced before release.
- **Before release**: Run this skill to translate all missing/stale keys.
- The `scripts/i18n-sync-check.mjs` script returns exit code 1 if there are
  outstanding translations, making it suitable for CI checks.
