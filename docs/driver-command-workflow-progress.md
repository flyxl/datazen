# Driver Command / Workflow Progress

## Scope

This document tracks the migration of DataZen from Driver-specific/query-oriented workflow operations to the unified Driver Command API.

## Completed

- [x] Added `DriverCommandDefinition` and `CommandResult` to `packages/driver-api`.
- [x] Added `DatabaseDriver::command_definitions()` with compatibility definitions for `query` and `execute`.
- [x] Added `DatabaseDriver::execute_command()` with the legacy `query`/`execute` bridge.
- [x] Added command forwarding to `ReuseDriver`.
- [x] Added Driver command discovery and execution IPC handlers.
- [x] Kept the IPC layer thin: resolve Connection, validate the declared command/input, then delegate to the Driver API.
- [x] Added frontend Driver Command IPC wrappers.
- [x] Added Workflow Command steps and generic command runtime support.
- [x] Added workflow-level default Connection with per-step Connection override/inheritance.
- [x] Added legacy `query` → `command("query")` compatibility without changing legacy YAML on disk.
- [x] Added schema-driven Command input UI and Connection-based command discovery.
- [x] Split the large workflow implementation into focused model, registry, context, conditions, executor, command, and command-runtime modules while retaining the historical facade.
- [x] Updated `AGENTS.md` and workflow/system architecture documentation.
- [x] Updated WorkflowForm tests after introducing workflow-level Connection.
- [x] Fixed Workflow model/facade visibility and equality derives required by the Rust compiler.
- [x] Fixed Driver Command IPC to use the existing structured `CommandError::Validation` path.
- [x] Fixed basic-CI sync adapter test linking so unselected optional driver crates are not referenced.
- [x] Added this progress document.

## CI Status

Rust lib tests compile on this branch after restoring the workflow facade `model` re-export and filling `WorkflowDefinition.connection` in AI integration tests.

## Remaining Feature Work

### P0 — Driver-specific command migration

- [x] Registered generic Driver Command IPC (`get_connection_commands`, `get_driver_commands`, `execute_driver_command`).
- [x] Resolved Workflow/IPC Connection ids through `resolve_session` so config ids inherit/connect correctly.
- [x] Migrated Redis plugin operations onto `command_definitions()` / `execute_command()`. Redis UI and Pub/Sub subscribe/unsubscribe use `execute_driver_command`; the Redis Tauri plugin is setup-only (Pub/Sub event sink).
- [x] Non-SQL path drivers override command discovery: MongoDB/Elasticsearch/InfluxDB keep query+execute with language-specific `sql` titles; HBase/Vector/VictoriaMetrics expose query only. SQL path drivers keep the default pair.
- [x] `execute_driver_command` accepts `connectionId` or `driverType`. Commands with `metadata.requiresConnection = false` run against the registry Driver instance without a live session.
- [x] Kiwi `login` / `list_instances` are unbound Driver Commands; the Kiwi Tauri plugin is setup-only.

### P1 — Permission enforcement

- [x] `DriverCommandDefinition.permissions` are classified into Read / Write / HighRisk and enforced before `execute_command()` for Workflow and IPC callers that supply an MCP permission mode.
- [x] GUI `execute_driver_command` stays unrestricted for discovered commands (desktop operator). Read-only / safe-write modes deny write and high-risk commands.

### P1 — Integration coverage

- [x] Command discovery from a Connection.
- [x] Command input validation.
- [x] Driver command execution through IPC.
- [x] Workflow Command execution and Connection inheritance.
- [x] Legacy Query loading/execution through the Command runtime.
- [x] E2E spec `e2e/specs/driver-commands.ts` covers discovery + query execution + unsupported command.

### P2 — Command metadata

- [x] `DriverCommandDefinition.metadata` carries `category`, optional `risk`, `workflow`/`ui` visibility, `deprecated`/`replacedBy`, and `requiresConnection`.
- [x] Explicit `metadata.risk` overrides permission-token classification.
- [x] Workflow UI hides `workflow: false` and deprecated commands; runtime rejects `workflow: false`.
- [x] Redis Pub/Sub subscribe/unsubscribe are `pubSub` and hidden from workflows.

## Branch Discipline

All implementation work for this migration is performed on:

`agent/driver-command-workflow`

`main` must not receive direct changes as part of this work.
