# DataZen Workflow Guide

> This document describes the full YAML syntax and usage of DataZen **Workflows**.  
> Source of truth: `src-tauri/src/workflow/workflows.rs`, in-repo test YAML, and the `workflow_generate` prompt.  
> **On-disk YAML must use snake_case field names** (e.g. `timeout_secs`, `then_steps`, `as_var`). Frontend TypeScript / IPC responses may use camelCase — that is a serialization difference. **Do not write camelCase in hand-authored YAML.**

---

## 1. Overview

A Workflow is a reusable automation: chain **SQL queries, AI analysis, condition branches, and loops** in YAML, with variable substitution and **cross-database** support (each step can bind a different connection).

| Capability | Description |
|------------|-------------|
| Step types | `query` / `ai` / `condition` / `foreach` |
| Variables | `string` / `number` / `connection`, optional defaults |
| Templates | `{{...}}` in SQL, prompts, connections, output, etc. |
| Cross-DB | Per-step `connection` / `database` |
| Error strategy | Global or per-step: `abort` / `skip` / `fallback` |
| Entry points | Connection AI sidebar, dedicated Workflow window, MCP `run_workflow`, AI chat generation |

**Current limitations**

- No dedicated “script / HTTP” step type  
- The right-hand side of a `condition` comparison is **not** resolved as a step path (see [§7](#7-condition-expressions))  
- `foreach` defaults to at most 100 iterations  
- MCP `run_workflow` usually returns only the final text output, not per-step details  

---

## 2. Files and lifecycle

### 2.1 Storage

- Workflow files: `{app data dir}/workflows/{id}.yaml`  
- Run history: `{app data dir}/workflow_history/*.json` (about 100 entries retained)  

In the UI, use the “workflow directory” affordances to see the path (IPC: `workflow_get_dir`).

### 2.2 Loading

- First list/get lazily loads `.yaml` / `.yml` from the directory  
- After external edits, **Refresh / `workflow_reload`** is required to rescan  
- Prefer filename matching `id`; the YAML `id` field is the registry key  

### 2.3 Validation

On save/parse, at least:

- Non-empty `id`, `name`, `description`  
- Non-empty `steps` array  

`variables` may be omitted (defaults to `[]`).

---

## 3. Quick start

### 3.1 Create in the UI

1. Open a database connection window  
2. Open the AI sidebar → **Workflows** tab → **New**  
3. Or open the dedicated **Workflow** window (menu / shortcut)  

Note: the sidebar form supports `condition` / `foreach` more fully; the dedicated window’s simple form may only cover `query` + `ai`. For complex steps, edit YAML directly.

### 3.2 Minimal runnable example

Save the following as `workflows/daily-report.yaml` (or save via UI). Run with AI configured and a connection selected:

```yaml
id: daily-report
name: Daily report
description: Query today’s orders and summarize
version: "1.0"

variables:
  - name: date
    type: string
    description: Query date (YYYY-MM-DD)
    required: true
    default: "2024-01-01"

steps:
  - type: query
    id: get_orders
    sql: |
      SELECT COUNT(*) AS total, COALESCE(SUM(amount), 0) AS revenue
      FROM orders
      WHERE order_date = '{{date}}'

  - type: ai
    id: summary
    prompt: |
      Write a short daily summary from the following query result:
      row count: {{steps.get_orders.rows_count}}
      data: {{steps.get_orders.result}}

output:
  format: text
  template: "{{steps.summary.result}}"
```

Key points:

- Field names are **snake_case** (`timeout_secs`), not `timeoutSecs`  
- Query results use `rows` / `rows_count`; `{{steps.xxx.result}}` on a query **falls back to JSON of `rows`** (compat)  
- AI step body is in `{{steps.summary.result}}`  

---

## 4. Top-level schema

```yaml
id: string                 # required, unique id
name: string               # required, display name
description: string        # required
version: string            # optional
author: string             # optional
variables: []              # optional, default []
steps: []                  # required, at least one step
output:                    # optional
  format: string           # e.g. text / markdown (display hint; execution uses template)
  template: string         # optional; default = last step result
timeout_secs: number       # optional, whole-workflow timeout, default 300
error_handling:            # optional, default strategy: abort
  strategy: abort | skip | fallback
  fallback_steps: []       # used when strategy is fallback
```

### Field summary

| Field | Type | Required | Default | Notes |
|-------|------|----------|---------|-------|
| `id` | string | yes | — | Registry / MCP `workflow_id` |
| `name` | string | yes | — | UI list title |
| `description` | string | yes | — | Description |
| `version` | string | no | — | Documentation |
| `author` | string | no | — | Documentation |
| `variables` | array | no | `[]` | Runtime inputs |
| `steps` | array | yes | — | Steps (may nest in condition/foreach) |
| `output` | object | no | last step | Final output |
| `timeout_secs` | u64 | no | `300` | Global timeout (seconds) |
| `error_handling` | object | no | `abort` | Default error strategy |

Serde uses `#[serde(tag = "type")]`: every step object must have `type: query|ai|condition|foreach`.

---

## 5. Variables

```yaml
variables:
  - name: uid
    type: string          # string | number | connection
    description: User ID
    required: true        # optional, default false
    default: "u001"       # optional; filled when not provided at runtime
```

### 5.1 Types

| `type` | Meaning | UI |
|--------|---------|-----|
| `string` | Plain string | Text input |
| `number` | Numeric (still enters template context as string) | Number input |
| `connection` | Connection config ID | Connection picker; use as `connection: "{{name}}"` |

### 5.2 Required and defaults

At run start:

1. Fill context from caller `variables` JSON  
2. Inject built-ins (below)  
3. Apply `default` for defined vars still unset  
4. If `required: true` and value missing or empty string → **fail** (`Required variable 'x' is missing`)  

### 5.3 Built-in variables (injected every run)

| Name | Example | Notes |
|------|---------|-------|
| `current_date` | `2026-08-07` | Local date |
| `current_month` | `2026-08` | Local year-month |
| `current_year` | `2026` | Local year |

Usage: `WHERE d = '{{current_date}}'`.

---

## 6. Template syntax `{{...}}`

The engine replaces with regex `\{\{([^}]+)\}\}`; surrounding whitespace in the expression is trimmed.

### 6.1 Supported patterns

| Pattern | Example | Result |
|---------|---------|--------|
| Input / built-in | `{{uid}}`, `{{current_date}}` | String |
| Query row field | `{{steps.get_orders.rows.0.order_id}}` | First row, that column |
| Query row count | `{{steps.get_orders.rows_count}}` | e.g. `3` |
| Wildcard (IN list) | `{{steps.get_orders.rows.*.order_id}}` | `'a','b','c'` (quoted, comma-joined) |
| AI text | `{{steps.summary.result}}` | Model string |
| Query `result` | `{{steps.get_orders.result}}` | If no dedicated `result`, falls back to JSON string of `rows` |
| Loop object field | `{{order.order_id}}` | foreach `as_var` |
| Loop scalar | `{{item}}` | When current element is a simple value |
| Index syntax | `{{steps.s1.result[0].name}}` | Compat; empty `result`/`data` falls back to `rows` |

### 6.2 Resolution order (one expression)

1. Starts with `steps.` → take that step’s structured result, then path  
2. Else if contains `.` and left side is a foreach loop var → field on loop object  
3. Else if whole expression is a loop var name → that value  
4. Else look up input/built-in map; missing → **empty string** (does not abort the template)  

### 6.3 `rows.*.column` details

- Intended for SQL `IN (...)`  
- Each value wrapped in single quotes: `'ORD-1','ORD-2'`  
- **No** SQL escaping; values containing `'` must be handled by you (no bind-parameter API yet)  
- Intermediate `result` / `data` in the path may fall back to `rows`  

### 6.4 Recommended style

```yaml
# Single value
sql: "SELECT * FROM t WHERE id = '{{steps.s1.rows.0.id}}'"

# Multi-value IN
sql: "SELECT * FROM t WHERE id IN ({{steps.s1.rows.*.id}})"

# Whole table into AI (JSON)
prompt: |
  Data:
  {{steps.s1.result}}
```

---

## 7. Condition expressions

Used in the `if` field of `condition` steps.

### 7.1 Processing order

1. Run **template substitution** on the `if` string (`{{...}}`)  
2. Then evaluate the condition  

**Recommended: when comparing step paths, do not wrap them in `{{}}`:**

```yaml
if: "steps.get_orders.rows_count > 0"
```

If you write `if: "{{steps.get_orders.rows_count}} > 0"`, the template becomes `3 > 0` first; the evaluator treats left-hand `3` as a variable name, usually resolves to empty, and the comparison breaks.

### 7.2 Supported forms

**Suffix checks**

| Expression | True when |
|------------|-----------|
| `steps.s1.rows_count.is_empty` | Value empty, `"0"`, `"null"`, or `"[]"` |
| `steps.s1.rows_count.is_not_empty` | Opposite |

**Binary comparison** (operators matched in order: `>=` `<=` `!=` `==` `>` `<`)

```yaml
if: "steps.s1.rows_count > 0"
if: "status == 'active'"
if: "steps.s1.rows_count != 0"
```

- Left: `resolve_expression` (`steps....` or variable name)  
- Right: literal after stripping one layer of quotes; **not** resolved as `steps.xxx`  
- If both parse as numbers → float compare; else string compare  

**Truthy (no operator)**

```yaml
if: "some_flag"
```

True when non-empty and not `"0"` / `"false"` / `"null"`.

### 7.3 Limits

- No `&&` / `||` / parenthesized compound logic  
- Right side cannot be another step path (e.g. `steps.a.x > steps.b.y` is invalid)  

---

## 8. Step types

Every step has an `id` (string; unique within the workflow for `steps.<id>...` references).

---

### 8.1 `query` — SQL

```yaml
- type: query
  id: get_orders
  sql: "SELECT order_id, amount FROM orders WHERE uid = '{{uid}}'"
  connection: "{{pg_conn}}"   # optional
  database: "{{db_name}}"     # optional; use_database before run
  timeout_secs: 10            # optional, default 30
  on_error:                   # optional, overrides global
    strategy: skip
```

| Field | Required | Default | Notes |
|-------|----------|---------|-------|
| `id` | yes | — | Step id |
| `sql` | yes | — | Supports `{{...}}`; trailing `;` stripped before run |
| `connection` | no | default connection at execute | After template: session id or **saved connection config id** |
| `database` | no | — | If non-empty, `use_database` first |
| `timeout_secs` | no | `30` | Step timeout |
| `on_error` | no | global `error_handling` | See [§10](#10-timeouts-and-error-handling) |

**Connection resolution order**

1. Step `connection` (after template)  
2. Else `connection_id` from `workflow_execute` / MCP  
3. Neither → error: `Query step requires a database connection`  

`resolve_connection` accepts:

- An already-open session id  
- Or a config-store connection id (`get_or_connect`)  

**Context written on success**

```json
{
  "rows": [ { "order_id": "O1", "amount": 10 }, ... ],
  "rows_count": 2,
  "columns": [ /* driver column metadata */ ],
  "execution_time_ms": 12
}
```

The UI may also record `sql_executed` / `connection_name` on the result object (not necessarily in template context).

---

### 8.2 `ai` — AI analysis

```yaml
- type: ai
  id: summary
  prompt: |
    Summarize in English:
    {{steps.get_orders.result}}
  timeout_secs: 60
  on_error:
    strategy: abort
```

| Field | Required | Default | Notes |
|-------|----------|---------|-------|
| `id` | yes | — | Step id |
| `prompt` | yes | — | Templates; uses configured AI provider |
| `timeout_secs` | no | `30` | Step timeout |
| `on_error` | no | global | Same as query |

**Success shape**

```json
{ "result": "model text" }
```

Reference: `{{steps.summary.result}}`.  
AI must be configured in settings; temperature is set inside the executor (~`0.3`).

---

### 8.3 `condition` — branch

```yaml
- type: condition
  id: check_orders
  if: "steps.get_orders.rows_count > 0"
  then_steps:
    - type: query
      id: get_logistics
      connection: "{{mysql_conn}}"
      sql: "SELECT * FROM logistics WHERE order_id IN ({{steps.get_orders.rows.*.order_id}})"
  else_steps:
    - type: ai
      id: no_data
      prompt: "User {{uid}} has no orders. Reply in one sentence."
```

| Field | Required | Notes |
|-------|----------|-------|
| `id` | yes | Step id |
| `if` | yes | Condition expression (YAML key is `if`) |
| `then_steps` | yes | Steps when true |
| `else_steps` | no | Steps when false; omit to skip |

- The condition step itself records success: `{ "condition": true|false }`  
- **No** step-level `on_error` / `timeout_secs` (global timeout still applies)  
- `then_steps` / `else_steps` may nest any step types  

---

### 8.4 `foreach` — loop

```yaml
- type: foreach
  id: per_order
  items: "steps.get_orders.rows"
  as_var: order
  max_iterations: 50
  steps:
    - type: query
      id: one_ship
      connection: "{{mysql_conn}}"
      sql: |
        SELECT * FROM logistics WHERE order_id = '{{order.order_id}}'
```

| Field | Required | Default | Notes |
|-------|----------|---------|-------|
| `id` | yes | — | Step id |
| `items` | yes | — | See resolution below |
| `as_var` | yes | — | Name inside the loop body |
| `steps` | yes | — | Sub-steps each iteration |
| `max_iterations` | no | `100` | Cap |

**`items` resolution**

1. Template substitution  
2. Try parse result as JSON  
3. On failure, treat string as a deep path (e.g. `steps.get_orders.rows`) via `resolve_deep_path`  
4. Must be a **JSON array**; otherwise this foreach is **Skipped** with an error note  

Prefer a bare path (no `{{}}`):

```yaml
items: "steps.get_orders.rows"
```

Inside the loop:

- Object element: `{{order.field}}`  
- Scalar element: `{{order}}` (when `as_var` is `order`)  

**foreach result shape**

```json
{
  "iterations_completed": 2,
  "iterations": [
    { "index": 0, "steps": [ /* per-iteration step records */ ] },
    { "index": 1, "steps": [ ... ] }
  ]
}
```

If a sub-step fails with `abort`, the whole workflow fails.

---

## 9. Connections and cross-database

### 9.1 Three binding layers

| Layer | Source | Role |
|-------|--------|------|
| Execute default | UI current connection / IPC `connectionId` / MCP `connection_id` | Used by queries without `connection` |
| Step connection | `connection: "{{pg_conn}}"` or literal id | Overrides default |
| Database name | `database: "mydb"` | Switch DB after connect |

### 9.2 Cross-database pattern

Declare both connections as `type: connection` variables and reference them per step:

```yaml
variables:
  - name: pg_conn
    type: connection
    description: Orders DB (PostgreSQL)
    required: true
  - name: mysql_conn
    type: connection
    description: Logistics DB (MySQL)
    required: true
  - name: uid
    type: string
    description: User ID
    required: true

steps:
  - type: query
    id: get_orders
    connection: "{{pg_conn}}"
    sql: |
      SELECT order_id, product_name, amount
      FROM test_orders
      WHERE uid = '{{uid}}'
      ORDER BY created_at DESC

  - type: condition
    id: check_orders
    if: "steps.get_orders.rows_count > 0"
    then_steps:
      - type: query
        id: get_logistics
        connection: "{{mysql_conn}}"
        sql: |
          SELECT order_id, carrier, tracking_no, status
          FROM test_logistics
          WHERE order_id IN ({{steps.get_orders.rows.*.order_id}})

output:
  format: markdown
  template: |
    ## User {{uid}}

    ### Orders
    {{steps.get_orders.result}}

    ### Logistics
    {{steps.get_logistics.result}}
```

Reference sample: `scripts/test-cross-db-workflow.yaml`.

### 9.3 Dedicated Workflow window

Runs from the dedicated window **may not** pass a default connection; a bare query without `connection` will fail. Cross-DB workflows should declare `connection` variables or literal connection ids.

---

## 10. Timeouts and error handling

### 10.1 Timeouts

| Scope | Field | Default |
|-------|-------|---------|
| Whole workflow | top-level `timeout_secs` | `300` |
| Single query/ai | step `timeout_secs` | `30` |

Global timeout is checked in the step loop; errors look like `Global timeout (300s) exceeded`.  
Step timeout status is `timed_out`, then error strategy applies.

### 10.2 Strategies

```yaml
error_handling:
  strategy: abort          # abort | skip | fallback
  fallback_steps: []       # fallback only
```

Per-step ( **query / ai** only):

```yaml
on_error:
  strategy: fallback
  fallback_steps:
    - type: query
      id: safe_fallback
      sql: "SELECT 'unavailable' AS error"
```

| Strategy | Behavior |
|----------|----------|
| `abort` | Record failure and stop (default) |
| `skip` | Mark step skipped/timed_out, set that step’s context to `null`, continue |
| `fallback` | Run `fallback_steps` (same default strategy + global timeout) |

`condition` / `foreach` have **no** `on_error`; nested steps still can.

---

## 11. Output

```yaml
output:
  format: markdown
  template: |
    Summary: {{steps.summary.result}}
```

| Case | Final `final_output` |
|------|----------------------|
| Has `output.template` | Rendered template |
| Has `output` but no template | Last step: AI prefers `.result` string, else pretty JSON |
| No `output` | Same (last step) |

`format` (`text` / `markdown`) is a display hint; the executor uses the template string.

On failure, `final_output` from completed steps and an `error` field may still be present.

---

## 12. How to run

### 12.1 Connection window · AI sidebar · Workflows

- List / create / edit / run  
- Execute usually passes the **current connection** as default `connection_id`  
- Results show tables, SQL, timing per step  

### 12.2 Dedicated Workflow window

- Run, history, edit, AI create tabs  
- Prefer YAML editing for complex steps  

### 12.3 AI generation

Chat uses prompt: `resources/prompts/{lang}/workflow_generate.txt`.  
Output must be **snake_case** YAML inside a \`\`\`yaml fence.

### 12.4 MCP

| Tool | Role |
|------|------|
| `list_workflows` | List workflows |
| `run_workflow` | `{ workflow_id, variables?, connection_id? }` → mainly final text |

Resource: `datazen://workflows`.

### 12.5 Related IPC (in-app)

| Command | Use |
|---------|-----|
| `workflow_list` / `workflow_get` | List and detail |
| `workflow_save` / `workflow_delete` | Persist CRUD |
| `workflow_reload` | Rescan directory |
| `workflow_execute` | Execute |
| `workflow_history_*` | History |

---

## 13. Full examples

### 13.1 Single DB + AI summary

See [§3.2](#32-minimal-runnable-example).

### 13.2 Cross-DB orders and logistics

See [§9.2](#92-cross-database-pattern) and `scripts/test-cross-db-workflow.yaml`.

### 13.3 foreach + condition

```yaml
id: notify-large-orders
name: Explain large orders one by one
description: Find large orders; AI one-liner per row
timeout_secs: 120
error_handling:
  strategy: skip

variables:
  - name: min_amount
    type: number
    description: Amount threshold
    required: true
    default: 1000

steps:
  - type: query
    id: large_orders
    sql: |
      SELECT order_id, amount, customer
      FROM orders
      WHERE amount >= {{min_amount}}
      LIMIT 20

  - type: condition
    id: has_rows
    if: "steps.large_orders.rows_count.is_not_empty"
    then_steps:
      - type: foreach
        id: each_order
        items: "steps.large_orders.rows"
        as_var: order
        max_iterations: 20
        steps:
          - type: ai
            id: one_line
            prompt: |
              In one English sentence describe order {{order.order_id}},
              customer {{order.customer}}, amount {{order.amount}}.
            timeout_secs: 45
    else_steps:
      - type: ai
        id: empty_msg
        prompt: "No orders with amount ≥ {{min_amount}}."

output:
  format: text
  template: "{{steps.empty_msg.result}}{{steps.each_order.result}}"
```

Note: `else` and `then` are mutually exclusive; only one side runs. In the `output` template above, the unused side resolves to an empty string.

### 13.4 Step-level fallback

```yaml
id: schema-or-fallback
name: Degrade when table read fails
description: On main query failure, return a hint row
error_handling:
  strategy: abort

steps:
  - type: query
    id: main
    sql: "SELECT COUNT(*) AS cnt FROM maybe_missing_table"
    on_error:
      strategy: fallback
      fallback_steps:
        - type: query
          id: fallback_row
          sql: "SELECT 0 AS cnt, 'table missing' AS note"

output:
  format: text
  template: "count={{steps.main.rows.0.cnt}}{{steps.fallback_row.rows.0.note}}"
```

Note: fallback runs **different step ids**; when `main` fails it may have no success result — prefer referencing the fallback id, or only the fallback step.

Safer output:

```yaml
output:
  template: "{{steps.fallback_row.result}}{{steps.main.result}}"
```

---

## 14. Troubleshooting

| Symptom | Likely cause |
|---------|--------------|
| YAML parse / wrong step type | Used camelCase (`thenSteps`, `timeoutSecs`, `asVar`) |
| `Query step requires a database connection` | No default connection and no `connection` |
| Condition always false | Wrote `{{steps...}} > 0` in `if`; use bare path |
| `IN ()` syntax error | `rows.*` empty; guard with `condition` on `rows_count` |
| foreach Skipped | `items` not an array; check `steps.<id>.rows` |
| Required variable missing | Missing required var and no default |
| Step timed out | Increase step or global `timeout_secs` |
| AI step failed | Provider not configured / unavailable |
| Disk YAML edits ignored | Did not `workflow_reload` / Refresh |
| Wrong DB in cross-DB | Wrong config id on `connection` variable |

---

## 15. Appendix

### 15.1 YAML snake_case ↔ frontend/IPC camelCase

| YAML (disk / execute definition) | TS / some IPC |
|----------------------------------|---------------|
| `timeout_secs` | `timeoutSecs` |
| `then_steps` | `thenSteps` |
| `else_steps` | `elseSteps` |
| `as_var` | `asVar` |
| `max_iterations` | `maxIterations` |
| `error_handling` | `errorHandling` |
| `on_error` | `onError` |
| `fallback_steps` | `fallbackSteps` |
| `rows_count` (result field) | same in JSON result |

**Hand-written and AI-generated YAML must use the left column only.**

### 15.2 Step result cheat sheet

| Step | Main context fields |
|------|---------------------|
| query | `rows`, `rows_count`, `columns`, `execution_time_ms`; `result` path falls back to `rows` |
| ai | `result` (string) |
| condition | Record includes boolean `condition`; rarely templated |
| foreach | `iterations_completed`, `iterations` |

### 15.3 Defaults cheat sheet

| Item | Default |
|------|---------|
| Global timeout | 300s |
| query/ai timeout | 30s |
| Error strategy | abort |
| foreach `max_iterations` | 100 |
| `variables` | `[]` |

### 15.4 Related source and tests

| Path | Contents |
|------|----------|
| `src-tauri/src/workflow/workflows.rs` | Model, execute, templates, conditions |
| `scripts/test-cross-db-workflow.yaml` | Cross-DB sample |
| `src-tauri/tests/workflow_tests.rs` | Integration tests |
| `src-tauri/resources/prompts/*/workflow_generate.txt` | AI generation rules |
| `e2e/specs/workflow*.ts` | UI E2E |

### 15.5 Corrections vs older docs

If you saw older samples, prefer this:

- Fields must be **snake_case** (`then_steps`, not `thenSteps`)  
- Conditions: `steps.id.rows_count > 0` — do not wrap the whole left side in `{{ }}`  
- Query structured result is `rows` / `rows_count`; `{{steps.id.result}}` on query is a compat fallback, not a separate field  
- Prefer `foreach.items: steps.id.rows` deep paths  

---

*Kept in sync with the DataZen Workflow implementation; if behavior conflicts with this doc, `workflows.rs` wins.*
