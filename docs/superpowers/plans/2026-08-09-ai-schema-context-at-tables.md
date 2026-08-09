# AI Schema Context (@ Tables) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Chat/NL2SQL full-schema injection with a unified SchemaContextPipeline (table list + `@` pinned DDL + tools), and ship a Cursor-style `@` picker that can select tables as inline tokens (Backspace/Delete to remove), covered by unit tests and E2E.

**Architecture:** New `SchemaContextPipeline` orchestrates `SchemaContextBuilder` into a `PromptSeed`. Chat and NL2SQL consume the seed; when the provider `supports_tools()`, attach existing DB tools and keep the tool loop; otherwise fall back to truncated full DDL. Frontend extends context selection with `ContextItem` (file|table), Cursor-style picker (categories / recent / drill-in), and inline tokens in `AiInput`.

**Tech Stack:** Rust (Tauri commands, `SchemaContextBuilder`), TypeScript/React, Zustand, Vitest, WebdriverIO E2E.

**Spec:** `docs/superpowers/specs/2026-08-09-ai-schema-context-at-tables-design.md`

## Global Constraints

- Scope: **Chat + NL2SQL only**; diagnose / MCP keep legacy `build_sql_context`.
- Hybrid: `@` tables pre-inject DDL **and** keep DB tools; no `@` → tools select schemas.
- No-tools providers: legacy truncated full DDL fallback.
- `@` UX: Cursor-style categories + recent + drill-in + filter; **inline tokens**, not chips with ×.
- Reuse `get_tables`, `db_tool_definitions`, `execute_db_tool`.
- E2E CTX-T01–T06 required; do not assert LLM prose.
- Branch: `feat/ai-schema-context` (worktree `.worktrees/ai-schema-context`).
- IPC: frontend camelCase (`contextTables`) ↔ Rust `context_tables`.

## File map

| File | Responsibility |
|------|----------------|
| `src-tauri/src/ai/schema_pipeline.rs` | `PromptSeed`, `SchemaContextPipeline::resolve`, prompt block helpers |
| `src-tauri/src/ai/mod.rs` | Export pipeline |
| `src-tauri/src/ai/context.rs` | Keep builder; used by pipeline |
| `src-tauri/src/commands/ai.rs` | `context_tables` args; Chat/NL2SQL use pipeline; extract shared tool loop for NL2SQL |
| `src-tauri/src/commands/mod.rs` / `lib.rs` | Wire `schema_context_builder` into pipeline if needed (pipeline can wrap Arc builder) |
| `src/types/index.ts` | `ContextItem`, `ContextKind` |
| `src/commands/ai.ts` | `contextTables` on `chat` / `generateSql` |
| `src/stores/aiStore.ts` | Pass `contextTables` |
| `src/components/ai/ContextPicker.tsx` | Cursor-style picker |
| `src/components/ai/AiInput.tsx` | Inline tokens + Backspace/Delete |
| `src/components/ai/AiChatPanel.tsx` / `Nl2SqlPanel.tsx` / `WorkflowChatPanel.tsx` | Hold `ContextItem[]`, split files/tables |
| `src/locales/en.ts` / `zh-CN.ts` | New context keys |
| `src/components/ai/__tests__/*` | Picker + token tests |
| `e2e/specs/ai-context-tables.ts` | CTX-T01–T06 |
| `package.json` | Include new spec in `e2e:ai` |

---

### Task 1: PromptSeed + pure prompt helpers (Rust)

**Files:**
- Create: `src-tauri/src/ai/schema_pipeline.rs`
- Modify: `src-tauri/src/ai/mod.rs`
- Test: unit tests inside `schema_pipeline.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces:
  - `pub struct PromptSeed { pub database_type: String, pub table_names: Vec<String>, pub pinned_schema_ddl: String, pub attach_db_tools: bool, pub fallback_schema_ddl: Option<String> }`
  - `pub fn format_table_names_block(names: &[String], max_names: usize) -> String`
  - `pub fn compose_schema_system_suffix(seed: &PromptSeed) -> String`

- [ ] **Step 1: Write failing tests** in `schema_pipeline.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_table_names_truncates_with_more() {
        let names: Vec<String> = (0..5).map(|i| format!("t{i}")).collect();
        let block = format_table_names_block(&names, 3);
        assert!(block.contains("t0"));
        assert!(block.contains("t2"));
        assert!(block.contains("and 2 more"));
        assert!(!block.contains("t3"));
    }

    #[test]
    fn compose_includes_pinned_and_tools_hint() {
        let seed = PromptSeed {
            database_type: "Postgres".into(),
            table_names: vec!["users".into(), "orders".into()],
            pinned_schema_ddl: "  users (id int PK)".into(),
            attach_db_tools: true,
            fallback_schema_ddl: None,
        };
        let text = compose_schema_system_suffix(&seed);
        assert!(text.contains("users"));
        assert!(text.contains("orders"));
        assert!(text.contains("users (id int PK)"));
        assert!(text.contains("get_table_schema"));
        assert!(!text.contains("FULL SCHEMA FALLBACK"));
    }

    #[test]
    fn compose_fallback_when_no_tools() {
        let seed = PromptSeed {
            database_type: "Mysql".into(),
            table_names: vec!["a".into()],
            pinned_schema_ddl: String::new(),
            attach_db_tools: false,
            fallback_schema_ddl: Some("  a (id int)\n  b (id int)".into()),
        };
        let text = compose_schema_system_suffix(&seed);
        assert!(text.contains("a (id int)"));
        assert!(!text.contains("get_table_schema"));
    }
}
```

- [ ] **Step 2: Run tests (expect fail)**

```bash
cd src-tauri && cargo test -p datazen --lib ai::schema_pipeline -- --nocapture
```

Expected: compile error / module not found.

- [ ] **Step 3: Implement minimal module**

```rust
//! Unified schema prompt seeding for Chat / NL2SQL.

use crate::ai::context::SchemaContextBuilder;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct PromptSeed {
    pub database_type: String,
    pub table_names: Vec<String>,
    pub pinned_schema_ddl: String,
    pub attach_db_tools: bool,
    pub fallback_schema_ddl: Option<String>,
}

pub fn format_table_names_block(names: &[String], max_names: usize) -> String {
    if names.is_empty() {
        return "(no tables)".to_string();
    }
    if names.len() <= max_names {
        return names.join(", ");
    }
    let head: Vec<&str> = names.iter().take(max_names).map(String::as_str).collect();
    format!(
        "{}, …and {} more",
        head.join(", "),
        names.len() - max_names
    )
}

pub fn compose_schema_system_suffix(seed: &PromptSeed) -> String {
    let names = format_table_names_block(&seed.table_names, 200);
    let mut out = format!(
        "Database type: {}\nAvailable tables:\n{}\n",
        seed.database_type, names
    );
    if !seed.pinned_schema_ddl.trim().is_empty() {
        out.push_str("\nPinned table schemas (user @ selection):\n");
        out.push_str(&seed.pinned_schema_ddl);
        out.push('\n');
    }
    if seed.attach_db_tools {
        out.push_str(
            "\nUse list_tables / get_table_schema tools to fetch schemas for tables you need beyond the pinned set.\n",
        );
    } else if let Some(fb) = &seed.fallback_schema_ddl {
        out.push_str("\nSchema:\n");
        out.push_str(fb);
        out.push('\n');
    }
    out
}

pub struct SchemaContextPipeline {
    builder: Arc<SchemaContextBuilder>,
}

impl SchemaContextPipeline {
    pub fn new(builder: Arc<SchemaContextBuilder>) -> Self {
        Self { builder }
    }

    // resolve implemented in Task 2
}
```

Export from `ai/mod.rs`:

```rust
pub mod schema_pipeline;
pub use schema_pipeline::{PromptSeed, SchemaContextPipeline, compose_schema_system_suffix};
```

- [ ] **Step 4: Run tests (expect pass)**

```bash
cd src-tauri && cargo test -p datazen --lib ai::schema_pipeline -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ai/schema_pipeline.rs src-tauri/src/ai/mod.rs
git commit -m "feat(ai): add PromptSeed helpers for schema context pipeline"
```

---

### Task 2: `SchemaContextPipeline::resolve`

**Files:**
- Modify: `src-tauri/src/ai/schema_pipeline.rs`
- Modify: `src-tauri/src/lib.rs` (construct pipeline next to builder) **or** construct ad-hoc in commands from `state.schema_context_builder` (prefer ad-hoc `SchemaContextPipeline::new(state.schema_context_builder.clone())` to avoid AppState churn)

**Interfaces:**
- Consumes: `SchemaContextBuilder::{get_table_names, build_selective_context, build_sql_context}`
- Produces:
  - `impl SchemaContextPipeline { pub async fn resolve(&self, connection_id: &str, database: &str, pinned_tables: &[String], supports_tools: bool, pinned_budget: usize, fallback_budget: usize) -> Result<PromptSeed, String> }`

- [ ] **Step 1: Write failing test for resolve logic via a small pure helper** (keep async resolve thin):

```rust
#[test]
fn decide_seed_fields_tools_on() {
    let seed = assemble_seed(
        "Postgres".into(),
        vec!["u".into()],
        "  u (id int)".into(),
        true,
        Some("SHOULD_NOT_USE".into()),
    );
    assert!(seed.attach_db_tools);
    assert!(seed.fallback_schema_ddl.is_none());
    assert_eq!(seed.pinned_schema_ddl, "  u (id int)");
}

#[test]
fn decide_seed_fields_tools_off_keeps_fallback() {
    let seed = assemble_seed(
        "Postgres".into(),
        vec!["u".into()],
        String::new(),
        false,
        Some("  u (id int)".into()),
    );
    assert!(!seed.attach_db_tools);
    assert_eq!(seed.fallback_schema_ddl.as_deref(), Some("  u (id int)"));
}

fn assemble_seed(
    database_type: String,
    table_names: Vec<String>,
    pinned_schema_ddl: String,
    supports_tools: bool,
    fallback_schema_ddl: Option<String>,
) -> PromptSeed {
    PromptSeed {
        database_type,
        table_names,
        pinned_schema_ddl,
        attach_db_tools: supports_tools,
        fallback_schema_ddl: if supports_tools { None } else { fallback_schema_ddl },
    }
}
```

- [ ] **Step 2: Implement `resolve`**

```rust
impl SchemaContextPipeline {
    pub async fn resolve(
        &self,
        connection_id: &str,
        database: &str,
        pinned_tables: &[String],
        supports_tools: bool,
        pinned_budget: usize,
        fallback_budget: usize,
    ) -> Result<PromptSeed, String> {
        let (db_type, table_names) = self
            .builder
            .get_table_names(connection_id, database)
            .await
            .unwrap_or_else(|_| (String::new(), Vec::new()));

        let pinned_schema_ddl = if pinned_tables.is_empty() {
            String::new()
        } else {
            self.builder
                .build_selective_context(connection_id, database, pinned_tables, pinned_budget)
                .await
                .map(|c| c.schema_ddl)
                .unwrap_or_default()
        };

        let fallback_schema_ddl = if supports_tools {
            None
        } else {
            let ctx = self
                .builder
                .build_sql_context(connection_id, database, None, &[], fallback_budget)
                .await?;
            Some(ctx.schema_ddl)
        };

        Ok(assemble_seed(
            db_type,
            table_names,
            pinned_schema_ddl,
            supports_tools,
            fallback_schema_ddl,
        ))
    }
}
```

Put `assemble_seed` as a private/pub(crate) fn used by tests + resolve.

- [ ] **Step 3: Run unit tests**

```bash
cd src-tauri && cargo test -p datazen --lib ai::schema_pipeline -- --nocapture
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/ai/schema_pipeline.rs
git commit -m "feat(ai): resolve PromptSeed via SchemaContextPipeline"
```

---

### Task 3: Wire `ai_chat` to Pipeline + `context_tables`

**Files:**
- Modify: `src-tauri/src/commands/ai.rs` (`ai_chat` signature + `include_schema` block ~750–798)

**Interfaces:**
- Consumes: `SchemaContextPipeline::resolve`, `compose_schema_system_suffix`, `provider.supports_tools()`
- Produces: `ai_chat(..., context_tables: Option<Vec<String>>)`

- [ ] **Step 1: Add arg and replace full `build_sql_context` injection**

In `ai_chat` parameters add:

```rust
context_tables: Option<Vec<String>>,
```

Replace the `if include_schema { ... build_sql_context ... Schema:\n{} }` block with:

```rust
if include_schema {
    if let Some(ref conn_id) = connection_id {
        let db = database.as_deref().unwrap_or("");
        let pinned = context_tables.clone().unwrap_or_default();
        let supports_tools = provider.supports_tools();
        let pipeline = SchemaContextPipeline::new(state.schema_context_builder.clone());
        if let Ok(seed) = pipeline
            .resolve(conn_id, db, &pinned, supports_tools, 4000, 4000)
            .await
        {
            // resolve prompt template as today (Chat / WorkflowGenerate), then append:
            let suffix = compose_schema_system_suffix(&seed);
            // push system message = base_tpl + connection blurb + suffix
            // set request.tools = if seed.attach_db_tools { Some(all_tools) } else { None or only ask_questions }
        }
    }
}
```

Details to preserve from current code:
- Workflow scenario still uses `build_connections_context` / `WorkflowGenerate` template.
- Non-workflow zh/en “user is connected…” blurb stays.
- File context injection (`context_files`) unchanged.
- When `attach_db_tools` is false: still allow `ask_questions` tool if desired; **omit** `db_tool_definitions()` (or keep ask_questions only). Spec: no DB tools on fallback path.

Move `let (provider, ai_config) = resolve_ai` **above** schema seeding if not already (needed for `supports_tools`).

- [ ] **Step 2: Compile**

```bash
cd src-tauri && cargo check -p datazen
```

Expected: success.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/ai.rs
git commit -m "feat(ai): chat uses SchemaContextPipeline and context_tables"
```

---

### Task 4: NL2SQL uses Pipeline + shared tool loop

**Files:**
- Modify: `src-tauri/src/commands/ai.rs` (`ai_generate_sql` ~194–340; extract helper from chat loop ~914–1065)

**Interfaces:**
- Produces:
  - `async fn run_db_tool_stream_loop(...)` extracted from chat (or shared internal fn)
  - `ai_generate_sql(..., context_tables: Option<Vec<String>>)`

- [ ] **Step 1: Extract tool loop**

Refactor the chat streaming tool loop into something like:

```rust
async fn run_streaming_tool_loop(
    provider: Arc<dyn AiProvider>,
    state: &AppState,
    window: &WebviewWindow,
    request_id: &str,
    mut request: CompletionRequest,
    max_rounds: usize,
) -> Result<(), CommandError>
```

Chat calls it; behavior unchanged for DB tools / ask_questions.

- [ ] **Step 2: Change `ai_generate_sql`**

- Add `context_tables: Option<Vec<String>>`.
- Replace `build_sql_context(..., 4000)` with Pipeline resolve (`pinned_budget=4000`, `fallback_budget=4000`).
- System prompt: existing Nl2Sql template vars — set `schema` to `compose_schema_system_suffix(&seed)` **or** `pinned + fallback` content consistent with template (`{{schema}}`). Prefer putting the composed suffix into `schema` var so templates keep working.
- If `seed.attach_db_tools`: set `request.tools = Some(db_tool_definitions())` and call `run_streaming_tool_loop` instead of single `stream_complete` without tools.
- If not: keep current single-stream path.

- [ ] **Step 3: Compile + focused tests**

```bash
cd src-tauri && cargo test -p datazen --lib ai::schema_pipeline && cargo check -p datazen
```

Expected: PASS / success.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/ai.rs
git commit -m "feat(ai): NL2SQL uses pipeline and schema tool loop"
```

---

### Task 5: Frontend types + IPC wiring

**Files:**
- Modify: `src/types/index.ts`
- Modify: `src/commands/ai.ts`
- Modify: `src/stores/aiStore.ts` (`sendChatMessage`, `generateSql`, workflow send)

**Interfaces:**
- Produces:
  - `export type ContextKind = 'file' | 'dir' | 'table'`
  - `export interface ContextItem { kind: ContextKind; id: string; name: string; path?: string; database?: string }`
  - Keep `ContextEntry` for file-tree IPC shape from backend
  - Helpers:

```ts
export function splitContextItems(items: ContextItem[]): {
  contextFiles: string[];
  contextTables: string[];
} {
  const contextFiles: string[] = [];
  const contextTables: string[] = [];
  for (const it of items) {
    if (it.kind === 'table') contextTables.push(it.id);
    else if (it.kind === 'file' || it.kind === 'dir') contextFiles.push(it.path ?? it.id);
  }
  return { contextFiles, contextTables };
}
```

Place helper in `src/lib/contextItems.ts` (create) to keep types file thinner.

- [ ] **Step 1: Add types + helper + Vitest**

Create `src/lib/contextItems.ts` and `src/lib/__tests__/contextItems.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { splitContextItems } from '../contextItems';

describe('splitContextItems', () => {
  it('splits tables and files', () => {
    const r = splitContextItems([
      { kind: 'table', id: 'users', name: 'users' },
      { kind: 'file', id: 'a.sql', name: 'a.sql', path: 'a.sql' },
    ]);
    expect(r.contextTables).toEqual(['users']);
    expect(r.contextFiles).toEqual(['a.sql']);
  });
});
```

- [ ] **Step 2: Run test fail then implement helper / pass**

```bash
npx vitest run src/lib/__tests__/contextItems.test.ts
```

- [ ] **Step 3: Wire commands + store**

```ts
// ai.ts generateSql + chat params
contextTables?: string[];
```

Store send paths:

```ts
const { contextFiles, contextTables } = splitContextItems(items);
// pass both to aiCommands.chat / generateSql
```

Panels will switch to `ContextItem[]` in Task 7–8; for this task, update function signatures to accept optional `contextTables?: string[]` alongside existing `contextFiles`.

- [ ] **Step 4: Commit**

```bash
git add src/types/index.ts src/lib/contextItems.ts src/lib/__tests__/contextItems.test.ts src/commands/ai.ts src/stores/aiStore.ts
git commit -m "feat(ai): wire contextTables through frontend IPC"
```

---

### Task 6: Cursor-style `ContextPicker`

**Files:**
- Rewrite: `src/components/ai/ContextPicker.tsx`
- Modify: `src/components/ai/__tests__/ContextPicker.test.tsx`
- Uses: `databaseCommands.getTables` (or existing `src/commands/database.ts` `get_tables`), `contextCommands.listFiles`

**Interfaces:**
- Props:

```ts
interface ContextPickerProps {
  query: string;
  onSelect: (item: ContextItem) => void;
  onClose: () => void;
  anchorRef: React.RefObject<HTMLElement | null>;
  position?: 'above' | 'below';
  connectionId?: string;
  database?: string;
}
```

- DOM: root container `data-testid="context-picker"`
- Category rows: `data-testid="context-cat-tables"` / `context-cat-files`
- Back: `data-testid="context-picker-back"`
- Items: `data-testid="context-item"` with `data-kind` / `data-id`

- [ ] **Step 1: Rewrite tests for root / drill / filter** (mock `get_tables` + `listFiles`)

```ts
it('shows Tables and Files categories at root', async () => { /* ... */ });
it('drills into Tables and lists names', async () => { /* ... */ });
it('filters across categories when query set at root', async () => { /* ... */ });
it('back returns to root', async () => { /* ... */ });
```

- [ ] **Step 2: Implement picker**

Behavior:
- State: `view: 'root' | 'tables' | 'files'`
- Root without query: categories (Tables only if `connectionId`) + recent from `localStorage` key `datazen.contextRecent` (JSON array of `ContextItem`, max 8)
- Root with query: flat filter over tables (fetch once) + files
- Nested: list + Back button
- Keyboard: ArrowUp/Down/Enter/Escape (Escape pops nested first)
- On select: push to recent, call `onSelect`

- [ ] **Step 3: Vitest pass**

```bash
npx vitest run src/components/ai/__tests__/ContextPicker.test.tsx
```

- [ ] **Step 4: Commit**

```bash
git add src/components/ai/ContextPicker.tsx src/components/ai/__tests__/ContextPicker.test.tsx
git commit -m "feat(ai): Cursor-style @ context picker with tables"
```

---

### Task 7: `AiInput` inline tokens + Backspace/Delete

**Files:**
- Modify: `src/components/ai/AiInput.tsx`
- Modify: `src/components/ai/__tests__/AiInput.test.tsx`
- Modify panels to use `ContextItem[]` + `onContextItemsChange`

**Interfaces:**
- Replace `contextFiles` / `onContextFilesChange` with:

```ts
contextItems?: ContextItem[];
onContextItemsChange?: (items: ContextItem[]) => void;
```

(Update all call sites in same task.)

- Tokens: `data-testid="context-token"` `data-kind=` `data-id=`
- No × button
- Backspace when `selectionStart === 0` and value has no leading text selection → remove last token
- Delete when caret at 0 similarly if needed
- Selecting from picker: dedupe by `kind+id`, strip `@query`

- [ ] **Step 1: Failing Vitest for Backspace token delete + no chip ×**

```ts
it('renders inline tokens without remove buttons', () => { /* queryByRole times 0 for token dismiss */ });
it('Backspace at start removes last token', async () => { /* ... */ });
```

- [ ] **Step 2: Implement + update Chat/NL2SQL/Workflow panels** to `useState<ContextItem[]>` and `splitContextItems` on send.

- [ ] **Step 3: Vitest pass**

```bash
npx vitest run src/components/ai/__tests__/AiInput.test.tsx src/components/ai/__tests__/ContextPicker.test.tsx
```

- [ ] **Step 4: Commit**

```bash
git add src/components/ai/AiInput.tsx src/components/ai/__tests__/AiInput.test.tsx \
  src/components/ai/AiChatPanel.tsx src/components/ai/Nl2SqlPanel.tsx src/components/ai/WorkflowChatPanel.tsx
git commit -m "feat(ai): inline @ context tokens with Backspace delete"
```

---

### Task 8: i18n

**Files:**
- Modify: `src/locales/en.ts`, `src/locales/zh-CN.ts` (and mirror keys in other locales as empty/English fallback if project requires all keys — follow existing pattern: add to all locale files that typecheck against a key union, or only en/zh-CN if keys are loose)

Keys:

```ts
'context.tables': 'Tables',
'context.files': 'Files',
'context.recent': 'Recent',
'context.back': 'Back',
'context.noTables': 'No tables',
'context.placeholder': 'Type @ for tables or files…',
```

Update zh-CN equivalents. Update placeholder string used by `AiInput`.

- [ ] **Step 1: Add keys + use in picker**
- [ ] **Step 2: Commit**

```bash
git add src/locales/en.ts src/locales/zh-CN.ts src/components/ai/ContextPicker.tsx
git commit -m "i18n(ai): context picker tables/files strings"
```

---

### Task 9: E2E CTX-T01–T06

**Files:**
- Create: `e2e/specs/ai-context-tables.ts`
- Modify: `package.json` `e2e:ai` script to include the new spec (and keep `ai-context.ts` if not already — add both):

```json
"e2e:ai": "node e2e/run.mjs --skip-build -- --spec e2e/specs/ai-features.ts,e2e/specs/ai-context.ts,e2e/specs/ai-context-tables.ts"
```

**Setup pattern:** reuse helpers from `ai-context.ts` / `sqlite` connection open used elsewhere. Prefer SQLite e2e DB with known tables. Add `data-testid` selectors from Tasks 6–7.

- [ ] **Step 1: Write spec**

```ts
describe('AI context tables (CTX-T01~T06)', () => {
  // before: open connection with tables, open AI chat input

  it('CTX-T01: @ shows Tables and Files categories', async () => {
    await textarea.setValue('@');
    await expect($('[data-testid="context-cat-tables"]')).toBeExisting();
    await expect($('[data-testid="context-cat-files"]')).toBeExisting();
  });

  it('CTX-T02: drill Tables then back', async () => {
    await $('[data-testid="context-cat-tables"]').click();
    await expect($('[data-testid="context-picker-back"]')).toBeExisting();
    // at least one context-item with data-kind=table
    await $('[data-testid="context-picker-back"]').click();
    await expect($('[data-testid="context-cat-tables"]')).toBeExisting();
  });

  it('CTX-T03: filter narrows items', async () => { /* type after @ */ });

  it('CTX-T04: select table → token; Backspace removes', async () => {
    // select a table item
    await expect($('[data-testid="context-token"][data-kind="table"]')).toBeExisting();
    // clear text, Backspace
    await browser.keys(['Backspace']);
    await expect($('[data-testid="context-token"][data-kind="table"]')).not.toBeExisting();
  });

  it('CTX-T05: file select still works', async () => { /* seed file like ai-context.ts */ });

  it('CTX-T06: send includes context_tables', async () => {
    // Prefer: stub/spy via browser.execute wrapping __TAURI_INTERNALS__.invoke
    // Capture args to ai_chat / ai_generate_sql and expect contextTables array non-empty
  });
});
```

For CTX-T06, install invoke spy in `before`:

```ts
await browser.execute(() => {
  const w = window as any;
  w.__invokeCalls = [];
  const inv = w.__TAURI_INTERNALS__.invoke.bind(w.__TAURI_INTERNALS__);
  w.__TAURI_INTERNALS__.invoke = (cmd: string, args: any) => {
    w.__invokeCalls.push({ cmd, args });
    return inv(cmd, args);
  };
});
```

Then after send, read `__invokeCalls` filtered by `ai_chat` / `ai_generate_sql`.

- [ ] **Step 2: Run E2E** (requires existing webdriver debug binary per project docs)

```bash
pnpm e2e:ai
```

Expected: new cases green (or fix selectors until green). If binary missing, `pnpm e2e:minimal` once then re-run.

- [ ] **Step 3: Commit**

```bash
git add e2e/specs/ai-context-tables.ts package.json src/components/ai/
git commit -m "test(e2e): AI @ table context picker CTX-T01–T06"
```

---

### Task 10: Smoke + docs touch-up

**Files:**
- Optional one-line pointer in `docs/architecture` only if an AI context section already exists; otherwise skip (YAGNI).
- Run:

```bash
cd src-tauri && cargo test -p datazen --lib ai::schema_pipeline
npx vitest run src/lib/__tests__/contextItems.test.ts src/components/ai/__tests__/ContextPicker.test.tsx src/components/ai/__tests__/AiInput.test.tsx
```

- [ ] **Step 1: Fix any fallout from renames**
- [ ] **Step 2: Final commit if needed**

```bash
git commit -m "chore(ai): schema context follow-ups"
```

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| Unified SchemaContextPipeline | 1–2 |
| Chat list-then-tools / pinned | 3 |
| NL2SQL same + tool loop | 4 |
| `context_tables` IPC | 3–5 |
| No-tools fallback | 2–4 |
| `@` pinned + tools still on | 2–3 |
| Cursor picker categories/recent/drill/filter | 6 |
| Inline tokens + Backspace | 7 |
| i18n | 8 |
| Unit tests | 1,5,6,7 |
| E2E CTX-T01–T06 | 9 |
| Diagnose/MCP unchanged | (no task — leave code paths) |

## Self-review notes

- No TBD placeholders in tasks.
- `PromptSeed` / `contextTables` naming consistent across tasks.
- `assemble_seed` defined in Task 2 and used by resolve.
- E2E script updated so cases are not orphaned.
