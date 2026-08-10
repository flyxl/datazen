# Task 8 Report — TableStructureEditor refactor

**Status:** Complete

## Summary

Refactored `TableStructureEditor` to consume driver UI config (`DB_REGISTRY.structureEditor`), runtime caps (`getStructureCapabilities`), and DDL planning (`planTableStructureChanges`). Removed all Host-side PG SQL generation (`PG_TYPES`, `generateCreateSQL`, `generateAlterSQL`).

## Changes

| Area | Detail |
|------|--------|
| `TableStructureEditor.tsx` | Parallel load schema + caps on open; draft columns + indexes; preview via plan IPC; execute statements one-by-one with stop-on-failure + partial count message |
| `structure/` subcomponents | Wired `StructureColumnTable`, `StructureIndexTable`, `StructurePlanPreview` |
| `SqlConnectionView.tsx` | Passes `databaseType` into editor |
| `draftDefaults.ts` | Pure helpers for empty/default draft rows |
| `controlHints.ts` | Single i18n key `structEditor.capDisabled` for disabled controls |
| Locales (`en`, `zh-CN`) | Added cap/disabled, indexes section, partial execute, not-supported keys |

## Behavior checklist

- [x] Open: schema + caps + meta in parallel (alter) / caps + defaults (create)
- [x] Draft includes columns and indexes on same screen
- [x] Controls disabled via `capEnabled` + tooltip reason
- [x] `reorderColumn === false` disables drag reorder
- [x] Preview → `planTableStructureChanges` with sql/summary/risk
- [x] Execute one statement at a time; stop on failure; show executed count on partial failure
- [x] No Host PG SQL hardcoding
- [x] `buildStructureChangeRequest` helper + vitest (pre-existing)
- [x] Missing/disabled `structureEditor` → clear message, no crash
- [x] E2E selectors preserved (`new_table`, `column_name`, preview button text)

## Tests

```bash
npx vitest run src/lib/structureEditor/   # 5 passed
```

## Concerns / follow-ups

- Task 9 (IndexesView opt-out / entry guards) not in scope; create-table entry in sidebar still shown for all SQL types — guard belongs to Task 9.
- Create-mode default column template uses generic `id` + PK checkbox; dialect-specific serial types come from driver plan, not Host defaults.
- No dedicated vitest for `draftDefaults.ts` (trivial); mapping covered by `buildStructureChangeRequest.test.ts`.

## Commit

```
refactor(ui): TableStructureEditor consumes driver caps and plan IPC
```
