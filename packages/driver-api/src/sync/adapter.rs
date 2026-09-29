//! Sync adapter traits — the bridge between native types and the IR.

use super::ir::{IRColumn, IRDefault, IRForeignKey, IRIndex, IRTable, IRTableObjects, IRType};
use super::key::{contract_from_column, SyncKeyContract, SyncKeyValue};
use crate::{ColumnSchema, TableOptions, TableSchema, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// Converts native column metadata into IR (used for the *source* side of a sync).
pub trait SyncSourceAdapter: Send + Sync {
    /// Validate one source column for Data Transfer before any target write.
    ///
    /// This is intentionally separate from the Data Sync key contract and is
    /// only called by the heterogeneous Data Transfer planner/executor. The
    /// default keeps existing adapters source-compatible and behavior-neutral.
    fn validate_transfer_source_column(&self, _column: &ColumnSchema) -> Result<(), String> {
        Ok(())
    }

    /// Whether this source column is represented only by its native type name
    /// in the transfer IR. Targets may preserve it only when they can prove the
    /// same native type exists. This signal is Data Transfer-only.
    fn transfer_source_type_is_native_only(
        &self,
        _column: &ColumnSchema,
        _source_ir: &IRColumn,
    ) -> bool {
        false
    }

    /// Maximum source text size in bytes when the source type has a finite
    /// documented bound. Used only by Data Transfer to compare an existing or
    /// planned target type before writes. `None` means the adapter has no
    /// transfer-specific bound to contribute.
    fn transfer_source_text_limit_bytes(&self, _column: &ColumnSchema) -> Option<u64> {
        None
    }

    /// Whether this source type needs a collation-preserving target mapping
    /// that the source adapter cannot prove from its column metadata.
    fn transfer_source_requires_collation_preservation(&self, _column: &ColumnSchema) -> bool {
        false
    }

    /// Describe the equality and total-order semantics that Data Sync may use
    /// for this column.  The default is conservative: an adapter must opt a
    /// key type in before the host can compare or page it.
    fn sync_key_contract(&self, column: &ColumnSchema) -> Result<SyncKeyContract, String> {
        contract_from_column(column)
    }

    /// Normalize a runtime value using the driver's key contract.  Drivers
    /// override this when their wire value needs a type-aware conversion.
    fn normalize_sync_key(
        &self,
        value: &Option<Value>,
        contract: &SyncKeyContract,
    ) -> Result<SyncKeyValue, String> {
        contract.normalize(value)
    }

    /// Convert a raw key value into the parameter representation required by
    /// the driver's keyset seek expression.  Drivers may need this when their
    /// order expression changes the SQL storage class (for example, SQLite's
    /// `CAST(text_key AS BLOB)`).
    fn sync_key_seek_value(
        &self,
        value: &Value,
        _contract: &SyncKeyContract,
    ) -> Result<Value, String> {
        Ok(value.clone())
    }

    /// SQL expression used by both `ORDER BY` and the seek predicate.  It
    /// must have the same ordering as [`normalize_sync_key`](Self::normalize_sync_key).
    fn sync_key_order_expression(
        &self,
        quoted_column: &str,
        _contract: &SyncKeyContract,
    ) -> String {
        quoted_column.to_string()
    }

    /// Convert a single column to its IR representation.
    ///
    /// `native_full_type` carries the fully-qualified type string with precision
    /// (e.g. PostgreSQL's `format_type()` output). When `None`, the adapter
    /// falls back to `column.data_type`.
    fn column_to_ir(&self, column: &ColumnSchema, native_full_type: Option<&str>) -> IRColumn;

    /// Convert an entire `TableSchema` to an `IRTable`.
    ///
    /// The default implementation iterates over columns and delegates to
    /// [`column_to_ir`](Self::column_to_ir).
    fn table_to_ir(
        &self,
        schema: &TableSchema,
        full_types: Option<&HashMap<String, String>>,
    ) -> IRTable {
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
    }

    /// Convert table-level objects to the neutral form used by export and
    /// transfer renderers. Drivers may override this when their catalog uses
    /// a native index or constraint representation that needs normalization.
    fn table_objects_to_ir(&self, schema: &TableSchema) -> IRTableObjects {
        IRTableObjects {
            indexes: schema
                .indexes
                .iter()
                .map(|index| IRIndex {
                    name: index.name.clone(),
                    columns: index.columns.clone(),
                    is_unique: index.is_unique,
                    is_primary: index.is_primary,
                    index_type: index.index_type.clone(),
                })
                .collect(),
            foreign_keys: schema
                .foreign_keys
                .iter()
                .map(|foreign_key| IRForeignKey {
                    name: foreign_key.name.clone(),
                    columns: foreign_key.columns.clone(),
                    referenced_table: foreign_key.referenced_table.clone(),
                    referenced_columns: foreign_key.referenced_columns.clone(),
                    on_update: foreign_key.on_update.clone(),
                    on_delete: foreign_key.on_delete.clone(),
                })
                .collect(),
        }
    }

    /// Optional SQL returning `(col_name, full_type)` rows for precision-preserving sync.
    /// Default: none (host uses `column.data_type` only).
    fn full_column_types_query(&self, _table: &str) -> Option<String> {
        None
    }

    /// Optional SQL returning a single row whose first column is a CREATE TABLE suffix
    /// (e.g. ClickHouse `ENGINE = MergeTree\nORDER BY (id)`).
    /// Default: none.
    fn table_options_query(&self, _table: &str) -> Option<String> {
        None
    }

    /// Optional query that returns one row per source object whose structure
    /// is not represented in the transfer metadata model (for example a
    /// generated column or expression/prefix index).
    fn unsupported_transfer_structure_query(
        &self,
        _database: &str,
        _schema: Option<&str>,
        _table: &str,
    ) -> Option<String> {
        None
    }
}

/// Renders IR back into native DDL fragments (used for the *target* side of a sync).
pub trait SyncTargetAdapter: Send + Sync {
    /// Render an `IRType` as a native DDL type string.
    fn ir_type_to_native(&self, ir_type: &IRType) -> String;

    /// Data Transfer-only type renderer. Defaults to the shared renderer so
    /// existing Data Sync and Schema Diff behavior is unchanged.
    fn transfer_ir_type_to_native(&self, ir_type: &IRType) -> String {
        self.ir_type_to_native(ir_type)
    }

    /// Whether a default is valid for a Data Transfer-created column. The
    /// shared default policy remains the fallback for existing adapters.
    fn transfer_allows_column_default(&self, ir_type: &IRType) -> bool {
        self.allows_column_default(ir_type)
    }

    /// Whether an explicit native type is known to support a Data Transfer
    /// default. `None` means the adapter cannot prove the native type's
    /// default semantics; Transfer then rejects the default before writing.
    /// Shared DDL callers continue using `allows_column_default`.
    fn transfer_native_type_allows_column_default(&self, _native_type: &str) -> Option<bool> {
        None
    }

    /// Data Transfer-only fallback for a type whose source default cannot be
    /// represented directly. Returning `None` makes the transfer planner fail
    /// closed instead of narrowing or dropping the source default.
    fn transfer_default_capable_type_for(&self, ir_type: &IRType) -> Option<IRType> {
        self.default_capable_type_for(ir_type)
    }

    /// Validate a source-to-target column mapping for Data Transfer before
    /// any target write. `target_native_type` is populated from the inspected
    /// target catalog for existing tables and from the reviewed mapping for
    /// CREATE plans. `creating_target` distinguishes schema portability from
    /// data-only compatibility with an existing table.
    ///
    /// The default is a no-op so this does not alter Data Sync or Schema Diff.
    fn validate_transfer_column_type(
        &self,
        source_column: &ColumnSchema,
        source_ir: &IRColumn,
        _source_text_limit_bytes: Option<u64>,
        _source_requires_collation_preservation: bool,
        source_type_is_native_only: bool,
        _target_native_type: Option<&str>,
        _creating_target: bool,
    ) -> Result<(), String> {
        if source_type_is_native_only {
            return Err(format!(
                "source type '{}' has no target adapter proof of a lossless native mapping",
                source_column.data_type
            ));
        }
        let _ = source_ir;
        Ok(())
    }

    /// Validate character-set metadata for an existing transfer target column.
    /// Drivers that require catalog proof before copying text can reject
    /// unknown or lossy target character sets here.
    fn validate_transfer_target_character_metadata(
        &self,
        _source_ir: &IRColumn,
        _target_native_type: &str,
        _target_character_set: Option<&str>,
        _target_collation: Option<&str>,
        _creating_target: bool,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Optional driver-owned query returning `(column_name, character_set,
    /// collation)` rows for a target table. Data Transfer uses this only to
    /// prove that existing string columns can preserve source text.
    fn transfer_target_character_metadata_query(
        &self,
        _database: &str,
        _schema: Option<&str>,
        _table: &str,
    ) -> Option<String> {
        None
    }

    /// Render an `IRDefault` as the content of a `DEFAULT` clause.
    /// Return `None` to omit the clause entirely (e.g. for auto-increment columns
    /// whose default is handled by the database engine).
    fn format_default(&self, default: &IRDefault) -> Option<String>;

    /// Whether a `DEFAULT` clause is valid for this IR type on the target engine.
    /// MySQL rejects defaults on TEXT/BLOB/JSON/GEOMETRY columns.
    fn allows_column_default(&self, ir_type: &IRType) -> bool {
        let _ = ir_type;
        true
    }

    /// When [`allows_column_default`](Self::allows_column_default) is false but the
    /// source column still has a default, return a narrower IR type that can carry
    /// the default on this engine (e.g. PG `text` → MySQL `VARCHAR(16383)`).
    fn default_capable_type_for(&self, ir_type: &IRType) -> Option<IRType> {
        let _ = ir_type;
        None
    }

    /// Format a runtime `Value` as a SQL literal suitable for INSERT statements.
    fn format_literal(&self, value: &Option<Value>, ir_type: &IRType) -> String;

    fn quote_char(&self) -> char {
        '"'
    }

    fn quote_ident(&self, name: &str) -> String {
        let q = self.quote_char();
        if q == '`' {
            format!("`{}`", name.replace('`', "``"))
        } else {
            format!("\"{}\"", name.replace('"', "\"\""))
        }
    }

    /// Quote a table relation using the target engine's namespace rules.
    /// The database endpoint is part of the reviewed transfer scope even when
    /// an engine cannot express it as a relation qualifier (for example,
    /// PostgreSQL uses the selected database connection and an optional
    /// schema, while MySQL can qualify a relation with its database).
    fn qualify_relation(&self, _database: &str, schema: Option<&str>, table: &str) -> String {
        match schema {
            Some(schema) if !schema.trim().is_empty() => {
                format!("{}.{}", self.quote_ident(schema), self.quote_ident(table))
            }
            _ => self.quote_ident(table),
        }
    }

    /// Whether the target database supports inline PRIMARY KEY constraints
    /// in CREATE TABLE. OLAP engines typically do not.
    fn supports_primary_key(&self) -> bool {
        true
    }

    /// Keyword appended after the column type for auto-increment columns.
    /// Return `None` if the engine uses a different mechanism (e.g. PG SERIAL/IDENTITY).
    fn auto_increment_keyword(&self) -> Option<&str> {
        None
    }

    /// Whether the target accepts explicit values for identity/auto-increment
    /// columns during INSERT, either directly or through a driver-managed
    /// session mode such as SQL Server IDENTITY_INSERT.
    fn supports_explicit_identity_values(&self) -> bool {
        false
    }

    /// Whether non-primary index names are local to a table on this target.
    /// Unknown engines default to the stricter schema-wide rule so a transfer
    /// plan cannot defer a duplicate-name failure until after writes begin.
    fn index_names_are_table_scoped(&self) -> bool {
        false
    }

    /// Whether foreign-key constraint names are local to a table on this
    /// target. Unknown engines default to the stricter schema-wide rule.
    fn foreign_key_names_are_table_scoped(&self) -> bool {
        false
    }

    /// Whether quoted secondary-object names preserve case for uniqueness.
    /// Engines with case-folded identifiers should keep the conservative
    /// default; PostgreSQL quoted names are case-sensitive.
    fn object_names_are_case_sensitive(&self) -> bool {
        false
    }

    /// Map source catalog table options to a target CREATE suffix. The default
    /// recognizes InnoDB as a portable transactional row-store detail and
    /// refuses all options whose semantics cannot be represented.
    fn render_source_table_options(
        &self,
        options: &TableOptions,
    ) -> Result<Option<String>, String> {
        if let Some(collation) = options
            .collation
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let charset_note = if options
                .charset
                .as_deref()
                .is_some_and(|charset| charset.eq_ignore_ascii_case("utf8mb4"))
            {
                " The source UTF8MB4 character encoding can map to PostgreSQL UTF8, but that does not prove equivalent sort, case, or accent rules."
            } else {
                " Matching character encodings do not prove equivalent sort, case, or accent rules."
            };
            return Err(format!(
                "source table collation '{collation}' has no proven equivalent on this target; choose a target collation with reviewed matching semantics or create the target table with an explicit reviewed conversion.{charset_note}"
            ));
        }
        if options.comment.is_some() || options.charset.is_some() {
            return Err("table comment or character set cannot be preserved".into());
        }
        if let Some(engine) = options.engine.as_deref() {
            if !engine.eq_ignore_ascii_case("innodb") {
                return Err(format!("source table engine '{engine}' is unsupported"));
            }
        }
        Ok(None)
    }

    /// Appended after `CREATE TABLE (...)` closing paren. Default: use `ir_table.table_options` if present.
    fn create_table_suffix(&self, ir_table: &IRTable) -> Option<String> {
        ir_table.table_options.clone()
    }

    /// Optional value transform before formatting literals (identity by default).
    fn transform_value(&self, value: &Option<Value>, _ir_type: &IRType) -> Option<Value> {
        value.clone()
    }

    /// Render one non-primary index after its table has been created. The
    /// default is deliberately conservative: only ordinary B-tree indexes
    /// have portable SQL across the registered SQL adapters. Drivers with
    /// richer index syntax can override this hook.
    fn render_index_ddl(&self, table_ref: &str, index: &IRIndex) -> Result<Option<String>, String> {
        if index.is_primary {
            return Ok(None);
        }
        if index.name.trim().is_empty() || index.columns.is_empty() {
            return Err("index must have a name and at least one column".into());
        }
        if index
            .columns
            .iter()
            .any(|column| column.trim().is_empty() || column.contains('(') || column.contains(')'))
        {
            return Err(
                "target adapter cannot represent index expressions or prefix lengths".into(),
            );
        }
        let normalized = index.index_type.trim().to_ascii_lowercase();
        if !normalized.is_empty() && !matches!(normalized.as_str(), "btree" | "b-tree" | "default")
        {
            return Err(format!(
                "target adapter cannot represent index type '{}'",
                index.index_type
            ));
        }
        let unique = if index.is_unique { "UNIQUE " } else { "" };
        let columns = index
            .columns
            .iter()
            .map(|column| self.quote_ident(column))
            .collect::<Vec<_>>()
            .join(", ");
        Ok(Some(format!(
            "CREATE {unique}INDEX {} ON {table_ref} ({columns})",
            self.quote_ident(&index.name)
        )))
    }

    /// Validate a mapped target column before placing it in an ordinary
    /// secondary index. Engines that require prefix lengths or special index
    /// forms for some native types should reject those types here so the host
    /// can stop before any table DDL is executed.
    fn validate_index_column_type(&self, _column: &IRColumn) -> Result<(), String> {
        Ok(())
    }

    /// Validate a complete index key after its target columns have been
    /// projected and mapped. Implementations may enforce aggregate key limits.
    fn validate_index_columns(&self, columns: &[IRColumn]) -> Result<(), String> {
        for column in columns {
            self.validate_index_column_type(column)?;
        }
        Ok(())
    }

    /// Render one foreign key after all table definitions and row data. The
    /// caller supplies a target relation reference so source catalogs never
    /// leak into a generated SQL-file artifact.
    fn render_foreign_key_ddl(
        &self,
        table_ref: &str,
        foreign_key: &IRForeignKey,
        referenced_table_ref: &str,
    ) -> Result<String, String> {
        if foreign_key.name.trim().is_empty()
            || foreign_key.columns.is_empty()
            || foreign_key.columns.len() != foreign_key.referenced_columns.len()
        {
            return Err("foreign key must have a name and matching column lists".into());
        }
        let action = |raw: &str, clause: &str| -> Result<Option<String>, String> {
            let normalized = raw.trim().to_ascii_uppercase();
            if normalized.is_empty() || normalized == "NO ACTION" {
                return Ok(None);
            }
            if matches!(normalized.as_str(), "CASCADE" | "RESTRICT" | "SET NULL") {
                return Ok(Some(format!(" {clause} {normalized}")));
            }
            Err(format!(
                "target adapter cannot represent foreign-key action '{}'",
                raw
            ))
        };
        let on_update = action(&foreign_key.on_update, "ON UPDATE")?;
        let on_delete = action(&foreign_key.on_delete, "ON DELETE")?;
        let columns = foreign_key
            .columns
            .iter()
            .map(|column| self.quote_ident(column))
            .collect::<Vec<_>>()
            .join(", ");
        let referenced_columns = foreign_key
            .referenced_columns
            .iter()
            .map(|column| self.quote_ident(column))
            .collect::<Vec<_>>()
            .join(", ");
        Ok(format!(
            "ALTER TABLE {table_ref} ADD CONSTRAINT {} FOREIGN KEY ({columns}) REFERENCES {referenced_table_ref} ({referenced_columns}){}{}",
            self.quote_ident(&foreign_key.name),
            on_update.unwrap_or_default(),
            on_delete.unwrap_or_default()
        ))
    }
}

/// Type-erased sync adapter pair produced by driver crates.
pub struct BoxedSyncAdapter {
    pub source: Arc<dyn SyncSourceAdapter>,
    pub target: Arc<dyn SyncTargetAdapter>,
}

impl BoxedSyncAdapter {
    pub fn both<T>(adapter: T) -> Self
    where
        T: SyncSourceAdapter + SyncTargetAdapter + 'static,
    {
        let arc = Arc::new(adapter);
        Self {
            source: arc.clone(),
            target: arc,
        }
    }
}

/// Inventory factory discovered by the host sync registry.
pub struct SyncAdapterFactory {
    pub db_types: &'static [&'static str],
    pub create: fn() -> BoxedSyncAdapter,
}

inventory::collect!(SyncAdapterFactory);

/// Helper: submit a sync adapter factory at link time.
///
/// ```ignore
/// datazen_driver_api::register_sync_adapter!(SyncAdapterFactory {
///     db_types: &["postgresql"],
///     create: || BoxedSyncAdapter::both(PgSyncAdapter),
/// });
/// ```
#[macro_export]
macro_rules! register_sync_adapter {
    ($factory:expr) => {
        $crate::inventory::submit! { $factory }
    };
}
