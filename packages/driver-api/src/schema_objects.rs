//! Dialect SQL for views, routines, triggers, and privilege listings.
//!
//! Dialect SQL helpers used by driver `list_objects` / `get_object_ddl` /
//! `list_privileges` commands. Host must not execute these SQL strings directly.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObjectKind {
    Table,
    View,
    Function,
    Procedure,
    Trigger,
    Sequence,
    Type,
}

impl ObjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::View => "view",
            Self::Function => "function",
            Self::Procedure => "procedure",
            Self::Trigger => "trigger",
            Self::Sequence => "sequence",
            Self::Type => "type",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "table" => Some(Self::Table),
            "view" => Some(Self::View),
            "function" => Some(Self::Function),
            "procedure" => Some(Self::Procedure),
            "trigger" => Some(Self::Trigger),
            "sequence" => Some(Self::Sequence),
            "type" => Some(Self::Type),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseObject {
    pub kind: String,
    pub schema: Option<String>,
    pub name: String,
    /// PostgreSQL routine identity arguments. This disambiguates overloaded
    /// functions/procedures without putting SQL syntax into `name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// Schema/name of the relation a trigger is attached to. A trigger name
    /// is only unique within its relation on some supported engines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_name: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PrivilegeGrant {
    pub grantee: String,
    pub object_schema: Option<String>,
    pub object_name: String,
    pub privilege: String,
}

/// List-query SQL for routines/triggers. `None` when the dialect has no objects.
pub fn list_objects_sql(db_type: &str, kind: ObjectKind) -> Option<String> {
    let family = dialect_family(db_type);
    match (family, kind) {
        ("postgresql", ObjectKind::Function) => Some(
            "SELECT n.nspname AS schema, p.proname AS name, \
                    pg_get_function_identity_arguments(p.oid) AS signature \
             FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
             WHERE n.nspname NOT IN ('pg_catalog','information_schema') \
               AND p.prokind = 'f' \
             ORDER BY 1, 2, 3"
                .into(),
        ),
        ("postgresql", ObjectKind::Procedure) => Some(
            "SELECT n.nspname AS schema, p.proname AS name, \
                    pg_get_function_identity_arguments(p.oid) AS signature \
             FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
             WHERE n.nspname NOT IN ('pg_catalog','information_schema') \
               AND p.prokind = 'p' \
             ORDER BY 1, 2, 3"
                .into(),
        ),
        ("postgresql", ObjectKind::Trigger) => Some(
            "SELECT DISTINCT event_object_schema AS schema, trigger_name AS name, \
                    event_object_schema AS target_schema, event_object_table AS target_name \
             FROM information_schema.triggers \
             ORDER BY 1, 2, 4"
                .into(),
        ),
        ("postgresql", ObjectKind::View) => Some(
            "SELECT schemaname AS schema, viewname AS name \
             FROM pg_views \
             WHERE schemaname NOT IN ('pg_catalog','information_schema') \
             ORDER BY 1, 2"
                .into(),
        ),
        ("postgresql", ObjectKind::Sequence) => Some(
            "SELECT schemaname AS schema, sequencename AS name \
             FROM pg_sequences \
             WHERE schemaname NOT IN ('pg_catalog','information_schema') \
             ORDER BY 1, 2"
                .into(),
        ),
        ("postgresql", ObjectKind::Type) => Some(
            "SELECT n.nspname AS schema, t.typname AS name \
             FROM pg_type t \
             JOIN pg_namespace n ON n.oid = t.typnamespace \
             LEFT JOIN pg_class type_rel ON type_rel.oid = t.typrelid \
             WHERE n.nspname NOT IN ('pg_catalog','information_schema') \
               AND (t.typtype IN ('e','d','r') OR (t.typtype = 'c' AND type_rel.relkind = 'c')) \
             ORDER BY 1, 2"
                .into(),
        ),
        ("mysql", ObjectKind::Function) => Some(
            "SELECT ROUTINE_SCHEMA AS `schema`, ROUTINE_NAME AS name \
             FROM information_schema.ROUTINES \
             WHERE ROUTINE_SCHEMA = DATABASE() AND ROUTINE_TYPE = 'FUNCTION' \
             ORDER BY 1, 2"
                .into(),
        ),
        ("mysql", ObjectKind::Procedure) => Some(
            "SELECT ROUTINE_SCHEMA AS `schema`, ROUTINE_NAME AS name \
             FROM information_schema.ROUTINES \
             WHERE ROUTINE_SCHEMA = DATABASE() AND ROUTINE_TYPE = 'PROCEDURE' \
             ORDER BY 1, 2"
                .into(),
        ),
        ("mysql", ObjectKind::Trigger) => Some(
            "SELECT TRIGGER_SCHEMA AS `schema`, TRIGGER_NAME AS name, \
                    EVENT_OBJECT_SCHEMA AS target_schema, EVENT_OBJECT_TABLE AS target_name \
             FROM information_schema.TRIGGERS \
             WHERE TRIGGER_SCHEMA = DATABASE() \
             ORDER BY 1, 2, 4"
                .into(),
        ),
        ("mysql", ObjectKind::View) => Some(
            "SELECT TABLE_SCHEMA AS `schema`, TABLE_NAME AS name \
             FROM information_schema.VIEWS WHERE TABLE_SCHEMA = DATABASE() \
             ORDER BY 1, 2"
                .into(),
        ),
        ("sqlite", ObjectKind::View) => Some(
            "SELECT NULL AS schema, name FROM sqlite_master \
             WHERE type = 'view' ORDER BY name"
                .into(),
        ),
        ("sqlite", ObjectKind::Trigger) => Some(
            "SELECT NULL AS schema, name, NULL AS target_schema, tbl_name AS target_name \
                 FROM sqlite_master WHERE type = 'trigger' ORDER BY name"
                .into(),
        ),
        ("duckdb", ObjectKind::Trigger) => Some(
            "SELECT NULL AS schema, trigger_name AS name \
             FROM information_schema.triggers \
             ORDER BY 1, 2"
                .into(),
        ),
        ("duckdb", ObjectKind::Sequence) => Some(
            "SELECT sequence_schema AS schema, sequence_name AS name \
             FROM information_schema.sequences \
             WHERE sequence_schema NOT IN ('information_schema') \
             ORDER BY 1, 2"
                .into(),
        ),
        ("sqlserver", ObjectKind::View) => Some(
            "SELECT s.name AS [schema], v.name AS name \
             FROM sys.views v JOIN sys.schemas s ON s.schema_id = v.schema_id \
             WHERE v.is_ms_shipped = 0 \
             ORDER BY 1, 2"
                .into(),
        ),
        ("sqlserver", ObjectKind::Function) => Some(
            "SELECT s.name AS [schema], o.name AS name \
             FROM sys.objects o JOIN sys.schemas s ON s.schema_id = o.schema_id \
             WHERE o.type IN ('FN','FS','FT','IF','TF') \
             ORDER BY 1, 2"
                .into(),
        ),
        ("sqlserver", ObjectKind::Procedure) => Some(
            "SELECT s.name AS [schema], o.name AS name \
             FROM sys.objects o JOIN sys.schemas s ON s.schema_id = o.schema_id \
             WHERE o.type IN ('P','PC') \
             ORDER BY 1, 2"
                .into(),
        ),
        ("sqlserver", ObjectKind::Trigger) => Some(
            // `sys.triggers` has no `schema_id` column (error 207); the owning
            // schema is reached through `sys.objects`.
            "SELECT s.name AS [schema], t.name AS name \
             FROM sys.triggers t \
             JOIN sys.objects o ON o.object_id = t.object_id \
             JOIN sys.schemas s ON s.schema_id = o.schema_id \
             WHERE t.is_ms_shipped = 0 \
             ORDER BY 1, 2"
                .into(),
        ),
        ("sqlserver", ObjectKind::Sequence) => Some(
            "SELECT s.name AS [schema], q.name AS name \
             FROM sys.sequences q \
             JOIN sys.schemas s ON s.schema_id = q.schema_id \
             ORDER BY 1, 2"
                .into(),
        ),
        ("sqlserver", ObjectKind::Type) => Some(
            "SELECT s.name AS [schema], t.name AS name \
             FROM sys.types t \
             JOIN sys.schemas s ON s.schema_id = t.schema_id \
             WHERE t.is_user_defined = 1 \
             ORDER BY 1, 2"
                .into(),
        ),
        _ => None,
    }
}

/// Build an object-list query for a specific database when the dialect needs
/// an explicit catalog filter. PostgreSQL chooses the database through
/// `query_at`; MySQL exposes all catalogs through `information_schema`, so its
/// routine/trigger predicates must name the selected database instead of
/// relying on the connection's default `DATABASE()` value.
pub fn list_objects_sql_for_database(
    db_type: &str,
    kind: ObjectKind,
    database: Option<&str>,
) -> Option<String> {
    let sql = list_objects_sql(db_type, kind)?;
    let Some(database) = database.filter(|value| !value.trim().is_empty()) else {
        return Some(sql);
    };
    if dialect_family(db_type) != "mysql" {
        return Some(sql);
    }

    Some(sql.replace("DATABASE()", &sql_string(database)))
}

pub fn object_ddl_sql(
    db_type: &str,
    kind: ObjectKind,
    name: &str,
    schema: Option<&str>,
) -> Option<String> {
    object_ddl_sql_with_metadata(db_type, kind, name, schema, None, None, None)
}

/// Build a metadata lookup for an object using its full identity.
///
/// `signature` is the PostgreSQL identity-argument string for routines.
/// `target_schema`/`target_name` disambiguate triggers attached to relations.
/// All values are encoded as SQL string literals; identifiers used in DDL
/// statements are quoted with the target dialect's rules.
pub fn object_ddl_sql_with_metadata(
    db_type: &str,
    kind: ObjectKind,
    name: &str,
    schema: Option<&str>,
    signature: Option<&str>,
    target_schema: Option<&str>,
    target_name: Option<&str>,
) -> Option<String> {
    let family = dialect_family(db_type);
    let ident = quote_ident(family, name);
    let schema_ident = schema
        .filter(|s| !s.is_empty())
        .map(|s| quote_ident(family, s));
    let qualified = match &schema_ident {
        Some(s) => format!("{s}.{ident}"),
        None => ident.clone(),
    };
    match (family, kind) {
        ("postgresql", ObjectKind::View) => Some(format!(
            "SELECT pg_get_viewdef(c.oid, true) AS ddl \
             FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE c.relname = {} AND n.nspname = {}",
            sql_string(name),
            sql_string(schema.unwrap_or("public")),
        )),
        ("postgresql", ObjectKind::Function | ObjectKind::Procedure) => {
            let prokind = if kind == ObjectKind::Function { "f" } else { "p" };
            let signature_filter = signature
                .map(|value| format!(" AND pg_get_function_identity_arguments(p.oid) = {}", sql_string(value)))
                .unwrap_or_default();
            Some(format!(
            "SELECT pg_get_functiondef(p.oid) AS ddl \
             FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
             WHERE p.proname = {} AND n.nspname = {} AND p.prokind = '{}'{}",
            sql_string(name),
            sql_string(schema.unwrap_or("public")),
            prokind,
            signature_filter,
        ))
        }
        ("postgresql", ObjectKind::Trigger) => Some(format!(
            "SELECT pg_get_triggerdef(t.oid, true) AS ddl \
             FROM pg_trigger t JOIN pg_class c ON c.oid = t.tgrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE t.tgname = {} AND n.nspname = {}",
            sql_string(name),
            sql_string(schema.unwrap_or("public")),
        )).map(|sql| {
            let mut sql = sql;
            if let Some(target_schema) = target_schema.or(schema) {
                sql.push_str(&format!(" AND n.nspname = {}", sql_string(target_schema)));
            }
            if let Some(target_name) = target_name {
                sql.push_str(&format!(" AND c.relname = {}", sql_string(target_name)));
            }
            sql
        }),
        ("postgresql", ObjectKind::Sequence) => {
            let schema_str = sql_string(schema.unwrap_or("public"));
            let name_str = sql_string(name);
            // Keep this lookup server-side and return one catalog-generated
            // script. pg_sequences exposes the complete mutable sequence
            // definition, while pg_depend/pg_attribute supplies OWNED BY.
            // The exact schema/name predicates make missing and ambiguous
            // catalog rows fail closed in extract_object_ddl_checked.
            Some(format!(
                "SELECT 'CREATE SEQUENCE ' || quote_ident(ns.nspname) || '.' || quote_ident(c.relname) \
                 || ' AS ' || format_type(s.seqtypid, NULL) \
                 || ' INCREMENT BY ' || s.seqincrement \
                 || ' MINVALUE ' || s.seqmin \
                 || ' MAXVALUE ' || s.seqmax \
                 || ' START WITH ' || s.seqstart \
                 || ' CACHE ' || s.seqcache \
                 || CASE WHEN s.seqcycle THEN ' CYCLE' ELSE ' NO CYCLE' END \
                 || ';' \
                 || CASE WHEN owner_ns.nspname IS NULL THEN '' \
                    ELSE ' ALTER SEQUENCE ' || quote_ident(ns.nspname) || '.' || quote_ident(c.relname) \
                      || ' OWNED BY ' || quote_ident(owner_ns.nspname) || '.' \
                      || quote_ident(owner_cls.relname) || '.' || quote_ident(owner_att.attname) || ';' END AS ddl \
                 FROM pg_class c \
                 JOIN pg_namespace ns ON ns.oid = c.relnamespace \
                 JOIN pg_sequence s ON s.seqrelid = c.oid \
                 LEFT JOIN pg_depend dep ON dep.objid = c.oid \
                   AND dep.classid = 'pg_class'::regclass \
                   AND dep.refclassid = 'pg_class'::regclass \
                   AND dep.deptype IN ('a', 'i') \
                 LEFT JOIN pg_class owner_cls ON owner_cls.oid = dep.refobjid \
                 LEFT JOIN pg_namespace owner_ns ON owner_ns.oid = owner_cls.relnamespace \
                 LEFT JOIN pg_attribute owner_att ON owner_att.attrelid = owner_cls.oid \
                   AND owner_att.attnum = dep.refobjsubid \
                   AND NOT owner_att.attisdropped \
                 WHERE c.relkind = 'S' AND ns.nspname = {schema_str} AND c.relname = {name_str}"
            ))
        }
        ("postgresql", ObjectKind::Type) => {
            let schema_str = sql_string(schema.unwrap_or("public"));
            let name_str = sql_string(name);
            Some(format!(
                "SELECT CASE t.typtype \
                   WHEN 'e' THEN 'CREATE TYPE ' || quote_ident(n.nspname) || '.' || quote_ident(t.typname) \
                     || ' AS ENUM (' || COALESCE((SELECT string_agg(quote_literal(e.enumlabel), ', ' ORDER BY e.enumsortorder) FROM pg_enum e WHERE e.enumtypid = t.oid), '') || ');' \
                   WHEN 'd' THEN 'CREATE DOMAIN ' || quote_ident(n.nspname) || '.' || quote_ident(t.typname) \
                     || ' AS ' || pg_catalog.format_type(t.typbasetype, t.typtypmod) \
                     || CASE WHEN domain_coll.oid IS NULL THEN '' ELSE ' COLLATE ' || quote_ident(domain_coll_ns.nspname) || '.' || quote_ident(domain_coll.collname) END \
                     || COALESCE(' DEFAULT ' || pg_get_expr(t.typdefaultbin, 0), '') \
                     || CASE WHEN t.typnotnull AND NOT EXISTS (SELECT 1 FROM pg_constraint con WHERE con.contypid = t.oid AND con.contype = 'n') THEN ' NOT NULL' ELSE '' END \
                     || COALESCE(' ' || (SELECT string_agg('CONSTRAINT ' || quote_ident(con.conname) || ' ' || pg_get_constraintdef(con.oid), ' ' ORDER BY con.oid) FROM pg_constraint con WHERE con.contypid = t.oid), '') \
                     || ';' \
                   WHEN 'c' THEN 'CREATE TYPE ' || quote_ident(n.nspname) || '.' || quote_ident(t.typname) \
                     || ' AS (' || COALESCE((SELECT string_agg(quote_ident(a.attname) || ' ' || pg_catalog.format_type(a.atttypid, a.atttypmod) \
                       || CASE WHEN attr_coll.oid IS NULL THEN '' ELSE ' COLLATE ' || quote_ident(attr_coll_ns.nspname) || '.' || quote_ident(attr_coll.collname) END, ', ' ORDER BY a.attnum) \
                       FROM pg_attribute a \
                       LEFT JOIN pg_collation attr_coll ON attr_coll.oid = NULLIF(a.attcollation, 0) \
                       LEFT JOIN pg_namespace attr_coll_ns ON attr_coll_ns.oid = attr_coll.collnamespace \
                       WHERE a.attrelid = t.typrelid AND a.attnum > 0 AND NOT a.attisdropped), '') || ');' \
                   WHEN 'r' THEN 'CREATE TYPE ' || quote_ident(n.nspname) || '.' || quote_ident(t.typname) \
                     || ' AS RANGE (SUBTYPE = ' || pg_catalog.format_type(r.rngsubtype, NULL) \
                     || CASE WHEN range_opclass.oid IS NULL THEN '' ELSE ', SUBTYPE_OPCLASS = ' || quote_ident(range_opclass_ns.nspname) || '.' || quote_ident(range_opclass.opcname) END \
                     || CASE WHEN range_coll.oid IS NULL THEN '' ELSE ', COLLATION = ' || quote_ident(range_coll_ns.nspname) || '.' || quote_ident(range_coll.collname) END \
                     || CASE WHEN canonical.oid IS NULL THEN '' ELSE ', CANONICAL = ' || quote_ident(canonical_ns.nspname) || '.' || quote_ident(canonical.proname) END \
                     || CASE WHEN subtype_diff.oid IS NULL THEN '' ELSE ', SUBTYPE_DIFF = ' || quote_ident(subtype_diff_ns.nspname) || '.' || quote_ident(subtype_diff.proname) END \
                     || CASE WHEN multirange.oid IS NULL THEN '' ELSE ', MULTIRANGE_TYPE_NAME = ' || quote_ident(multirange_ns.nspname) || '.' || quote_ident(multirange.typname) END \
                     || ');' \
                 END AS ddl \
                 FROM pg_type t \
                 JOIN pg_namespace n ON n.oid = t.typnamespace \
                 LEFT JOIN pg_range r ON r.rngtypid = t.oid \
                 LEFT JOIN pg_class type_rel ON type_rel.oid = t.typrelid \
                 LEFT JOIN pg_collation domain_coll ON domain_coll.oid = NULLIF(t.typcollation, 0) \
                 LEFT JOIN pg_namespace domain_coll_ns ON domain_coll_ns.oid = domain_coll.collnamespace \
                 LEFT JOIN pg_opclass range_opclass ON range_opclass.oid = r.rngsubopc \
                 LEFT JOIN pg_namespace range_opclass_ns ON range_opclass_ns.oid = range_opclass.opcnamespace \
                 LEFT JOIN pg_collation range_coll ON range_coll.oid = NULLIF(r.rngcollation, 0) \
                 LEFT JOIN pg_namespace range_coll_ns ON range_coll_ns.oid = range_coll.collnamespace \
                 LEFT JOIN pg_proc canonical ON canonical.oid = r.rngcanonical \
                 LEFT JOIN pg_namespace canonical_ns ON canonical_ns.oid = canonical.pronamespace \
                 LEFT JOIN pg_proc subtype_diff ON subtype_diff.oid = r.rngsubdiff \
                 LEFT JOIN pg_namespace subtype_diff_ns ON subtype_diff_ns.oid = subtype_diff.pronamespace \
                 LEFT JOIN pg_type multirange ON multirange.oid = NULLIF(to_jsonb(r)->>'rngmultitypid', '')::oid \
                 LEFT JOIN pg_namespace multirange_ns ON multirange_ns.oid = multirange.typnamespace \
                 WHERE n.nspname = {schema_str} AND t.typname = {name_str} \
                   AND (t.typtype IN ('e','d','r') OR (t.typtype = 'c' AND type_rel.relkind = 'c'))"
            ))
        }
        ("mysql", ObjectKind::Table) => Some(format!("SHOW CREATE TABLE {qualified}")),
        ("mysql", ObjectKind::View) => Some(format!(
            "SELECT TABLE_SCHEMA AS view_schema, VIEW_DEFINITION AS ddl, DEFINER AS view_definer, \
             SECURITY_TYPE AS view_security_type, CHECK_OPTION AS view_check_option, \
             CHARACTER_SET_CLIENT AS view_character_set_client, \
             COLLATION_CONNECTION AS view_collation_connection \
             FROM information_schema.VIEWS \
             WHERE TABLE_SCHEMA = COALESCE(NULLIF({}, ''), DATABASE()) AND TABLE_NAME = {}",
            sql_string(schema.filter(|s| !s.is_empty()).unwrap_or("")),
            sql_string(name),
        )),
        ("mysql", ObjectKind::Function) => Some(format!(
            "SHOW CREATE FUNCTION {qualified}"
        )),
        ("mysql", ObjectKind::Procedure) => Some(format!(
            "SHOW CREATE PROCEDURE {qualified}"
        )),
        ("mysql", ObjectKind::Trigger) => Some(format!("SHOW CREATE TRIGGER {qualified}")),
        ("sqlite", ObjectKind::Table) => Some(format!(
            "SELECT sql AS ddl FROM sqlite_master WHERE type = 'table' AND name = {}",
            sql_string(name),
        )),
        ("sqlite", ObjectKind::View) => Some(format!(
            "WITH view_source AS ( \
             SELECT sql, instr(lower(replace(replace(replace(sql, char(13), ' '), char(10), ' '), char(9), ' ')), ' as ') AS as_marker \
             FROM sqlite_master WHERE type = 'view' AND name = {} \
             ) \
             SELECT trim(substr(sql, as_marker + 3 \
                 + length(substr(sql, as_marker + 3)) \
                 - length(ltrim(substr(sql, as_marker + 3), char(9) || char(10) || char(13) || ' '))), \
                 char(9) || char(10) || char(13) || ' ') AS ddl \
             FROM view_source",
            sql_string(name),
        )),
        ("sqlite", ObjectKind::Trigger) => Some(format!(
            "SELECT sql AS ddl FROM sqlite_master WHERE type = 'trigger' AND name = {}",
            sql_string(name),
        )),
        ("duckdb", ObjectKind::Trigger) => Some(format!(
            "SELECT sql AS ddl FROM information_schema.triggers WHERE trigger_name = {} LIMIT 1",
            sql_string(name),
        )),
        ("duckdb", ObjectKind::Sequence) => Some(format!(
            "SELECT sql AS ddl FROM duckdb_sequences() WHERE sequence_name = {} LIMIT 1",
            sql_string(name),
        )),
        (
            "sqlserver",
            ObjectKind::View | ObjectKind::Function | ObjectKind::Procedure | ObjectKind::Trigger,
        ) => {
            let schema_str = schema.filter(|s| !s.is_empty()).unwrap_or("dbo");
            let target_ident = format!(
                "{}.{}",
                quote_ident("sqlserver", schema_str),
                quote_ident("sqlserver", name)
            );
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
        _ => {
            let _ = qualified;
            None
        }
    }
}

/// Build a MySQL `SHOW CREATE VIEW` query used to capture the definition
/// options omitted by `information_schema.VIEWS.VIEW_DEFINITION`.
pub fn mysql_show_create_view_sql(name: &str, schema: Option<&str>) -> String {
    let name = quote_ident("mysql", name);
    let qualified = schema
        .filter(|schema| !schema.is_empty())
        .map(|schema| format!("{}.{}", quote_ident("mysql", schema), name))
        .unwrap_or(name);
    format!("SHOW CREATE VIEW {qualified}")
}

pub fn list_privileges_sql(db_type: &str) -> Option<String> {
    match dialect_family(db_type) {
        "postgresql" => Some(
            "SELECT rolname AS grantee, '*' AS schema, '*' AS name, \
               CASE WHEN rolsuper THEN 'SUPERUSER' \
                    WHEN rolcreatedb THEN 'CREATEDB' \
                    WHEN rolcreaterole THEN 'CREATEROLE' \
                    ELSE 'LOGIN' END AS privilege \
             FROM pg_roles WHERE rolname NOT LIKE 'pg_%' AND rolcanlogin \
             UNION ALL \
             SELECT grantee, table_schema AS schema, table_name AS name, privilege_type AS privilege \
             FROM information_schema.role_table_grants \
             WHERE table_schema NOT IN ('pg_catalog','information_schema') \
             ORDER BY 1, 2, 3 LIMIT 500"
                .into(),
        ),
        "mysql" => Some(
            "SELECT GRANTEE AS grantee, '*' AS table_schema, '*' AS name, PRIVILEGE_TYPE AS privilege \
             FROM information_schema.USER_PRIVILEGES \
             UNION ALL \
             SELECT GRANTEE AS grantee, TABLE_SCHEMA AS table_schema, TABLE_NAME AS name, PRIVILEGE_TYPE AS privilege \
             FROM information_schema.TABLE_PRIVILEGES \
             WHERE TABLE_SCHEMA = DATABASE() \
             ORDER BY 1, 2, 3 LIMIT 500"
                .into(),
        ),
        "sqlserver" => Some(
            "SELECT pr.name AS grantee, \
                    CASE WHEN p.class = 0 THEN '<server>' ELSE OBJECT_SCHEMA_NAME(p.major_id) END AS [schema], \
                    CASE WHEN p.class = 0 THEN '*' ELSE OBJECT_NAME(p.major_id) END AS name, \
                    p.permission_name AS privilege \
             FROM sys.database_permissions p \
             JOIN sys.database_principals pr ON p.grantee_principal_id = pr.principal_id \
             WHERE pr.type IN ('S','U','G','R') \
             ORDER BY 1, 2, 3 \
             OFFSET 0 ROWS FETCH NEXT 500 ROWS ONLY"
                .into(),
        ),
        _ => None,
    }
}

pub fn dialect_family(db_type: &str) -> &'static str {
    match db_type.to_ascii_lowercase().as_str() {
        "postgresql" | "postgres" | "cockroach" | "cloudberry" | "questdb" => "postgresql",
        "mysql" | "mariadb" | "tidb" | "doris" | "starrocks" | "manticore" | "ob_oracle" => "mysql",
        "sqlite" | "rqlite" | "turso" => "sqlite",
        "duckdb" => "duckdb",
        "sqlserver" | "mssql" => "sqlserver",
        _ => "other",
    }
}

fn quote_ident(family: &str, name: &str) -> String {
    match family {
        "mysql" => format!("`{}`", name.replace('`', "``")),
        "sqlserver" => format!("[{}]", name.replace(']', "]]")),
        _ => format!("\"{}\"", name.replace('"', "\"\"")),
    }
}

fn sql_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_kind_parse() {
        assert_eq!(ObjectKind::parse("FUNCTION"), Some(ObjectKind::Function));
        assert_eq!(ObjectKind::parse("nope"), None);
    }

    #[test]
    fn object_kind_parses_table_and_view() {
        assert_eq!(ObjectKind::parse("table"), Some(ObjectKind::Table));
        assert_eq!(ObjectKind::parse("view"), Some(ObjectKind::View));
        assert_eq!(ObjectKind::Table.as_str(), "table");
        assert_eq!(ObjectKind::View.as_str(), "view");

        assert!(object_ddl_sql("mysql", ObjectKind::Table, "users", None).is_some());
        assert!(object_ddl_sql(
            "postgresql",
            ObjectKind::View,
            "active_users",
            Some("public")
        )
        .is_some());
        assert!(object_ddl_sql("sqlite", ObjectKind::Table, "users", None).is_some());
    }

    #[test]
    fn dialect_aliases_map_to_families() {
        assert_eq!(dialect_family("cockroach"), "postgresql");
        assert_eq!(dialect_family("tidb"), "mysql");
        assert_eq!(dialect_family("redis"), "other");
        assert!(list_objects_sql("cockroach", ObjectKind::Function).is_some());
        assert!(list_objects_sql("tidb", ObjectKind::Procedure).is_some());
        assert!(list_objects_sql("redis", ObjectKind::Function).is_none());
        // PG/MySQL reuse engines map to their protocol family.
        assert_eq!(dialect_family("cloudberry"), "postgresql");
        assert_eq!(dialect_family("questdb"), "postgresql");
        assert_eq!(dialect_family("doris"), "mysql");
        assert_eq!(dialect_family("starrocks"), "mysql");
        assert_eq!(dialect_family("manticore"), "mysql");
        assert_eq!(dialect_family("ob_oracle"), "mysql");
        // SQLite-compatible drivers map to the sqlite family.
        assert_eq!(dialect_family("rqlite"), "sqlite");
        assert_eq!(dialect_family("turso"), "sqlite");
        assert!(list_objects_sql("rqlite", ObjectKind::Trigger).is_some());
        assert!(list_objects_sql("turso", ObjectKind::Trigger).is_some());
        // DuckDB has its own sequence/trigger family.
        assert_eq!(dialect_family("duckdb"), "duckdb");
        assert!(list_objects_sql("duckdb", ObjectKind::Trigger).is_some());
        assert!(list_objects_sql("duckdb", ObjectKind::Sequence).is_some());
        assert!(object_ddl_sql("duckdb", ObjectKind::Sequence, "seq", None).is_some());
    }

    #[test]
    fn postgres_routine_catalog_and_ddl_use_overload_identity() {
        let list = list_objects_sql("postgresql", ObjectKind::Function).unwrap();
        assert!(list.contains("pg_get_function_identity_arguments"));
        assert!(list.contains("p.prokind = 'f'"));

        let ddl = object_ddl_sql_with_metadata(
            "postgresql",
            ObjectKind::Function,
            "lookup",
            Some("app"),
            Some("integer, text"),
            None,
            None,
        )
        .unwrap();
        assert!(ddl.contains("pg_get_function_identity_arguments"));
        assert!(ddl.contains("'integer, text'"));
        assert!(ddl.contains("p.prokind = 'f'"));
    }

    #[test]
    fn mysql_object_catalog_can_target_a_database_without_using_connection_default() {
        let sql =
            list_objects_sql_for_database("mysql", ObjectKind::Procedure, Some("manual'catalog"))
                .unwrap();
        assert!(sql.contains("ROUTINE_SCHEMA = 'manual''catalog'"));
        assert!(!sql.contains("DATABASE()"));

        let default_sql =
            list_objects_sql_for_database("mysql", ObjectKind::Function, None).unwrap();
        assert!(default_sql.contains("ROUTINE_SCHEMA = DATABASE()"));
    }

    #[test]
    fn postgres_type_catalog_excludes_table_row_types_and_preserves_type_options() {
        let list = list_objects_sql("postgresql", ObjectKind::Type).unwrap();
        assert!(list.contains("LEFT JOIN pg_class type_rel"));
        assert!(list.contains("type_rel.relkind = 'c'"));
        assert!(list.contains("t.typtype IN ('e','d','r')"));

        let ddl = object_ddl_sql_with_metadata(
            "postgresql",
            ObjectKind::Type,
            "delivery_window",
            Some("app"),
            None,
            None,
            None,
        )
        .unwrap();
        assert!(ddl.contains("pg_enum"));
        assert!(ddl.contains("typcollation"));
        assert!(ddl.contains("attcollation"));
        assert!(ddl.contains("rngsubopc"));
        assert!(ddl.contains("rngcanonical"));
        assert!(ddl.contains("rngsubdiff"));
        assert!(ddl.contains("rngmultitypid"));
        assert!(ddl.contains("con.contypid = t.oid"));
        assert!(ddl.contains("n.nspname = 'app'"));
        assert!(ddl.contains("t.typname = 'delivery_window'"));
    }

    #[test]
    fn ddl_identity_literals_escape_sql_metacharacters() {
        let sql = object_ddl_sql_with_metadata(
            "postgresql",
            ObjectKind::Function,
            "lookup' OR 1=1 --",
            Some("app'\"schema"),
            Some("integer' OR 1=1 --"),
            None,
            None,
        )
        .unwrap();
        assert!(sql.contains("p.proname = 'lookup'' OR 1=1 --'"));
        assert!(sql.contains("n.nspname = 'app''\"schema'"));
        assert!(sql.contains("= 'integer'' OR 1=1 --'"));
    }

    #[test]
    fn trigger_catalog_and_ddl_include_attached_relation() {
        let list = list_objects_sql("mysql", ObjectKind::Trigger).unwrap();
        assert!(list.contains("information_schema.TRIGGERS"));
        assert!(list.contains("target_name"));

        let ddl = object_ddl_sql_with_metadata(
            "postgresql",
            ObjectKind::Trigger,
            "audit_trigger",
            Some("app"),
            None,
            Some("app"),
            Some("orders"),
        )
        .unwrap();
        assert!(ddl.contains("c.relname = 'orders'"));
    }

    #[test]
    fn unsupported_sequence_families_fail_closed() {
        assert!(list_objects_sql("mysql", ObjectKind::Sequence).is_none());
        assert!(list_objects_sql("sqlite", ObjectKind::Sequence).is_none());
        assert!(object_ddl_sql("mysql", ObjectKind::Sequence, "seq", None).is_none());
        assert!(object_ddl_sql("sqlite", ObjectKind::Sequence, "seq", None).is_none());
    }

    #[test]
    fn sqlserver_objects_cover_all_kinds() {
        assert_eq!(dialect_family("sqlserver"), "sqlserver");
        assert_eq!(dialect_family("mssql"), "sqlserver");
        for kind in [
            ObjectKind::View,
            ObjectKind::Function,
            ObjectKind::Procedure,
            ObjectKind::Trigger,
            ObjectKind::Sequence,
            ObjectKind::Type,
        ] {
            let sql = list_objects_sql("sqlserver", kind)
                .unwrap_or_else(|| panic!("sqlserver should list object kind {kind:?}"));
            let catalog = match kind {
                ObjectKind::View => "sys.views",
                ObjectKind::Sequence => "sys.sequences",
                ObjectKind::Type => "sys.types",
                _ => "sys.objects",
            };
            assert!(
                sql.contains(catalog)
                    || (kind == ObjectKind::Trigger && sql.contains("sys.triggers")),
                "sqlserver list should read catalog views for {kind:?}: {sql}"
            );
            // `schema` is a reserved keyword in T-SQL: an unquoted alias is a
            // syntax error (156), so these queries must bracket-quote it.
            assert!(
                !sql.contains("AS schema"),
                "sqlserver list must not alias a column `AS schema` (reserved word) for {kind:?}: {sql}"
            );
        }
        let fn_ddl = object_ddl_sql("sqlserver", ObjectKind::Function, "fn", Some("dbo")).unwrap();
        assert!(fn_ddl.contains("OBJECT_DEFINITION"));
        assert!(fn_ddl.contains("[dbo].[fn]"));
        let view_ddl = object_ddl_sql("sqlserver", ObjectKind::View, "v", Some("dbo")).unwrap();
        assert!(view_ddl.contains("OBJECT_DEFINITION"));
        assert!(view_ddl.contains("[dbo].[v]"));
        // Without schema it falls back to dbo.
        let no_schema = object_ddl_sql("sqlserver", ObjectKind::Procedure, "p", None).unwrap();
        assert!(no_schema.contains("[dbo].[p]"));
        // Sequences/types are reconstructed from catalog metadata.
        let seq_ddl =
            object_ddl_sql("sqlserver", ObjectKind::Sequence, "seq", Some("dbo")).unwrap();
        assert!(seq_ddl.contains("sys.sequences"));
        assert!(seq_ddl.contains("CREATE SEQUENCE"));
        let type_ddl = object_ddl_sql("sqlserver", ObjectKind::Type, "phone", Some("dbo")).unwrap();
        assert!(type_ddl.contains("sys.types"));
        assert!(type_ddl.contains("CREATE TYPE"));
        assert!(list_privileges_sql("sqlserver").is_some());
        assert!(list_privileges_sql("mssql").is_some());
        // SQL Server uses bracket-quoted identifiers.
        assert_eq!(quote_ident("sqlserver", "my table"), "[my table]");
    }

    #[test]
    fn sqlserver_object_ddl_escapes_single_quotes_and_brackets() {
        let ddl_sql = object_ddl_sql(
            "sqlserver",
            ObjectKind::Function,
            "fn'special",
            Some("custom schema"),
        )
        .unwrap();
        assert!(ddl_sql.contains("[custom schema].[fn''special]"));
        assert!(ddl_sql.contains("fn''special"));

        let seq_sql =
            object_ddl_sql("sqlserver", ObjectKind::Sequence, "seq'one", Some("dbo")).unwrap();
        assert!(seq_sql.contains("WHERE q.name = 'seq''one'"));
        assert!(seq_sql.contains("[dbo].[seq'one]"));
    }
}
