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

## Current CI Fixes

CI run `31474270276`, job `93724251779`, reaches the Rust test build after all 121 frontend test files / 940 frontend tests pass.

The Rust build currently reports these groups of errors:

1. Workflow facade re-export visibility (`E0365`) because the extracted model/registry modules were `pub(crate)` while `workflows.rs` preserved public historical exports.
2. `CommandError::Message` no longer exists; command validation should use the existing structured `Validation` variant.
3. Three Rust integration-test `WorkflowDefinition` struct literals need the new optional `connection` field.
4. `WorkflowCommandStep` derives `PartialEq`, so `ErrorHandlingConfig` and its nested `WorkflowStep` need compatible `PartialEq` implementations.
5. `adapter_registry` test-only force-linking references every optional driver crate even though the CI `basic` driver set only links PostgreSQL/MySQL/SQLite/Redis.

The first, second and fourth groups have been fixed on `agent/driver-command-workflow`. The remaining work is to update the three test initializers and make the optional-driver force-linking compatible with the selected driver feature set, then rerun CI.

## Remaining Feature Work

### P0 — Driver-specific command migration

Inventory existing Driver-specific IPC/custom operations and migrate each operation to:

```text
command_definitions()
        ↓
execute_command()
```

The migration should remove duplicate Driver-specific IPC dispatch rather than creating a second command system.

### P1 — Permission enforcement

`DriverCommandDefinition.permissions` is now part of the manifest and is carried through IPC, but the current implementation only exposes the declaration. A real permission policy/enforcement layer still needs to consume it.

### P1 — Integration coverage

Add end-to-end tests covering:

- command discovery from a Connection;
- command input validation;
- Driver command execution through IPC;
- Workflow Command execution and Connection inheritance;
- legacy Query loading/execution through the Command runtime.

### P2 — Command metadata

Consider adding optional metadata such as command category, risk level, UI/workflow support, and deprecation status once concrete Driver migrations demonstrate the need.

## Branch Discipline

All implementation work for this migration is performed on:

`agent/driver-command-workflow`

`main` must not receive direct changes as part of this work.
