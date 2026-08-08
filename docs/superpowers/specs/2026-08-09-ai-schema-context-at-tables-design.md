# Design: AI Schema Context — List-then-Select + @ Tables

**Date:** 2026-08-09  
**Branch:** `feat/ai-schema-context`  
**Status:** Approved  
**Scope:** AI Chat + NL2SQL (interactive UI). Diagnose / MCP deferred.

## Goals

1. Stop default **full-schema dump** into Chat / NL2SQL prompts; switch to **table list first**, then pull schemas for needed tables.
2. Hybrid selection:
   - User `@` tables → pre-inject those tables’ compact DDL; **still attach DB tools** so the model can fetch related tables.
   - No `@` tables → rely on **tool calling** (`list_tables` / `get_table_schema`) in the same request loop.
3. Extend `@` context picker (Cursor-like): categories on top, recent below, in-place drill-down with back, keyboard filter; selected items are **inline tokens** (not chips), removable with Backspace/Delete.
4. Unify behavior behind a **SchemaContextPipeline** shared by Chat and NL2SQL.
5. Cover with unit tests **and E2E**.

## Non-goals

- Changing SQL error diagnose or MCP prompt schema injection in this revision.
- Rewriting AI provider protocols.
- Complex multi-part table identifiers beyond driver `TableInfo.name` as returned today.
- Requiring real LLM answer assertions in E2E (avoid flaky provider dependency).

## Decisions

| Topic | Choice |
|-------|--------|
| Approach | Unified SchemaContextPipeline (Approach 2) |
| Who selects tables | Hybrid: `@` pins + tools for more; no `@` → tools |
| Surfaces | Chat + NL2SQL only |
| No-tools fallback | Legacy `build_sql_context` truncated full DDL |
| When `@` pins exist | Inject pinned DDL **and** keep DB tools |
| `@` picker UX | Cursor-style: categories / recent / drill-in + back / type-to-filter |
| Selected context UI | Inline tokens (icon + name), not chips; Backspace/Delete removes whole token |
| Table list for picker | Existing `get_tables` IPC |
| Tools | Reuse `db_tool_definitions` / `execute_db_tool` |
| E2E | Required (`CTX-T01`–`CTX-T06`) |

## Current state

- `SchemaContextBuilder::build_sql_context` loads all tables’ compact DDL until a token budget; used by Chat (`include_schema`), NL2SQL, diagnose, MCP.
- `build_selective_context` / `get_table_names` exist but are mainly used by `ai_generate_schema_doc` (two-phase when &gt;30 tables).
- Chat **already** has a streaming tool loop with `list_tables` / `get_table_schema` (and other DB tools), but still pre-injects full truncated schema when `include_schema` is true — redundant and expensive.
- `@` picker (`ContextPicker`) lists **context files only**; chips with × buttons in `AiInput`.
- E2E file coverage for files: `e2e/specs/ai-context.ts` (CTX-001+).

## Architecture

```
Frontend (Chat / Nl2Sql)
  ContextPicker (Categories → drill-in → Recent + filter)
  inline tokens: files + tables
       │
       ▼ IPC
  context_tables?: string[]
  context_files?: string[]     // existing
       │
       ▼
SchemaContextPipeline (new; orchestrates SchemaContextBuilder)
  resolve(connection_id, database, pinned_tables, supports_tools)
    → PromptSeed {
         database_type,
         table_names,
         pinned_schema_ddl,
         attach_db_tools,
         fallback_schema_ddl?,  // only when !supports_tools
      }
       │
       ├─ Chat: system from PromptSeed + existing tool loop
       └─ NL2Sql: same PromptSeed + shared schema tool-loop executor
```

**Boundaries**

- Pipeline owns policy; `SchemaContextBuilder` stays the low-level cache/DDL helper.
- Workflow Chat may share the pipeline; Tables category only when `connectionId` is set.
- Diagnose / MCP remain on legacy injection until a follow-up.

## Data model & IPC

### Frontend

```ts
type ContextKind = 'file' | 'dir' | 'table';

interface ContextItem {
  kind: ContextKind;
  id: string;       // file: relative path; table: table name
  name: string;     // token label
  path?: string;    // files
  database?: string;// optional multi-db hint
}
```

Send path splits:

- `context_files: string[]` (unchanged)
- `context_tables: string[]` (new)

### Backend commands

| Command | Change |
|---------|--------|
| `ai_chat` | Add `context_tables: Option<Vec<String>>` |
| `ai_generate_sql` | Add `context_tables`; use Pipeline + tool loop when `supports_tools` |
| Table listing for UI | Reuse `get_tables` |

### `PromptSeed`

- `database_type: String`
- `table_names: Vec<String>`
- `pinned_schema_ddl: String` — compact DDL for `@` tables (budgeted)
- `attach_db_tools: bool`
- `fallback_schema_ddl: Option<String>` — truncated full DDL when tools unavailable

### Prompt policy

- **Tools on:** system includes table name list + pinned DDL; instruct model to call `get_table_schema` for additional tables.
- **Tools off:** system includes `fallback_schema_ddl` with pinned tables sorted first; no tool definitions.

## Frontend UI

### Root `@` menu

1. **Categories (top):** `Tables` (only if connected), `Files` — chevron when drill-in available.
2. **Separator**
3. **Recent (bottom):** up to ~8 session / `localStorage` items (tables + files).

### Drill-in

- Header: `← Back` + category title.
- Tables: `get_tables(connectionId, database)`.
- Files: existing `context_list_files`.
- Escape / Backspace on empty filter returns to root.

### Filter

- Typing after `@` filters: root = cross-category; nested = current list only.
- Arrow keys + Enter; mouse hover/click.

### Inline tokens (not chips)

- After select: strip `@query` from textarea; show icon + name tokens before the text.
- No per-token × button.
- Backspace at start of text deletes the previous token as one unit; Delete at token boundary same.
- Deduplicate by table name / file path.

### Edge cases

- No connection: hide/disable Tables; Files + recent files only.
- Loading / error states inside picker; do not block typing.
- i18n keys for en + zh-CN at minimum (`context.tables`, `context.files`, `context.recent`, `context.back`, …).

## Backend flow

1. Parse `context_tables` / `context_files`.
2. `SchemaContextPipeline::resolve(...)` → `PromptSeed`.
3. Build system message from seed; attach DB tools when `attach_db_tools`.
4. Inject file contexts into user message (existing).
5. **Tools:** run shared tool loop (Chat already has one; extract/reuse for NL2SQL; prefer streaming consistent with Chat).
6. **No tools:** single completion/stream with `fallback_schema_ddl`.

### Error handling

| Case | Behavior |
|------|----------|
| Pinned table missing / schema fetch fail | Skip table, warn log; continue |
| `get_tables` fail for picker | Show short error in picker |
| `get_tables` fail at send | Omit name list; still use pinned / tools if possible |
| Tool execution error | Return error string as tool result (existing) |
| Max tool rounds | End stream + warn (existing) |
| Tables without connection | Frontend omits Tables; backend ignores invalid `context_tables` |

### Token budgets (initial)

- Pinned DDL ≈ 4000 (len/4 estimate, same style as today).
- Fallback full truncate ≈ 4000 for Chat/NL2SQL path.
- Oversized table name list: truncate with `…and N more`.

## Testing

### Unit

- **Rust:** Pipeline — pinned priority; tools vs fallback; empty tables; budget stop.
- **Vitest:** picker root/drill/filter/back; Backspace token delete; file+table dedupe.

### E2E (required)

Extend `e2e/specs/ai-context.ts` or add `ai-context-tables.ts`; include in `e2e:ai`.

| ID | Coverage |
|----|----------|
| CTX-T01 | Connected Chat: `@` shows Tables / Files (+ optional Recent) |
| CTX-T02 | Enter Tables → see seeded table names; Back to root |
| CTX-T03 | Type after `@` filters list |
| CTX-T04 | Select table → inline token (not chip); Backspace removes token |
| CTX-T05 | File select still works (regression) |
| CTX-T06 | Send includes `context_tables` (assert via IPC args / store / existing e2e spy patterns) |

Fixtures: existing e2e DB with tables; open AI panel. **Do not** assert model prose output.

## Implementation sketch (ordered)

1. Backend `PromptSeed` + `SchemaContextPipeline`; wire Chat `include_schema` path off full dump.
2. Pass `context_tables` through IPC; pinned DDL injection.
3. Extract shared tool-loop helper; NL2SQL adopts it when `supports_tools`.
4. Frontend `ContextItem` + Cursor-style picker + inline tokens + Backspace.
5. i18n; unit tests; E2E CTX-T01–T06.

## Open follow-ups (out of scope)

- Diagnose / MCP migrate onto Pipeline.
- Optional `+` button to open picker without typing `@`.
- Persisted recent across app restarts polish.
