//! Port of `packages/frontend/src/lib/structureEditor/resolveCreateTableSchema.ts`.
//!
//! The branch order is kept verbatim: absent meta falls back on the dialect id,
//! `namespaceEnsure === 'postgresql'` always wins (a PG schema namespace, never
//! the logical database name), `databaseFieldType === 'path'` is unqualified,
//! and everything else uses `currentDatabase`. `??` maps to `Option::or` /
//! `unwrap_or_else`, so an empty string stays a value instead of being treated
//! as absent, exactly as in JavaScript.
//!
//! React's `resolveCreateTableSchema` reads `DB_REGISTRY[databaseType]`. There
//! is no Rust `DB_REGISTRY` in `datazen-ui` / `datazen_ui_api`: the driver meta
//! lives in the TypeScript extension registry. Rather than hard-coding one
//! dialect in host logic, the Rune entry point takes an **injected** meta
//! resolver closure and passes its answer to the branch-for-branch port.
//!
//! Removal stage: when a Rust registry equivalent is exposed through
//! `datazen_ui_api`, replace `meta_resolver` with that lookup and keep
//! `resolve_create_table_schema_from_meta` unchanged.

/// `namespaceEnsure` value that selects the PostgreSQL-schema branch.
pub const NAMESPACE_POSTGRESQL: &str = "postgresql";
/// React's fallback when the meta has no `namespaceEnsure` and the dialect is
/// not PostgreSQL.
pub const NAMESPACE_DEFAULT_SQL: &str = "default-sql";
/// `databaseFieldType` value that makes the qualifier `null`.
pub const DATABASE_FIELD_PATH: &str = "path";
/// React's fallback PG schema namespace.
pub const DEFAULT_PG_SCHEMA: &str = "public";

/// The subset of `DatabaseTypeMeta` this resolver reads. React types both
/// fields as required/optional union strings; `Option<String>` preserves the
/// absent case for the injected Rune meta without inventing a dialect table.
// consumers: the future structure-editor view and its driver-meta adapter;
// delete this allow when both land.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreateTableSchemaMeta {
    pub namespace_ensure: Option<String>,
    pub database_field_type: Option<String>,
}

impl CreateTableSchemaMeta {
    /// Convenience constructor for callers adapting a driver's meta record.
    // consumers: the future structure-editor view and its driver-meta adapter;
    // delete this allow when both land.
    #[allow(dead_code)]
    pub fn new(namespace_ensure: Option<&str>, database_field_type: Option<&str>) -> Self {
        Self {
            namespace_ensure: namespace_ensure.map(str::to_owned),
            database_field_type: database_field_type.map(str::to_owned),
        }
    }
}

/// Port of `ResolveCreateTableSchemaParams`. `None` is JS `null`/`undefined`;
/// `Some(String::new())` is `''` and must survive as a value.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolveCreateTableSchemaParams {
    pub current_database: Option<String>,
    pub context_schema: Option<String>,
}

impl ResolveCreateTableSchemaParams {
    /// Convenience constructor over borrowed strings.
    // consumers: the future structure-editor view; delete this allow when it lands.
    #[allow(dead_code)]
    pub fn new(current_database: Option<&str>, context_schema: Option<&str>) -> Self {
        Self {
            current_database: current_database.map(str::to_owned),
            context_schema: context_schema.map(str::to_owned),
        }
    }
}

/// Port of `resolveCreateTableSchemaFromMeta(meta, databaseType, params)`.
// consumers: resolve_create_table_schema and the future structure-editor view;
// delete this allow when both are gone.
#[allow(dead_code)]
pub fn resolve_create_table_schema_from_meta(
    meta: Option<&CreateTableSchemaMeta>,
    database_type: &str,
    params: &ResolveCreateTableSchemaParams,
) -> Option<String> {
    let Some(meta) = meta else {
        if database_type == NAMESPACE_POSTGRESQL {
            return Some(
                params
                    .context_schema
                    .clone()
                    .unwrap_or_else(|| DEFAULT_PG_SCHEMA.to_owned()),
            );
        }
        return params
            .context_schema
            .clone()
            .or_else(|| params.current_database.clone());
    };

    // React: meta.namespaceEnsure ?? (databaseType === 'postgresql'
    // ? 'postgresql' : 'default-sql').
    let namespace_ensure = meta.namespace_ensure.as_deref().unwrap_or_else(|| {
        if database_type == NAMESPACE_POSTGRESQL {
            NAMESPACE_POSTGRESQL
        } else {
            NAMESPACE_DEFAULT_SQL
        }
    });
    if namespace_ensure == NAMESPACE_POSTGRESQL {
        // A PG schema namespace — never the logical database name.
        return Some(
            params
                .context_schema
                .clone()
                .unwrap_or_else(|| DEFAULT_PG_SCHEMA.to_owned()),
        );
    }

    if meta.database_field_type.as_deref() == Some(DATABASE_FIELD_PATH) {
        return None;
    }

    params.current_database.clone()
}

/// Port of `resolveCreateTableSchema(databaseType, params)`. The third argument
/// stands in for React's `DB_REGISTRY[databaseType]` lookup (see module docs for
/// the removal stage).
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
pub fn resolve_create_table_schema(
    database_type: &str,
    params: &ResolveCreateTableSchemaParams,
    mut meta_resolver: impl FnMut(&str) -> Option<CreateTableSchemaMeta>,
) -> Option<String> {
    let meta = meta_resolver(database_type);
    resolve_create_table_schema_from_meta(meta.as_ref(), database_type, params)
}
