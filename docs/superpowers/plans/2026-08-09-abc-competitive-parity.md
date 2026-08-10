# A→B→C Competitive Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Close the highest-ROI gaps vs DBX in order: distribution/trust (A) → daily SQL UX (B) → differentiation packaging (C), without JDBC breadth or Docker/Web.

**Architecture:** Extend existing MCP settings/permission model, reuse sync schema-diff IPC for a standalone window, ship builtin workflow YAML + Ollama as a thin OpenAI-compatible provider, and auto-switch chartable query results.

**Tech Stack:** Tauri 2, React 18, Zustand, Vitest, Rust (`mcp/`, `workflow/`, `ai/`), existing `importData` / `compare_table_schemas`.

**Worktree:** `.worktrees/feat-abc-competitive` · branch `feat/abc-competitive-parity`

---

## File map

| Area | Create | Modify |
|------|--------|--------|
| A allowlist | `src-tauri/src/mcp/allowlist.rs` | `permission` wiring, `db_tools`, `server`, store, settings UI, types |
| A agent config | `src/lib/mcpAgentConfig.ts` (+ test) | `SettingsWindow.tsx`, locales |
| A docs | `docs/packaging.md` | `competitive-comparison-dbx.md`, `README.md`, `docs/architecture/backend/mcp.md` |
| B object search | — | `StandardSchemaTree.tsx`, `MultiDatabaseSchemaTree.tsx`, locales |
| B schema diff | `src/windows/schema-diff/*`, extract panel | `windowKind.ts`, `App.tsx`, menu/locales |
| C workflows | `src-tauri/resources/builtin-workflows/*.yaml` | `workflows.rs`, `WorkflowWindow.tsx` |
| C chart | — | `queryStore.ts`, `QueryPanel.tsx`, settings |
| C Ollama | `src-tauri/src/ai/ollama.rs` | `ai-api` types, registry, `aiProviders.ts`, settings |

---

### Task A1: MCP connection allowlist (backend)

**Files:**
- Create: `src-tauri/src/mcp/allowlist.rs`
- Modify: `src-tauri/src/mcp/mod.rs`, `store/mod.rs`, `services/db_tools.rs`, `mcp/server.rs`, `types` / settings serde

- [ ] Write unit tests: empty allowlist = all allowed; non-empty filters list + rejects resolve
- [ ] Implement `is_connection_allowed` + wire into list/resolve/resource
- [ ] Persist `mcp_allowed_connection_ids: Vec<String>` (empty = unrestricted)
- [ ] `cargo test -p datazen --lib mcp::`

### Task A2: MCP allowlist + agent snippets (frontend)

**Files:**
- Create: `src/lib/mcpAgentConfig.ts`, `src/lib/__tests__/mcpAgentConfig.test.ts`
- Modify: `SettingsWindow.tsx`, `settingsStore.ts`, `src/types/index.ts`, `en.ts`, `zh-CN.ts`

- [ ] Multi-select saved connections; empty = all
- [ ] Cursor / Claude Desktop JSON snippets + Copy
- [ ] `npx vitest run src/lib/__tests__/mcpAgentConfig.test.ts`

### Task A3: Packaging + competitive narrative

- [ ] Add `docs/packaging.md` checklist
- [ ] Refresh `docs/competitive-comparison-dbx.md` (updater/MCP/import/export/Excel)
- [ ] README Features: MCP / Workflow / trust bullets
- [ ] Commit Path A

### Task B1: Column-aware object search

- [ ] When search ≥ 2 chars, match column names via cached/`get_columns` for loaded tables
- [ ] Highlight / expand matching tables; i18n hint
- [ ] Unit or component-level test if cheap; else manual note

### Task B2: Standalone Schema Diff window

- [ ] Extract/reuse diff panel from DataSync
- [ ] Register `schema-diff` window kind; open from menu or connection tools
- [ ] Commit Path B

### Task C1: Builtin workflow templates

- [ ] 3 YAML under `resources/builtin-workflows/`
- [ ] Seed into user workflows on empty registry
- [ ] Templates UI in WorkflowWindow

### Task C2: Auto-chart + Ollama

- [ ] Setting `autoChartOnQuery` + post-run switch when chartable
- [ ] Ollama provider (OpenAI-compatible) + settings row + `/api/tags`
- [ ] Commit Path C; run `pnpm test:unit` focused + `cargo test -p datazen --lib`

---

## Out of scope

- Publishing live Homebrew tap / winget-pkgs PR
- JDBC / Docker Web / full CLI package
- Field lineage engine
- Theme store/CDN
