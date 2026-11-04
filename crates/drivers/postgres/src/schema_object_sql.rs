//! PostgreSQL dialect SQL for the shared schema-object commands.
//!
//! The `list_objects` / `get_object_ddl` / `list_privileges` commands are
//! implemented once in the driver API; the SQL is PostgreSQL knowledge and
//! therefore lives here, next to the driver that runs it.

use datazen_driver_api::schema_object_sql::{ObjectDdlRequest, SchemaObjectSql};
use datazen_driver_api::schema_objects::{sql_string, ObjectKind};

/// PostgreSQL's dialect SQL provider.
pub struct PostgresSchemaObjectSql;

impl SchemaObjectSql for PostgresSchemaObjectSql {
    fn list_objects_sql(&self, kind: ObjectKind) -> Option<String> {
        match kind {
            ObjectKind::Function => Some(
                "SELECT n.nspname AS schema, p.proname AS name, \
                    pg_get_function_identity_arguments(p.oid) AS signature \
             FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
             WHERE n.nspname NOT IN ('pg_catalog','information_schema') \
               AND p.prokind = 'f' \
             ORDER BY 1, 2, 3"
                    .into(),
            ),
            ObjectKind::Procedure => Some(
                "SELECT n.nspname AS schema, p.proname AS name, \
                    pg_get_function_identity_arguments(p.oid) AS signature \
             FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
             WHERE n.nspname NOT IN ('pg_catalog','information_schema') \
               AND p.prokind = 'p' \
             ORDER BY 1, 2, 3"
                    .into(),
            ),
            ObjectKind::Trigger => Some(
                "SELECT DISTINCT event_object_schema AS schema, trigger_name AS name, \
                    event_object_schema AS target_schema, event_object_table AS target_name \
             FROM information_schema.triggers \
             ORDER BY 1, 2, 4"
                    .into(),
            ),
            ObjectKind::View => Some(
                "SELECT schemaname AS schema, viewname AS name \
             FROM pg_views \
             WHERE schemaname NOT IN ('pg_catalog','information_schema') \
             ORDER BY 1, 2"
                    .into(),
            ),
            ObjectKind::Sequence => Some(
                "SELECT schemaname AS schema, sequencename AS name \
             FROM pg_sequences \
             WHERE schemaname NOT IN ('pg_catalog','information_schema') \
             ORDER BY 1, 2"
                    .into(),
            ),
            ObjectKind::Type => Some(
                "SELECT n.nspname AS schema, t.typname AS name \
             FROM pg_type t \
             JOIN pg_namespace n ON n.oid = t.typnamespace \
             LEFT JOIN pg_class type_rel ON type_rel.oid = t.typrelid \
             WHERE n.nspname NOT IN ('pg_catalog','information_schema') \
               AND (t.typtype IN ('e','d','r') OR (t.typtype = 'c' AND type_rel.relkind = 'c')) \
             ORDER BY 1, 2"
                    .into(),
            ),
            _ => None,
        }
    }

    fn object_ddl_sql(&self, request: &ObjectDdlRequest<'_>) -> Option<String> {
        let ObjectDdlRequest {
            kind,
            name,
            schema,
            signature,
            target_schema,
            target_name,
        } = *request;
        match kind {
            ObjectKind::View => Some(format!(
                "SELECT pg_get_viewdef(c.oid, true) AS ddl \
             FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE c.relname = {} AND n.nspname = {}",
                sql_string(name),
                sql_string(schema.unwrap_or("public")),
            )),
            ObjectKind::Function | ObjectKind::Procedure => {
                let prokind = if kind == ObjectKind::Function {
                    "f"
                } else {
                    "p"
                };
                let signature_filter = signature
                    .map(|value| {
                        format!(
                            " AND pg_get_function_identity_arguments(p.oid) = {}",
                            sql_string(value)
                        )
                    })
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
            ObjectKind::Trigger => Some(format!(
                "SELECT pg_get_triggerdef(t.oid, true) AS ddl \
             FROM pg_trigger t JOIN pg_class c ON c.oid = t.tgrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE t.tgname = {} AND n.nspname = {}",
                sql_string(name),
                sql_string(schema.unwrap_or("public")),
            ))
            .map(|sql| {
                let mut sql = sql;
                if let Some(target_schema) = target_schema.or(schema) {
                    sql.push_str(&format!(" AND n.nspname = {}", sql_string(target_schema)));
                }
                if let Some(target_name) = target_name {
                    sql.push_str(&format!(" AND c.relname = {}", sql_string(target_name)));
                }
                sql
            }),
            ObjectKind::Sequence => {
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
            ObjectKind::Type => {
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
            _ => None,
        }
    }

    fn list_privileges_sql(&self) -> Option<String> {
        Some(
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
        )
    }
}
