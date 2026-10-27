# External Contract Policy

DataZen exposes several **external contracts** consumed by MCP clients, database driver extensions, AI provider extensions, GUI extensions, and automation scripts. This document defines what counts as a breaking change, how deprecations work before and after v1.0, and what contributors must verify before merging.

Related docs:

- Driver API boundary: [`driver-api-dependency-boundary.md`](driver-api-dependency-boundary.md)
- MCP architecture: [`../architecture/backend/mcp.md`](../architecture/backend/mcp.md)
- Naming (`connectionId` vs `connection_id`): [`../architecture/naming.md`](../architecture/naming.md)

## Contract surfaces

| Surface | Consumers | Stability expectation |
|---------|-----------|----------------------|
| **MCP tools** (`list_tools` / `call_tool`) | Headless MCP clients, external agents | Tool **names** and **input JSON keys** are stable; see golden test |
| **MCP resources** (`datazen://…` URIs) | MCP clients reading connections, history, schema | URI paths and serialized JSON field names are stable |
| **MCP prompts** | MCP clients using NL2SQL / diagnose helpers | Prompt names and argument keys are stable |
| **Tauri IPC commands** | React frontend, E2E, extensions bridge | Command names and request/response shapes used by the Host UI |
| **`PROTOCOL_VERSION`** (`packages/driver-api`) | Path and Git database drivers | Bump only with migration notes; drivers must recompile |
| **`AI_PROTOCOL_VERSION`** (`packages/ai-api`) | AI provider extensions | Same as driver protocol |
| **Persisted store JSON** | Upgrades across app versions | Field renames require migration or dual-read period |

Internal refactors (file splits, private helper moves) are **not** contract changes as long as observable behavior and the surfaces above stay the same.

## Breaking vs non-breaking

**Breaking** (requires deprecation process below):

- Renaming or removing an MCP tool, resource URI, or prompt
- Renaming or removing a required MCP tool input property (e.g. `connection_id` → `connectionId`)
- Removing support for a previously accepted input alias (e.g. re-allowing removed `config_id`)
- Changing serialized JSON field names in MCP resources or IPC responses (`connectionId` ↔ `configId`)
- Raising minimum `PROTOCOL_VERSION` / `AI_PROTOCOL_VERSION` without a compatibility shim
- Removing or renaming a Tauri IPC command used by the Host frontend

**Non-breaking** (release notes optional):

- Adding a new optional MCP tool input property
- Adding a new MCP tool, resource, or prompt
- Adding new optional fields to IPC responses
- Internal module layout changes with unchanged behavior
- Bug fixes that align implementation with documented contract

## Deprecation policy

DataZen is currently **v0.x** (see `src-tauri/Cargo.toml`). Policy tightens as we approach **v1.0**.

### v0.x (current: pre-1.0)

- Breaking contract changes are **discouraged** but allowed when necessary for correctness or security.
- When breaking a surface:
  1. Document in the PR and CHANGELOG / release notes.
  2. Prefer **one minor release** of backward compatibility (dual-read old + new names, or tool alias) when cost is low.
  3. Update golden / contract tests in the same PR.
- Silent renames without notice are **not** acceptable.

### Approaching v1.0 (≥ 0.9.x)

- Treat MCP tool names, input keys, and resource URIs as **public API**.
- Breaking changes require:
  - **Deprecation window**: at least **two minor releases** or **90 days**, whichever is longer.
  - Runtime warning or documented migration path.
  - Updated golden tests only after the removal release (or dual-read period covered by tests).

### After v1.0

- Follow [Semantic Versioning](https://semver.org/): breaking external contract changes bump **major**.
- MCP and IPC breaking changes require major version + migration guide.

## MCP-specific rules

### Tool names

- Use `snake_case` (e.g. `list_connections`, `search_tables`).
- Do **not** rename without an alias period: old name should remain callable or return a clear error pointing to the replacement.
- The canonical tool list is guarded by `MCP_ALL_TOOLS` and the golden fixture `src-tauri/src/mcp/fixtures/mcp_external_contract.json`.

### Tool input arguments

- JSON keys use **`snake_case`** (`connection_id`, `workflow_id`), matching Rust `Deserialize` structs in `mcp/types.rs`.
- **Persistent connection identity** is always `connection_id`. The legacy `config_id` field is removed and must not return.
- Optional fields may be added; required fields must not be renamed without deprecation.
- When changing schemas, update `mcp/tool_help.rs` examples and the golden contract test.

### Resources

- Fixed URIs: `datazen://connections`, `datazen://query-history`, `datazen://workflows`.
- Template: `datazen://schema/{connectionId}/{database}` (path segments URL-encoded).
- Serialized resource bodies use **camelCase** for persisted-id fields (e.g. `connectionId` in query history), consistent with frontend IPC.

### Prompts

- Prompt handlers are part of the MCP contract; treat renames like tool renames.

### Stateful session tools

`open_session` / `use_session` / `close_session` are a stateful trio; the rest of the tool list is stateless.

- A stateful tool takes an **opaque `clientSessionRef`**. The reference must stay unresolvable from the client side: clients may not receive `db_session_id`, connection internals, or any other handle they could pass to a different entry point.
- The **consumer identity behind the reference is generated by the backend** (one per MCP server instance) and must never be accepted from a tool input. Ownership is decided by that stored identity, not by anything the caller asserts.
- The `reference -> handle` mapping lives **in the MCP adapter memory only**. It must not be written into workflow definitions, execution history, or database configuration; a lost mapping is reported as lost and is never rebuilt under the same reference.
- `use_session` on a reference the caller does not own (or one that is closed or expired) must report the same `SessionLost` outcome as an unknown reference. `close_session` must be idempotent and must not distinguish "never existed" from "no longer exists".
- Making a stateful tool does not exempt it from the existing gates: connection allowlist, tool permission mode, and the SQL guard still apply to the commands run inside a session.
- Behavioral changes here (TTL bounds, close modes, row caps, error wording) are contract changes: update `mcp.md` §1.4, `mcp/tool_help.rs`, and the tests below in the same PR.

## Driver and AI protocol versions

- **`PROTOCOL_VERSION`** and **`AI_PROTOCOL_VERSION`** are independent of the app semver.
- Increment only when public trait signatures or wire shapes change in `packages/driver-api` or `packages/ai-api`.
- When bumping, update all in-tree path drivers and document Git driver minimum host version in release notes.
- See [`driver-api-dependency-boundary.md`](driver-api-dependency-boundary.md) for API exposure rules.

## Regression tests

Contract drift must be caught in CI:

| Area | Test location |
|------|----------------|
| MCP tool names, inputs, resources | `src-tauri/src/mcp/contract.rs` + golden JSON |
| MCP input rejects removed fields | `mcp/server.rs` (`config_id` test) |
| MCP stateful session ownership, TTL, isolation, idempotent close | `mcp/server/tests_stateful.rs` |
| MCP session registry unit behavior (sanitize_ttl, lookup, close_client) | `mcp/session.rs` |
| Host E2E IPC journeys | `e2e/contract/` (separate matrix) |

If you change any row in **Breaking vs non-breaking**, update tests in the same PR.

## Contributor checklist

Before merging a PR that touches MCP, IPC, driver-api, ai-api, or persisted JSON:

- [ ] I classified the change as breaking or non-breaking using this document.
- [ ] Breaking changes include release notes and, when feasible, a deprecation / dual-read period.
- [ ] MCP tool or input changes update `mcp/fixtures/mcp_external_contract.json` (or justify why the golden test is unchanged).
- [ ] Removed JSON keys stay removed (no accidental revival of `configId` / `config_id`).
- [ ] Stateful session tools keep `clientSessionRef` opaque, keep consumer identity backend-generated, and keep the mapping out of every persisted store.
- [ ] `cargo test -p datazen_desktop --lib` passes (includes MCP contract tests).

See also the PR template checklist for the same items.
