# Track: sql-s4-b — Completion & Schema Intelligence

## Status: READY_FOR_TEST

## Phase
- [x] Implementation (Core completion & function registry)
- [x] FK JOIN & Function Signature Help transferred to @datazen/extension-sql-pro
- READY_FOR_TEST

## Deliverables

### Core files
- `src/components/sql-editor/completion/functionRegistry.ts` — Function registry: single source of truth for completions. Dialect-filtered entries for Common/PG/MySQL/SQLite with DATE_ADD, CONCAT, and existing lists.
- `src/components/sql-editor/completion/schemaCompletion.ts` — Alias-aware schema completion: `alias.` resolves to that relation's columns with type/nullable/comment; ambiguous = no guess.

### Pro Extension Transferred
- FK JOIN completion & Signature Help have been moved to private repo `https://github.com/flyxl/datazen-extension-sql-pro`.
- Host uses `sqlEditorProEP` fallback.
