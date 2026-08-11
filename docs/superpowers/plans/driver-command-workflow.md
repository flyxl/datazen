# DataZen Driver Command & Workflow Connection Context — Implementation Plan

## 1. Implementation Principles

The implementation must follow these principles:

1. Introduce the new abstraction incrementally.
2. Keep existing Driver APIs during migration.
3. Keep existing Workflow `Query` configuration readable.
4. Make `Command` the only unified Driver execution path.
5. Keep Connection resolution outside individual Driver implementations.
6. Make Workflow Connection inheritance explicit in runtime semantics.
7. Make Driver Command discovery manifest-driven.
8. Keep Driver-specific logic inside Drivers.
9. Do not make Control Flow Steps connection-dependent.
10. Keep the application buildable after each phase.

Target runtime:

```text
Workflow
    │
    ▼
Resolve effective Connection
    │
    ▼
Resolve Driver
    │
    ▼
Driver::execute_command()
```

Target discovery:

```text
Workflow UI
    │
    ▼
get_connection_commands()
    │
    ▼
Connection
    │
    ▼
Driver Manifest
    │
    ▼
Command Definitions
```

## Phase 1 — Inspect Existing DataZen Architecture

### Task 1.1 — Locate Driver interfaces

Inspect:

- Driver trait
- Driver implementations
- Connection manager
- existing `query`
- existing `query_multi`
- existing `execute`
- custom command implementations

Create an inventory of Driver, Command, Input, Output, Permission, and Caller relationships.

### Task 1.2 — Locate Workflow model

Inspect:

- Workflow definition
- Workflow Step enum/types
- Query Step
- Workflow parser
- Workflow serializer
- Workflow executor
- Workflow context
- expression/template resolver

Identify exactly where `type: query` is parsed and executed.

### Task 1.3 — Locate existing IPC command architecture

Inspect:

- Tauri command registration
- Query IPC
- Driver-specific IPC
- command permission handling
- frontend invoke wrappers

Document which IPC commands currently duplicate Driver functionality.

## Phase 2 — Introduce Driver Command Types

### Task 2.1 — Define CommandResult

Reuse the existing Driver result representation if possible. Avoid creating a second result hierarchy. The result must support existing Query results.

### Task 2.2 — Define DriverCommandDefinition

Add the minimal definition:

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

Reuse existing schema and permission types if they already exist.

### Task 2.3 — Extend Driver trait

Add:

```rust
async fn execute_command(
    &self,
    handle: &ConnectionHandle,
    command: &str,
    input: Value,
) -> Result<CommandResult, DriverError>;
```

Add command discovery:

```rust
fn command_definitions(
    &self,
) -> Vec<DriverCommandDefinition>;
```

If the current Driver architecture requires async discovery, use an async API instead.

### Task 2.4 — Add default unsupported behavior

Existing Drivers must continue compiling. Where appropriate, `execute_command(...)` defaults to `UnsupportedCommand`. This allows Driver migration to happen incrementally.

## Phase 3 — Implement Query Command

### Task 3.1 — Define `query`

Standard command identifier:

```text
query
```

Do not create `sql_query`, `mysql_query`, or `postgres_query` unless existing Driver semantics require them. The command belongs to the Driver.

### Task 3.2 — Bridge to existing implementation

For SQL Drivers:

```text
match command {
    "query" => {
        // Parse input
        // Call existing query implementation
    }

    _ => ...
}
```

Do not rewrite query execution.

### Task 3.3 — Register query command metadata

Example:

```text
query
    name: Query
    input_schema:
        sql: string
```

For Drivers with additional parameters, preserve existing behavior.

### Task 3.4 — Add equivalence tests

Verify existing `query()` and `execute_command("query")` return equivalent results.

Test:

- normal result
- empty result
- large result
- truncation
- SQL error
- connection failure

## Phase 4 — Introduce Workflow Connection Context

### Task 4.1 — Add Workflow default Connection

Extend Workflow with an optional default Connection:

```rust
struct Workflow {
    ...
    connection: Option<ConnectionId>,
}
```

Use the existing naming convention in DataZen if a connection field already exists.

### Task 4.2 — Add Step-level override

Only Data-operation Steps should expose:

```rust
connection: Option<ConnectionId>
```

Do not blindly add Connection to every Step type. If the current Step architecture uses a common base structure, the field may be technically present but must not be semantically required by Control Flow Steps.

### Task 4.3 — Implement resolution

Add:

```rust
resolve_effective_connection(
    workflow,
    step,
    context
)
```

Resolution order:

```text
Step override
    ↓
Workflow default
    ↓
Missing Connection error
```

## Phase 5 — Add Generic Workflow Command Step

### Task 5.1 — Add Command Step representation

Introduce:

```rust
Command {
    id: String,
    connection: Option<ConnectionId>,
    command: String,
    input: Value,
}
```

Adapt this to the existing Workflow type architecture.

### Task 5.2 — Add Command executor

Execution:

```text
Command Step
    │
    ▼
Resolve effective Connection
    │
    ▼
ConnectionManager
    │
    ▼
Driver
    │
    ▼
execute_command()
```

Reuse existing context, variables, expression resolution, template substitution, result storage, and error handling.

### Task 5.3 — Add result propagation

The Command Step must produce a Workflow context result compatible with existing Query behavior so downstream Steps can consume it.

## Phase 6 — Migrate Legacy Query

### Task 6.1 — Preserve old parser

Continue accepting:

```yaml
type: query
```

Do not break old Workflow files.

### Task 6.2 — Normalize Query

Add a normalization layer:

```rust
fn normalize_step(
    step: WorkflowStep
) -> NormalizedWorkflowStep
```

Transform Query into Command(`query`) while preserving connection, SQL, limits, options, step ID, result semantics, and downstream references.

### Task 6.3 — Remove duplicate Query runtime

After normalization is proven:

```text
Query
    ↓
Command
    ↓
execute_command()
```

There should be no separate Workflow Query execution path.

### Task 6.4 — Legacy regression tests

Create fixtures containing real legacy Workflow files. Verify deserialize → normalize → execute → result matches previous behavior.

## Phase 7 — Implement Command Discovery

### Task 7.1 — Add connection-level discovery API

Add:

```text
get_connection_commands(connection_id)
```

The API should:

1. Resolve Connection.
2. Resolve Driver.
3. Obtain Driver Command Definitions.
4. Apply capability/permission filtering if required.
5. Return definitions.

### Task 7.2 — Do not expose Driver implementation details

The frontend should receive `CommandDefinition[]`, not Driver-specific Rust structures.

### Task 7.3 — Add caching where appropriate

Command metadata normally changes less frequently than command execution. A cache may be introduced if needed, but correctness must not depend on stale metadata. If server capabilities influence commands, the cache must support invalidation.

## Phase 8 — Manifest and Permission Integration

### Task 8.1 — Integrate existing build-time permission declarations

Inspect the current build script and generated permission files.

Map existing custom command permissions to `DriverCommandDefinition.permissions`.

Do not create another permission source.

### Task 8.2 — Expose command metadata

Ensure the Manifest contains enough information for Command Discovery, Input Form, Permission, and Documentation.

### Task 8.3 — Permission validation

Before `execute_command()` perform:

```text
Command Definition
    ↓
Permission
    ↓
Execute
```

Add tests for denied commands.

## Phase 9 — Generic IPC

### Task 9.1 — Add `execute_driver_command`

Input:

```json
{
  "connectionId": "...",
  "command": "...",
  "input": {}
}
```

Implementation:

```text
IPC
 ↓
ConnectionManager
 ↓
Driver
 ↓
Command validation
 ↓
Permission
 ↓
execute_command()
```

### Task 9.2 — Preserve existing Query IPC

Existing `execute_query` must remain functional. Internally it may delegate to `execute_command("query")` to avoid breaking existing frontend code.

### Task 9.3 — Migrate custom IPC commands

For existing Driver-specific IPC commands, route runtime behavior through `Driver::execute_command()`. Old IPC wrappers can remain temporarily for compatibility.

## Phase 10 — Workflow UI

### Task 10.1 — Workflow-level Connection selector

Provide a default Connection at Workflow level:

```text
Workflow
Connection: [ MySQL Production ▼ ]
```

### Task 10.2 — Add Command Step

When the user adds Command, resolve the effective Connection automatically. No Connection selection is required if one is already available.

### Task 10.3 — Dynamic Command selector

Call:

```text
get_connection_commands(connection_id)
```

and populate the Command selector from the returned definitions.

### Task 10.4 — Dynamic Input Form

When the user selects a Command, read its `input_schema` and generate the appropriate editor. The Workflow UI must not contain Driver-specific command branching.

### Task 10.5 — Connection Override

The Command Step should display the effective Connection. By default it is inherited; the user can explicitly override it. When changed, Connection → Driver → Commands → Input Schema must refresh.

## Phase 11 — Control Flow UI

Verify that Steps such as Condition, ForEach, Loop, Variable, and Transform do not require Connection selection. Their UI should not show unnecessary Connection controls.

## Phase 12 — Cross-Driver Workflow

Create an integration test Workflow:

```text
Workflow
  default = MySQL
       │
       ▼
Query MySQL
       │
       ▼
Transform
       │
       ▼
Command
connection = MongoDB
command = insert
       │
       ▼
Condition
       │
       ▼
Command
connection = MySQL
command = query
```

Verify inheritance, override, switching back to default, context propagation, and command discovery.

## Phase 13 — Driver-Specific Command Migration

For every existing custom Driver command:

### Step 13.1

Identify Command ID, Input, Output, Permission, and Implementation.

### Step 13.2

Move runtime implementation behind `Driver::execute_command()`.

### Step 13.3

Add `DriverCommandDefinition`.

### Step 13.4

Expose through Manifest.

### Step 13.5

Add Workflow integration test.

## Phase 14 — Validation

### Driver tests

```text
command discovery
command execution
query
custom command
invalid input
unsupported command
permission denied
```

### Workflow tests

```text
default connection
step override
missing connection
legacy query
query normalization
command
cross-driver workflow
context propagation
```

### IPC tests

```text
execute_driver_command
execute_query compatibility
permission failure
unsupported command
```

### UI tests

```text
Connection → Driver → Command list
Connection change → command refresh
Command selection → schema form
Inherited Connection
Connection override
Control Flow without Connection
```

## Phase 15 — Remove Duplicated Architecture

After all migration tests pass, review:

```text
query()
query_multi()
execute()
custom IPC commands
```

Determine which APIs are still required internally.

The desired final architecture is:

```text
                    ┌───────────────┐
                    │   Workflow    │
                    └───────┬───────┘
                            │
                    Command Step
                            │
                            ▼
                   resolve connection
                            │
                            ▼
                         Driver
                            │
                            ▼
                 execute_command()
                            ▲
                            │
                    ┌───────┴───────┐
                    │               │
                  IPC             Other
```

Low-level APIs may remain as private Driver implementation helpers, but application-level execution should converge on `execute_command()`.

## 16. Migration Safety

At every stage maintain these compatibility guarantees:

```text
Legacy Workflow
      ↓
still parses
      ↓
still executes
      ↓
same result
```

and:

```text
Existing Frontend
      ↓
Existing IPC
      ↓
still works
```

The migration must not require a synchronized frontend/backend release where possible.

## 17. Definition of Done

- [ ] Driver Command abstraction exists.
- [ ] Command Definition exists.
- [ ] Command Manifest exists.
- [ ] Driver supports command discovery.
- [ ] Driver supports `execute_command`.
- [ ] SQL Query is implemented as Command.
- [ ] Workflow supports default Connection.
- [ ] Data Steps inherit Connection.
- [ ] Data Steps support Connection override.
- [ ] Control Flow Steps do not require Connection.
- [ ] Legacy Query configuration remains readable.
- [ ] Legacy Query normalizes to Command.
- [ ] Query execution has one runtime path.
- [ ] `get_connection_commands` exists.
- [ ] Workflow UI dynamically loads commands.
- [ ] Command UI is schema-driven.
- [ ] Changing Connection refreshes Command definitions.
- [ ] Existing permission declarations are reused.
- [ ] Generic `execute_driver_command` exists.
- [ ] Existing Query IPC remains compatible.
- [ ] Existing custom IPC commands can migrate to Driver Commands.
- [ ] Workflow supports cross-Driver execution.
- [ ] No Driver-specific command logic exists in generic Workflow code.
- [ ] Unit tests pass.
- [ ] Workflow regression tests pass.
- [ ] IPC tests pass.
- [ ] UI tests pass.
- [ ] Legacy Workflow fixtures pass.
