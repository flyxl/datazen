//! The instrument for the refusal tier's coverage claim: a real driver with
//! **exactly one** serverless trait method's return value changed.
//!
//! The refusal tier asserts that `RefusalSnapshot` compares the wrapper's
//! serverless trait surface field by field. No amount of source reading settles
//! that: a method name can sit in the "compared" list while the snapshot has
//! stopped reading it, and every assertion in the file stays green. What does
//! settle it is moving the value. If the snapshot really reads
//! `supports_offset`, then changing what that method returns must change what
//! the snapshot reads out — and if the snapshot does not read it, the change is
//! invisible and the guard must be red.
//!
//! So the claim gets attacked instead of described. For each method the probe
//! can perturb, the refusal tier reads the snapshot off the inner driver and off
//! the probe and requires the two to differ.
//!
//! **Not a fake runtime** (`fake-runtime-fixtures.md` §10.4). Every value the
//! probe returns is either the inner driver's own, unchanged, or that value with
//! one deliberate edit; the connection methods are pure delegation. Nothing is
//! synthesised, and the probe answers nothing about any protocol — it exists only
//! to be unequal in exactly one place.
//!
//! The set of methods this probe can perturb is **not** a list: the refusal
//! tier reads it out of this file's own trait impl, keeping only the methods
//! whose bodies actually perturb, so it describes what the code can do rather
//! than what a document claims it can do.
//!
//! `#[path]`-included from `real_driver_contract.rs`; never compiled alone.

#![allow(dead_code)]

use datazen_driver_api::{
    async_trait, ConnectionConfig, ConnectionHandle, DatabaseDriver, DatabaseType, DdlAtomicity,
    DriverCommandDefinition, DriverError, QueryResult, ServerInfo, SqlLiteralDialect, TableInfo,
    TableSchema, Value,
};

/// Suffix appended to a perturbed `String`. A suffix rather than a fixed
/// replacement, so the perturbed value can never coincide with the one it
/// replaced however exotic the driver's own output is.
const PROBE_MARK: &str = " dz_probe";

/// The header of this file's trait impl, read by the guard. Spelled out here
/// rather than derived from the type, because the guard has to find the block in
/// the *file*.
///
/// Assembled from two fragments so the literal does not appear anywhere in this
/// file: a guard that searched for the header would otherwise find this
/// declaration first, and read the struct below it as if it were the impl. The
/// guard asserts the header occurs exactly once, which is what makes the split
/// safe rather than merely tidy.
pub const PROBE_IMPL_HEADER: &str = concat!(
    "impl<D: DatabaseDriver> DatabaseDriver for Misreports",
    "<D>"
);

/// The call that marks a method as perturbable. Every method the guard's
/// sensitivity loop reaches is exactly a method whose body contains it.
pub const PROBE_MARKER: &str = "self.hit(";

/// The name of this file, used to locate it in the template directory.
pub const PROBE_SOURCE_NAME: &str = "real_driver_contract_probe.rs";

/// A real driver with one named method's return value changed.
///
/// `corrupt` names the one method to perturb. Every other method returns the
/// inner driver's own value, so any difference between the two snapshots is
/// attributable to that one method.
pub struct Misreports<D> {
    inner: D,
    corrupt: String,
}

impl<D: DatabaseDriver> Misreports<D> {
    pub fn new(inner: D, corrupt: &str) -> Self {
        Self {
            inner,
            corrupt: corrupt.to_string(),
        }
    }

    /// True when the method being asked for is the one being perturbed.
    ///
    /// `&str`, not `&'static str`, and a `String` on the struct rather than a
    /// borrowed name: the caller iterates a list built at runtime, and leaking
    /// into `'static` to satisfy a signature would be a worse trade than one
    /// small allocation per test.
    fn hit(&self, method: &str) -> bool {
        self.corrupt == method
    }
}

/// A `bool` guaranteed to differ from `base` when `hit`.
fn flip(hit: bool, base: bool) -> bool {
    if hit {
        !base
    } else {
        base
    }
}

/// A `String` guaranteed to differ from `base` when `hit`.
fn marked(hit: bool, base: &str) -> String {
    if hit {
        format!("{base}{PROBE_MARK}")
    } else {
        base.to_string()
    }
}

/// A `char` guaranteed to differ from `base` when `hit`, chosen so it is still a
/// character a driver might plausibly quote with — a nonsense sentinel would make
/// the comparison trivially true for any snapshot that stores the value at all.
fn other_char(hit: bool, base: char) -> char {
    if hit {
        if base == '"' {
            '\''
        } else {
            '"'
        }
    } else {
        base
    }
}

/// A [`DdlAtomicity`] guaranteed to differ from `base` when `hit`.
fn other_atomicity(hit: bool, base: DdlAtomicity) -> DdlAtomicity {
    if hit {
        match base {
            DdlAtomicity::Transactional => DdlAtomicity::AutoCommitPerStatement,
            _ => DdlAtomicity::Transactional,
        }
    } else {
        base
    }
}

/// A dialect guaranteed to differ from `base` when `hit`.
fn other_sql_literal_dialect(
    hit: bool,
    base: Option<SqlLiteralDialect>,
) -> Option<SqlLiteralDialect> {
    if !hit {
        return base;
    }
    Some(match base {
        None | Some(SqlLiteralDialect::SqlServer) => SqlLiteralDialect::Sqlite,
        Some(SqlLiteralDialect::Sqlite) => SqlLiteralDialect::Postgres,
        Some(SqlLiteralDialect::Postgres) => SqlLiteralDialect::MySql,
        Some(SqlLiteralDialect::MySql) => SqlLiteralDialect::ClickHouse,
        Some(SqlLiteralDialect::ClickHouse) => SqlLiteralDialect::DuckDb,
        Some(SqlLiteralDialect::DuckDb) => SqlLiteralDialect::SqlServer,
    })
}

#[async_trait]
impl<D: DatabaseDriver> DatabaseDriver for Misreports<D> {
    // --- the serverless surface: each method perturbs only when named
    fn supports_query_execution_cancel(&self) -> bool {
        flip(
            self.hit("supports_query_execution_cancel"),
            self.inner.supports_query_execution_cancel(),
        )
    }

    fn driver_type(&self) -> DatabaseType {
        marked(self.hit("driver_type"), &self.inner.driver_type())
    }

    fn sync_family(&self) -> String {
        marked(self.hit("sync_family"), &self.inner.sync_family())
    }

    fn quote_char(&self) -> char {
        other_char(self.hit("quote_char"), self.inner.quote_char())
    }

    fn quote_ident(&self, name: &str) -> String {
        marked(self.hit("quote_ident"), &self.inner.quote_ident(name))
    }

    fn ddl_atomicity(&self) -> DdlAtomicity {
        other_atomicity(self.hit("ddl_atomicity"), self.inner.ddl_atomicity())
    }

    fn format_sql_literal(&self, value: &Option<Value>) -> String {
        marked(
            self.hit("format_sql_literal"),
            &self.inner.format_sql_literal(value),
        )
    }

    fn sql_literal_dialect(&self) -> Option<SqlLiteralDialect> {
        other_sql_literal_dialect(
            self.hit("sql_literal_dialect"),
            self.inner.sql_literal_dialect(),
        )
    }

    fn supports_bound_writes(&self) -> bool {
        flip(
            self.hit("supports_bound_writes"),
            self.inner.supports_bound_writes(),
        )
    }

    fn try_format_sql_literal(&self, value: &Option<Value>) -> Result<String, DriverError> {
        let result = self.inner.try_format_sql_literal(value);
        if self.hit("try_format_sql_literal") {
            Ok(match result {
                Ok(value) => format!("{value}{PROBE_MARK}"),
                Err(_) => PROBE_MARK.to_string(),
            })
        } else {
            result
        }
    }

    fn supports_offset(&self) -> bool {
        flip(self.hit("supports_offset"), self.inner.supports_offset())
    }

    fn supports_explain(&self) -> bool {
        flip(self.hit("supports_explain"), self.inner.supports_explain())
    }

    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        let base = self.inner.command_definitions();
        if !self.hit("command_definitions") {
            return base;
        }
        base.into_iter()
            .map(|mut definition| {
                definition.id = marked(true, &definition.id);
                definition
            })
            .collect()
    }

    fn qualify_sql_target(
        &self,
        sql: &str,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> Option<String> {
        let base = self.inner.qualify_sql_target(sql, database, schema);
        if self.hit("qualify_sql_target") {
            // `None` becomes `Some`: a driver that declines to qualify at all must
            // still come back unequal, or the probe would read as insensitive
            // when the method is in fact read.
            Some(marked(true, base.as_deref().unwrap_or(sql)))
        } else {
            base
        }
    }

    fn has_multi_database(&self) -> bool {
        flip(
            self.hit("has_multi_database"),
            self.inner.has_multi_database(),
        )
    }

    fn has_schema_level(&self) -> bool {
        flip(self.hit("has_schema_level"), self.inner.has_schema_level())
    }

    // --- pure delegation. The probe never opens a connection; these exist only
    //     because the trait requires them, and are never reached by the tier that
    //     uses this type.
    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        self.inner.connect(config).await
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        self.inner.test_connection(config).await
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        self.inner.disconnect(handle).await
    }

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        self.inner.get_databases(handle).await
    }

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        self.inner.execute(handle, sql).await
    }

    async fn cancel_query(&self, handle: &ConnectionHandle) -> Result<(), DriverError> {
        self.inner.cancel_query(handle).await
    }

    // --- more pure delegation, for the trait items that carry no default body.
    //     Same argument: a no-server tier never reaches them, and answering with
    //     the inner driver's own value keeps "the probe differs in exactly one
    //     place" true for every method it *is* asked about.
    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        self.inner.get_tables(handle, database, schema).await
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        self.inner
            .get_table_schema(handle, table, database, schema)
            .await
    }

    async fn query(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        self.inner.query(handle, sql).await
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<datazen_driver_api::MultiQueryResult, DriverError> {
        self.inner.query_multi(handle, sql, limit).await
    }

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        self.inner.query_with_params(handle, sql, params).await
    }
}
