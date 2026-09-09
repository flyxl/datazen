# Driver API Schema Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix critical and important schema bugs identified in `driver-api` and host integrations: catalog command inheritance in drivers and host fallback, type parsing truncation (PostgreSQL arrays / timezone modifiers), SQL Server schema object SQL injection/quoting vulnerabilities, primary key fallback consistency, and relation DDL object kind support.

**Architecture:** 
1. Enhance `driver-api` command helpers and trait defaults, ensuring catalog commands (`list_databases`, `list_tables`, `get_table_schema`) are reliably available across all drivers, and add defensive fallback in host `execute_driver_command_with_mode`.
2. Fix `parse_type_parts` in `schema_migration.rs` to retain characters after closing parenthesis (e.g. array brackets `[]`, `WITH TIME ZONE`).
3. Sanitize SQL Server DDL generation in `schema_objects.rs` using escaped literals and bracket identifiers.
4. Align `get_columns` and `table_to_ir` to use `TableSchema::effective_primary_keys()`.
5. Support `Table` and `View` in `ObjectKind` and `object_ddl_sql` for universal relation DDL extraction.

**Tech Stack:** Rust (edition 2021), Tauri v2, sqlparser, async-trait, TypeScript / Vitest.

## Global Constraints

- Workspace path: `/Users/flyxl/code/datazen/.worktrees/driver-api-schema`
- Branch: `feature/driver-api-schema`
- All driver crate rules: Driver-specific logic must be kept clean, zero panic in production code (`unwrap()` / `expect()` forbidden in production paths).
- Test verification: Run `cargo test -p datazen-driver-api` and relevant driver/host tests for every task.

---

### Task 1: Fix Type Parsing Truncation (`parse_type_parts`)

**Files:**
- Modify: `packages/driver-api/src/schema_migration.rs:136-180`
- Test: `packages/driver-api/src/schema_migration.rs:185-225`

**Interfaces:**
- Produces: `parse_type_parts(raw: &str) -> (String, Option<String>, String)` correctly returning base type, arguments inside parentheses, and full suffix including array brackets `[]` and trailing modifiers like `WITH TIME ZONE`.

- [ ] **Step 1: Write the failing tests**

In `packages/driver-api/src/schema_migration.rs` under `mod type_parts_tests`:

```rust
    #[test]
    fn parse_array_types_preserves_brackets() {
        let (base, args, suffix) = parse_type_parts("VARCHAR(255)[]");
        assert_eq!(base, "VARCHAR");
        assert_eq!(args.as_deref(), Some("255"));
        assert_eq!(suffix, "[]");
        assert_eq!(format_type(&base, args.as_deref(), &suffix), "VARCHAR(255) []");
    }

    #[test]
    fn parse_timestamp_with_time_zone_preserves_suffix() {
        let (base, args, suffix) = parse_type_parts("TIMESTAMP(6) WITH TIME ZONE");
        assert_eq!(base, "TIMESTAMP");
        assert_eq!(args.as_deref(), Some("6"));
        assert_eq!(suffix, "WITH TIME ZONE");
        assert_eq!(format_type(&base, args.as_deref(), &suffix), "TIMESTAMP(6) WITH TIME ZONE");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p datazen-driver-api parse_array_types_preserves_brackets`
Expected: FAIL (assertion failed: `suffix == "[]"`, actual `""`).

- [ ] **Step 3: Implement fix in `parse_type_parts`**

Update `parse_type_parts` in `packages/driver-api/src/schema_migration.rs`:

```rust
pub fn parse_type_parts(raw: &str) -> (String, Option<String>, String) {
    let trimmed = collapse_ws(raw);
    let (core, suffix) = peel_suffixes(&trimmed);
    match (core.find('('), core.rfind(')')) {
        (Some(open), Some(close)) if close > open => {
            let base = core[..open].trim().to_string();
            let args = Some(core[open + 1..close].trim().to_string());
            let remainder = core[close + 1..].trim();
            let combined_suffix = match (remainder.is_empty(), suffix.is_empty()) {
                (true, true) => String::new(),
                (false, true) => remainder.to_string(),
                (true, false) => suffix,
                (false, false) => format!("{remainder} {suffix}"),
            };
            (base, args, combined_suffix)
        }
        _ => (core, None, suffix),
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p datazen-driver-api schema_migration`
Expected: PASS (all tests in `schema_migration` pass).

- [ ] **Step 5: Commit**

```bash
git add packages/driver-api/src/schema_migration.rs
git commit -m "fix(driver-api): preserve array brackets and modifiers in parse_type_parts"
```

---

### Task 2: Fix Primary Key Extraction Consistency (`effective_primary_keys`)

**Files:**
- Modify: `packages/driver-api/src/traits.rs:190-196`
- Modify: `packages/driver-api/src/sync/adapter.rs:25-35`
- Test: `packages/driver-api/src/traits.rs`

**Interfaces:**
- Consumes: `TableSchema::effective_primary_keys(&self) -> Vec<String>`
- Produces: `DatabaseDriver::get_columns` and `SyncSourceAdapter::table_to_ir` return primary keys even if only `ColumnSchema.is_primary_key` was populated by driver.

- [ ] **Step 1: Write the failing tests**

In `packages/driver-api/src/traits.rs` under `mod structure_defaults_tests`:

```rust
    #[tokio::test]
    async fn get_columns_uses_effective_primary_keys_when_primary_keys_empty() {
        struct DriverWithColumnPkOnly;
        #[async_trait]
        impl DatabaseDriver for DriverWithColumnPkOnly {
            fn driver_type(&self) -> DatabaseType { "dummy".into() }
            async fn connect(&self, _: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> { unreachable!() }
            async fn test_connection(&self, _: &ConnectionConfig) -> Result<ServerInfo, DriverError> { unreachable!() }
            async fn disconnect(&self, _: ConnectionHandle) -> Result<(), DriverError> { Ok(()) }
            async fn get_databases(&self, _: &ConnectionHandle) -> Result<Vec<String>, DriverError> { Ok(vec![]) }
            async fn get_tables(&self, _: &ConnectionHandle, _: &str) -> Result<Vec<TableInfo>, DriverError> { Ok(vec![]) }
            async fn get_table_schema(&self, _: &ConnectionHandle, _: &str) -> Result<TableSchema, DriverError> {
                Ok(TableSchema {
                    table_name: "users".into(),
                    columns: vec![
                        ColumnSchema {
                            name: "id".into(),
                            data_type: "int".into(),
                            nullable: false,
                            default_value: None,
                            comment: None,
                            is_primary_key: true,
                            is_auto_increment: true,
                        }
                    ],
                    primary_keys: vec![], // intentionally empty to test fallback
                    indexes: vec![],
                    foreign_keys: vec![],
                })
            }
            async fn query(&self, _: &ConnectionHandle, _: &str) -> Result<QueryResult, DriverError> { unreachable!() }
            async fn query_multi(&self, _: &ConnectionHandle, _: &str, _: Option<u32>) -> Result<MultiQueryResult, DriverError> { unreachable!() }
            async fn query_with_params(&self, _: &ConnectionHandle, _: &str, _: &[Value]) -> Result<QueryResult, DriverError> { unreachable!() }
            async fn execute(&self, _: &ConnectionHandle, _: &str) -> Result<u64, DriverError> { unreachable!() }
            async fn cancel_query(&self, _: &ConnectionHandle) -> Result<(), DriverError> { Ok(()) }
        }

        let driver = DriverWithColumnPkOnly;
        let handle = ConnectionHandle { id: "c".into(), pool_id: "p".into() };
        let (_cols, pks) = driver.get_columns(&handle, "users").await.unwrap();
        assert_eq!(pks, vec!["id"]);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p datazen-driver-api get_columns_uses_effective_primary_keys_when_primary_keys_empty`
Expected: FAIL (`assert_eq!(pks, vec!["id"])` failed, actual was `[]`).

- [ ] **Step 3: Update `get_columns` and `table_to_ir`**

In `packages/driver-api/src/traits.rs:190`:
```rust
    async fn get_columns(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        let schema = self.get_table_schema(handle, table).await?;
        let pks = schema.effective_primary_keys();
        Ok((schema.columns, pks))
    }
```

In `packages/driver-api/src/sync/adapter.rs:25`:
```rust
        let effective_pks = schema.effective_primary_keys();
        let pk_set: std::collections::HashSet<&str> =
            effective_pks.iter().map(|s| s.as_str()).collect();

        let columns = schema
            .columns
            .iter()
            .map(|c| {
                let ft = full_types.and_then(|m| m.get(&c.name)).map(|s| s.as_str());
                let mut ir = self.column_to_ir(c, ft);
                if pk_set.contains(c.name.as_str()) {
                    ir.is_primary_key = true;
                }
                ir
            })
            .collect();

        IRTable {
            name: schema.table_name.clone(),
            columns,
            primary_keys: effective_pks,
            table_options: None,
        }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p datazen-driver-api`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/driver-api/src/traits.rs packages/driver-api/src/sync/adapter.rs
git commit -m "fix(driver-api): use effective_primary_keys in get_columns and table_to_ir"
```

---

### Task 3: Fix SQL Server Schema Objects Injection & Identifier Quoting

**Files:**
- Modify: `packages/driver-api/src/schema_objects.rs:240-278`
- Test: `packages/driver-api/src/schema_objects.rs:410-440`

**Interfaces:**
- Produces: `object_ddl_sql` properly quotes SQL Server schema and object names with brackets and escapes single quotes inside string literals.

- [ ] **Step 1: Write the failing tests**

In `packages/driver-api/src/schema_objects.rs` under `mod tests`:

```rust
    #[test]
    fn sqlserver_object_ddl_escapes_single_quotes_and_brackets() {
        let ddl_sql = object_ddl_sql("sqlserver", ObjectKind::Function, "fn'special", Some("custom schema")).unwrap();
        assert!(ddl_sql.contains("[custom schema].[fn'special]"));
        assert!(ddl_sql.contains("fn''special"));

        let seq_sql = object_ddl_sql("sqlserver", ObjectKind::Sequence, "seq'one", Some("dbo")).unwrap();
        assert!(seq_sql.contains("WHERE q.name = 'seq''one'"));
        assert!(seq_sql.contains("[dbo].[seq'one]"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p datazen-driver-api sqlserver_object_ddl_escapes_single_quotes_and_brackets`
Expected: FAIL.

- [ ] **Step 3: Implement fix in `object_ddl_sql` for sqlserver**

In `packages/driver-api/src/schema_objects.rs:240-278`:
```rust
        ("sqlserver", ObjectKind::Function | ObjectKind::Procedure | ObjectKind::Trigger) => {
            let schema_str = schema.filter(|s| !s.is_empty()).unwrap_or("dbo");
            let target_ident = format!("{}.{}", quote_ident("sqlserver", schema_str), quote_ident("sqlserver", name));
            let target_literal = sql_string(&target_ident);
            Some(format!(
                "SELECT OBJECT_DEFINITION(OBJECT_ID({target_literal})) AS ddl"
            ))
        }
        ("sqlserver", ObjectKind::Sequence) => {
            let schema_str = schema.filter(|s| !s.is_empty()).unwrap_or("dbo");
            let quoted_schema = quote_ident("sqlserver", schema_str);
            let quoted_name = quote_ident("sqlserver", name);
            let name_literal = sql_string(name);
            Some(format!(
                "SELECT 'CREATE SEQUENCE {quoted_schema}.{quoted_name} AS [' + ty.name + '] ' \
                 + 'START WITH ' + CAST(q.start_value AS varchar) \
                 + ' INCREMENT BY ' + CAST(q.increment AS varchar) \
                 + CASE WHEN q.is_cycling = 1 THEN ' CYCLE' ELSE ' NO CYCLE' END \
                 AS ddl \
                 FROM sys.sequences q \
                 JOIN sys.types ty ON ty.user_type_id = q.user_type_id \
                 WHERE q.name = {name_literal}"
            ))
        }
        ("sqlserver", ObjectKind::Type) => {
            let schema_str = schema.filter(|s| !s.is_empty()).unwrap_or("dbo");
            let quoted_schema = quote_ident("sqlserver", schema_str);
            let quoted_name = quote_ident("sqlserver", name);
            let name_literal = sql_string(name);
            let schema_literal = sql_string(schema_str);
            Some(format!(
                "SELECT 'CREATE TYPE {quoted_schema}.{quoted_name} FROM [' + ty.name + '](' \
                 + CASE WHEN ty.max_length > 0 THEN CAST(ty.max_length AS varchar) \
                        ELSE CAST(t.precision AS varchar) + ',' + CAST(t.scale AS varchar) END \
                 + ')' AS ddl \
                 FROM sys.types t \
                 JOIN sys.types ty ON ty.user_type_id = t.system_type_id \
                 WHERE t.is_user_defined = 1 AND t.schema_id = SCHEMA_ID({schema_literal}) \
                   AND t.name = {name_literal}"
            ))
        }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p datazen-driver-api sqlserver`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/driver-api/src/schema_objects.rs
git commit -m "fix(driver-api): sanitize SQL Server object DDL queries and identifier quoting"
```

---

### Task 4: Support Table & View in ObjectKind and `object_ddl_sql`

**Files:**
- Modify: `packages/driver-api/src/schema_objects.rs:7-36, 160-280`
- Modify: `packages/driver-api/src/schema_object_commands.rs:35-37, 72-74`
- Test: `packages/driver-api/src/schema_objects.rs`

**Interfaces:**
- Produces: `ObjectKind::Table`, `ObjectKind::View` parseable from `"table"` and `"view"`, allowing `object_ddl_sql` to return DDL query across MySQL, PostgreSQL, and SQLite.

- [ ] **Step 1: Write the failing tests**

In `packages/driver-api/src/schema_objects.rs`:

```rust
    #[test]
    fn object_kind_parses_table_and_view() {
        assert_eq!(ObjectKind::parse("table"), Some(ObjectKind::Table));
        assert_eq!(ObjectKind::parse("view"), Some(ObjectKind::View));
        assert_eq!(ObjectKind::Table.as_str(), "table");
        assert_eq!(ObjectKind::View.as_str(), "view");

        assert!(object_ddl_sql("mysql", ObjectKind::Table, "users", None).is_some());
        assert!(object_ddl_sql("postgresql", ObjectKind::View, "active_users", Some("public")).is_some());
        assert!(object_ddl_sql("sqlite", ObjectKind::Table, "users", None).is_some());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p datazen-driver-api object_kind_parses_table_and_view`
Expected: FAIL (no `ObjectKind::Table` variant).

- [ ] **Step 3: Add Table & View to `ObjectKind` and `object_ddl_sql`**

Update `ObjectKind`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Table,
    View,
    Function,
    Procedure,
    Trigger,
    Sequence,
    Type,
}
```
Update `as_str()` and `parse()` to include `"table"` and `"view"`.
Update `object_ddl_sql`:
- PostgreSQL:
  - `ObjectKind::View` $\to$ `"SELECT pg_get_viewdef(c.oid, true) AS ddl FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE c.relname = '...' AND n.nspname = '...'"`
  - `ObjectKind::Table` $\to$ None (or driver dump_table_ddl)
- MySQL:
  - `ObjectKind::Table` $\to$ `format!("SHOW CREATE TABLE {ident}")`
  - `ObjectKind::View` $\to$ `format!("SHOW CREATE VIEW {ident}")`
- SQLite:
  - `ObjectKind::Table` $\to$ `format!("SELECT sql AS ddl FROM sqlite_master WHERE type = 'table' AND name = {name_literal}")`
  - `ObjectKind::View` $\to$ `format!("SELECT sql AS ddl FROM sqlite_master WHERE type = 'view' AND name = {name_literal}")`

Update schema in `packages/driver-api/src/schema_object_commands.rs`: add `"table"`, `"view"` to the enum lists of input schemas.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p datazen-driver-api`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/driver-api/src/schema_objects.rs packages/driver-api/src/schema_object_commands.rs
git commit -m "feat(driver-api): add table and view to ObjectKind and object_ddl_sql"
```

---

### Task 5: Ensure Catalog Command Availability Across Drivers & Host Defensive Fallback

**Files:**
- Modify: `src-tauri/src/commands/driver_command/execute.rs:38-46`
- Modify: `packages/driver-api/src/command.rs` (add standard helper)
- Modify: `packages/drivers/clickhouse/src/clickhouse.rs`
- Modify: `packages/drivers/sqlserver/src/admin_commands.rs`
- Modify: `packages/drivers/mongodb/src/mongodb.rs`
- Modify: `packages/drivers/elasticsearch/src/elasticsearch.rs`
- Modify: `packages/drivers/hbase/src/hbase.rs`
- Modify: `packages/drivers/influxdb/src/influxdb.rs`
- Modify: `packages/drivers/vector/src/vector.rs`
- Modify: `packages/drivers/victoriametrics/src/victoriametrics.rs`
- Test: `src-tauri/tests/driver_command_ipc.rs` or `packages/drivers/clickhouse/tests/command_definitions.rs`

**Interfaces:**
- Produces: All drivers declare `list_databases`, `list_tables`, and `get_table_schema`. The host command runner additionally falls back to schema catalog definitions if a driver's custom definitions omitted them.

- [ ] **Step 1: Write the failing tests**

In `packages/drivers/clickhouse/tests/command_definitions.rs`:
```rust
#[test]
fn command_definitions_include_catalog_commands() {
    let ids: Vec<String> = ClickHouseDriver::new()
        .command_definitions()
        .into_iter()
        .map(|d| d.id)
        .collect();
    assert!(ids.contains(&"list_databases".to_string()));
    assert!(ids.contains(&"list_tables".to_string()));
    assert!(ids.contains(&"get_table_schema".to_string()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p datazen-driver-clickhouse command_definitions_include_catalog_commands`
Expected: FAIL (assertion failed: ids contains "list_databases").

- [ ] **Step 3: Implement driver command helper and host fallback**

1. In `src-tauri/src/commands/driver_command/execute.rs:38-46`:
```rust
    let mut definitions = driver.command_definitions();
    // Safety guard: if a driver forgot to extend schema_catalog_command_definitions,
    // inject them so navigation/browsing is never broken.
    if datazen_driver_api::is_schema_catalog_command(&request.command)
        && !definitions.iter().any(|d| d.id == request.command)
    {
        definitions.extend(datazen_driver_api::schema_catalog_command_definitions());
    }

    let definition = definitions
        .into_iter()
        .find(|definition| definition.id == request.command)
        .ok_or_else(|| {
            CommandError::Validation(format!("Unsupported driver command: {}", request.command))
        })?;
```

2. In each affected driver crate, extend `command_definitions()` with `schema_catalog_command_definitions()`:
- `packages/drivers/clickhouse/src/clickhouse.rs`: `cmds.extend(schema_catalog_command_definitions());`
- `packages/drivers/sqlserver/src/admin_commands.rs`: `cmds.extend(schema_catalog_command_definitions());`
- `packages/drivers/mongodb/src/mongodb.rs`: `cmds.extend(schema_catalog_command_definitions());`
- `packages/drivers/elasticsearch/src/elasticsearch.rs`: `cmds.extend(schema_catalog_command_definitions());`
- `packages/drivers/hbase/src/hbase.rs`: `cmds.extend(schema_catalog_command_definitions());`
- `packages/drivers/influxdb/src/influxdb.rs`: `cmds.extend(schema_catalog_command_definitions());`
- `packages/drivers/vector/src/vector.rs`: `cmds.extend(schema_catalog_command_definitions());`
- `packages/drivers/victoriametrics/src/victoriametrics.rs`: `cmds.extend(schema_catalog_command_definitions());`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p datazen-driver-clickhouse command_definitions_include_catalog_commands`
Run: `cargo test -p datazen --lib commands::schema`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/driver_command/execute.rs packages/drivers/
git commit -m "fix(driver): include schema catalog commands in all drivers and add host fallback"
```

---

### Task 6: Comprehensive Regression and Verification

**Files:**
- Test all crates touched.

- [ ] **Step 1: Run driver-api tests**
Run: `cargo test -p datazen-driver-api`
Expected: PASS.

- [ ] **Step 2: Run host Rust tests**
Run: `cargo test -p datazen --lib`
Expected: PASS.

- [ ] **Step 3: Run path driver UI tests**
Run: `pnpm test:unit:drivers`
Expected: PASS.

- [ ] **Step 4: Run frontend schemaStore tests**
Run: `npx vitest run src/stores/__tests__/schemaStore.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit any cleanup or doc updates**
```bash
git commit --allow-empty -m "chore: verify clean test suite across driver-api, host, and drivers"
```
