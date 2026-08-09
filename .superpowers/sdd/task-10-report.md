# Task 10 Report — i18n, docs, guardrails, e2e

## Done

1. **i18n** — Synced 11 missing keys (`structEditor.previewing` … `executePartial`, `indexes.editInStructure`) across de/es/fr/ja/ko/pt-BR/ru/zh-TW (en/zh-CN already complete).
2. **Docs** — In-app 使用说明「功能特色」新增表结构编辑器段落（ZH/EN）；`docs/competitive-comparison-dbx.md` 一行更新为驱动自报 caps 架构，并注明 P1 缺口（MySQL charset/unsigned、索引 INCLUDE UI）。
3. **Guardrail** — `scripts/check-structure-editor-guardrails.mjs` + CI step；`rg` on `src`/`src-tauri` 无 Host caps 表命中。
4. **e2e** — `table-structure.ts` 增加 SQL 预览标题断言、创建表/保存更改按钮可见性（TS-004b、TS-006b）。

## Verification

| Command | Result |
|---------|--------|
| `node scripts/check-structure-editor-guardrails.mjs` | ok |
| `npx vitest run src/lib/structureEditor src/locales/locales.test.ts` | 26 passed |
| `cargo test -p datazen-driver-postgres structure` | 16 passed |
| `cargo test -p datazen-driver-mysql structure` | 16 passed |
| `cargo test -p datazen-driver-sqlite structure` | 11 passed |
| `rg -n "capabilityByType\|structure_capabilities_by" src src-tauri` | no matches |

## Review fixes

1. **Guardrail** — Extended `check-structure-editor-guardrails.mjs` with `structureCapabilitiesBy` and `StructureCapabilities\s*\{[^}]*postgres` (dialect-keyed inline cap maps). The brace pattern intentionally excludes plain `interface StructureCapabilities { createTable: … }` in `types.ts` (no dialect key inside).
2. **e2e TS-006b** — Requires `connWin.editTableStructure` visible via `waitForDisplayed`; no silent skip.

Re-run: `node scripts/check-structure-editor-guardrails.mjs` → ok.

## Deferred (P1)

- MySQL charset / unsigned column UI
- Index INCLUDE column UI (PG 14+)
