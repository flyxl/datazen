# DataZen Driver Command & Workflow Connection Context

## 1. Overview

DataZen currently has several partially independent concepts:

- Tauri IPC commands.
- Driver-level operations such as `query`, `query_multi`, and `execute`.
- Workflow-level `Query` steps.
- Driver-specific/custom commands.
- Driver permission declarations.

These concepts need to converge into a unified Driver Command model.

The target architecture introduces three distinct layers:

```text
Tauri IPC
    │
    ▼
Application Command
    │
    ▼
Driver Command
    │
    ▼
Driver Implementation
```

and:

```text
Workflow Command Step
    │
    ▼
Driver Command
    │
    ▼
Driver Implementation
```

The Driver Command is the common runtime abstraction.

At the Workflow level, connection selection is represented as an execution context rather than something every Step must explicitly configure.

The Workflow provides a default Connection. Data-operation Steps inherit that Connection unless they explicitly override it.

## 2. Goals

1. Introduce a unified Driver Command abstraction.
2. Migrate Workflow `Query` execution to Driver Command.
3. Preserve backward compatibility with existing Workflow configuration files.
4. Allow Drivers to expose custom commands.
5. Allow Workflow to execute Driver-specific commands.
6. Allow frontend to dynamically discover commands supported by the current Driver.
7. Use command metadata to generate Workflow command configuration UI.
8. Reuse the existing Driver permission declaration mechanism.
9. Provide a generic IPC entry point for Driver Command execution.
10. Avoid Driver-specific logic inside the Workflow engine.
11. Avoid requiring every Step to explicitly select a Connection.
12. Support one Workflow containing multiple Connections.
13. Preserve existing Query behavior and result semantics.

## 3. Non-Goals

This change does not:

1. Redesign the entire Workflow engine.
2. Remove all existing Driver APIs immediately.
3. Remove legacy `type: query` configuration.
4. Require every Driver to implement custom commands immediately.
5. Introduce a separate permission system.
6. Require users to manually migrate existing Workflow files.
7. Make every Workflow Step connection-aware.
8. Make Control Flow Steps depend on database Drivers.

## 4. Core Concepts

### 4.1 Driver

A Driver represents a database or data-source implementation.

Examples:

```text
MySQL
PostgreSQL
MongoDB
Redis
Elasticsearch
Trino
Hive
```

The Driver owns connection handling, command implementation, command metadata, command input validation, command output, and command permissions.

## 5. Driver Command

A Driver Command represents an operation supported by a Driver.

Examples:

```text
MySQL
    query
    execute

MongoDB
    find
    aggregate
    insert
    update

Redis
    get
    set
    scan
    del

Elasticsearch
    search
    index
    update
    delete
```

The Workflow engine must not need to know these Driver-specific operations.

## 6. Command Definition

A Driver exposes command metadata through a manifest.

Conceptually:

```rust
pub struct DriverCommandDefinition {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
    pub output_schema: Option<Value>,
    pub permissions: Vec<String>,
}
```

The actual implementation should reuse existing DataZen schema and permission types whenever possible.

The command definition is used for command discovery, Workflow UI, input validation, dynamic form generation, permission checking, and future documentation/tooling.

## 7. Command Manifest

The Driver Manifest is the source of truth for Driver capabilities.

Example:

```json
{
  "driver": "mongodb",
  "commands": [
    {
      "id": "find",
      "name": "Find",
      "description": "Find documents",
      "input_schema": {},
      "permissions": []
    },
    {
      "id": "aggregate",
      "name": "Aggregate",
      "description": "Run aggregation pipeline",
      "input_schema": {},
      "permissions": []
    }
  ]
}
```

The frontend and Workflow engine must not hard-code Driver-specific command lists.

## 8. Command Execution API

The Driver exposes a unified execution API:

```rust
async fn execute_command(
    &self,
    handle: &ConnectionHandle,
    command: &str,
    input: Value,
) -> Result<CommandResult, DriverError>;
```

This becomes the common runtime entry point for Workflow, Tauri IPC, and future integrations.

The Driver owns command dispatch.

The generic Workflow engine must not contain Driver-specific command logic.

## 9. Query as a Command

The existing Query operation becomes the standard Driver Command:

```text
command = "query"
```

For SQL Drivers:

```json
{
  "sql": "SELECT * FROM users"
}
```

The existing query implementation may continue to be used internally:

```text
execute_command("query", input)
        │
        ▼
existing query/query_multi implementation
```

This allows the migration to happen without rewriting query execution.

## 10. Workflow Connection Context

A Workflow has an optional default Connection.

Example:

```yaml
connection: mysql-prod

steps:
  ...
```

This Connection acts as the default execution context for Data-operation Steps.

The Workflow does not require every Step to explicitly specify a Connection.

## 11. Connection Inheritance

A Step without an explicit Connection inherits the current/default Connection.

Example:

```yaml
connection: mysql-prod

steps:
  - type: command
    command: query

  - type: command
    command: query

  - type: command
    command: execute
```

All three Commands execute against `mysql-prod`.

## 12. Step Connection Override

A Data-operation Step may explicitly override the Workflow Connection.

Example:

```yaml
connection: mysql-prod

steps:
  - type: command
    command: query

  - type: command
    connection: mongodb-prod
    command: aggregate

  - type: command
    command: query
```

The effective connections are:

```text
Step 1 → mysql-prod
Step 2 → mongodb-prod
Step 3 → mysql-prod
```

This allows a Workflow to operate across multiple Drivers.

## 13. Connection Resolution

The effective Connection for a Data-operation Step is determined in this order:

```text
Step Connection
      │
      ├── exists → use it
      │
      └── absent
            │
            ▼
      Workflow Default Connection
            │
            ├── exists → use it
            │
            └── absent → connection required error
```

Conceptually:

```rust
fn resolve_connection(
    workflow: &Workflow,
    step: &WorkflowStep,
) -> Result<ConnectionId, WorkflowError>
```

## 14. Control Flow Steps

Not every Step requires a Connection.

Examples:

```text
Condition
If/Else
ForEach
Loop
Variable
Transform
Delay
```

These Steps must not be required to specify a Connection.

Connection is an execution context for Data-operation Steps, not a universal property of all Workflow Steps.

## 15. Command Step

The canonical Data-operation Step is:

```yaml
type: command
command: query
```

A complete example:

```yaml
steps:
  - id: query_users
    type: command
    command: query
    input:
      sql: |
        SELECT id, name
        FROM users
```

If the Workflow has `connection: mysql-prod`, the Step automatically uses it.

## 16. Command Discovery

The frontend must be able to query the commands supported by a Connection.

The preferred API is:

```text
get_connection_commands(connection_id)
```

rather than `get_driver_commands(driver_type)` because the actual Connection may determine Driver version, server capabilities, connection-specific capabilities, permissions, and feature availability.

The conceptual flow is:

```text
Connection ID
      │
      ▼
ConnectionManager
      │
      ▼
Driver
      │
      ▼
Command Manifest
```

## 17. Workflow UI Command Discovery

When creating a Command Step:

```text
Current Step
    │
    ▼
Resolve effective Connection
    │
    ▼
Resolve Driver
    │
    ▼
Load Driver Commands
    │
    ▼
Display Command Selector
```

The user should not have to select a Connection before seeing available Commands if the current Workflow context already provides one.

## 18. Command UI

Example:

```text
┌─────────────────────────────────────┐
│ Command                             │
├─────────────────────────────────────┤
│ Connection                          │
│ ↳ MySQL Production                  │
│                                     │
│ Command                             │
│ [ Query                         ▼ ] │
│                                     │
│ SQL                                 │
│ ┌─────────────────────────────────┐ │
│ │ SELECT * FROM users             │ │
│ └─────────────────────────────────┘ │
└─────────────────────────────────────┘
```

For MongoDB:

```text
┌─────────────────────────────────────┐
│ Command                             │
├─────────────────────────────────────┤
│ Connection                          │
│ ↳ MongoDB Production                │
│                                     │
│ Command                             │
│ [ Aggregate                     ▼ ] │
│                                     │
│ Collection                          │
│ [ orders                         ]  │
│                                     │
│ Pipeline                            │
│ [ ...                            ]  │
└─────────────────────────────────────┘
```

The UI is generated from the Driver Command definition.

## 19. Connection Override UI

The default case should be visually lightweight.

Example:

```text
Connection
↳ MySQL Production
```

The user can explicitly change it:

```text
Connection
[ MySQL Production ▼ ]
```

When the Connection changes, the Command list must refresh automatically.

## 20. Command Input Schema

The `input_schema` in the Command Definition is used to build the Command configuration UI.

Example:

```json
{
  "type": "object",
  "properties": {
    "collection": {
      "type": "string"
    },
    "pipeline": {
      "type": "array"
    }
  },
  "required": [
    "collection",
    "pipeline"
  ]
}
```

The frontend may generate Collection and Pipeline editors without containing MongoDB-specific UI logic.

## 21. Legacy Query Compatibility

Existing Workflow configurations must remain valid.

Existing:

```yaml
type: query
connection: mysql-prod
sql: |
  SELECT *
  FROM users
```

must continue to work.

Internally it is normalized into:

```text
Command {
    command: "query",
    connection: mysql-prod,
    input: {
        sql: "SELECT ..."
    }
}
```

The old configuration format does not need to be rewritten automatically.

## 22. Legacy Query Execution

The legacy Query representation must not maintain an independent execution implementation.

The runtime flow becomes:

```text
Legacy Query
      │
      ▼
Normalize
      │
      ▼
Command("query")
      │
      ▼
Driver::execute_command()
```

This ensures Query and custom Driver Commands share exactly the same execution path.

## 23. Workflow Context

The Workflow execution context should be able to resolve the current Connection.

Conceptually:

```rust
struct WorkflowExecutionContext {
    variables: ...,
    connection_context: ConnectionContext,
}
```

The Connection Context may contain:

```rust
struct ConnectionContext {
    default: Option<ConnectionId>,
}
```

Step-level Connection overrides remain part of the Data-operation Step configuration.

The implementation should allow future expansion to nested scopes without requiring a new Workflow execution architecture.

## 24. IPC Integration

Introduce:

```text
execute_driver_command
```

Input:

```json
{
  "connectionId": "mongodb-prod",
  "command": "aggregate",
  "input": {}
}
```

The IPC implementation:

1. Resolves Connection.
2. Resolves Driver.
3. Resolves Command Definition.
4. Validates input.
5. Checks permission.
6. Calls `Driver::execute_command()`.
7. Returns CommandResult.

The IPC layer must not implement Driver-specific behavior.

## 25. Existing IPC Compatibility

Existing IPC commands such as Query execution must remain compatible.

For example, `execute_query` may internally become `Driver::execute_command("query")`.

This allows existing frontend code to continue functioning.

## 26. Permission Model

Existing Driver permission declarations remain the source of permission information.

Command definitions may reference the corresponding permissions:

```json
{
  "id": "aggregate",
  "permissions": [
    "mongodb.aggregate"
  ]
}
```

Permission checking must happen before command execution.

No second independent permission framework should be introduced.

## 27. Error Model

Command execution must distinguish at least:

```text
UnsupportedCommand
InvalidCommandInput
PermissionDenied
ConnectionError
DriverExecutionError
```

Existing DataZen Driver error infrastructure should be reused.

## 28. Multi-Connection Workflow

The architecture must support:

```text
Workflow
│
├── MySQL
│   ├── Query
│   └── Query
│
├── Transform
│
└── MongoDB
    ├── Aggregate
    └── Insert
```

The user should only need to explicitly configure the Connection when switching away from the inherited/default Connection.

## 29. UI/UX Principles

1. Do not force every Step to select Connection.
2. Use Workflow Connection as the default context.
3. Show inherited Connection as lightweight metadata.
4. Allow explicit Connection override.
5. Refresh Commands when effective Connection changes.
6. Never hard-code Driver-specific Command lists.
7. Generate Command input UI from schema where possible.
8. Hide Connection controls for Control Flow Steps that do not require one.
9. Make cross-Driver operations explicit but lightweight.

## 30. Target Architecture

```text
                         Workflow
                            │
                   default Connection
                            │
                            ▼
                    Execution Context
                            │
                  ┌─────────┴─────────┐
                  │                   │
             inherited           Step override
                  │                   │
                  └─────────┬─────────┘
                            ▼
                        Connection
                            │
                            ▼
                          Driver
                            │
                 ┌──────────┴──────────┐
                 │                     │
          Command Manifest      execute_command()
                 │                     │
                 ▼                     ▼
           Command Selector       Driver Runtime
                 │
                 ▼
            Input Schema
                 │
                 ▼
             Command Step
```

## 31. Acceptance Criteria

- [ ] Driver exposes a unified Command execution API.
- [ ] Driver exposes Command definitions.
- [ ] Manifest describes Command capabilities.
- [ ] Existing SQL Query is available as `query` Command.
- [ ] Workflow supports generic Command Step.
- [ ] Workflow has a default Connection.
- [ ] Data-operation Steps inherit the default Connection.
- [ ] Steps can override Connection.
- [ ] Control Flow Steps do not require Connection.
- [ ] Commands are discovered dynamically from effective Connection.
- [ ] Command lists are not hard-coded in Workflow UI.
- [ ] Command input UI can be generated from Command schema.
- [ ] Existing `type: query` configuration remains compatible.
- [ ] Legacy Query is normalized into Command internally.
- [ ] Legacy Query and new Command produce equivalent results.
- [ ] Generic Driver Command IPC exists.
- [ ] Existing Query IPC remains compatible.
- [ ] Existing permission declarations continue to work.
- [ ] Command permissions are enforced.
- [ ] Workflow can execute multiple Drivers.
- [ ] Workflow executor contains no Driver-specific command logic.
- [ ] Existing Query tests remain green.
- [ ] New Command tests are added.
- [ ] Legacy configuration fixtures continue to pass.
