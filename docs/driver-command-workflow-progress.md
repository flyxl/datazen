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

The original failing CI run was `31474270276` / job `93724251779`.

A follow-up run is `31475495899` / job `93728124924` for the latest fixes on `agent/driver-command-workflow`.

The follow-up run has already passed:

- frontend unit tests;
- Site SEO/i18n checks;
- Rust toolchain setup;
- Linux Tauri/WebKit dependency setup.

It is currently in the Rust compilation/cache stage. The original Rust compile errors have been addressed in the source files except for three test struct literals in `src-tauri/src/commands/ai_integration_tests.rs`, which need `connection: None` because `WorkflowDefinition` now has the optional workflow-level Connection field. Those three initializers are in:

- `workflow_save_list_get_delete`;
- `workflow_execute_ai_step_with_wiremock`;
- `workflow_history_clear_after_execute`.

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
