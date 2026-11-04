use super::sql_string;

/// Build a PostgreSQL function dependency query. PostgreSQL does not record
/// references from ordinary PL/pgSQL bodies in `pg_depend`, so an empty catalog
/// result alone is not sufficient proof. The driver marks only the exact
/// `BEGIN RETURN NEW; END[;]` trigger passthrough body as dependency-complete;
/// all other function bodies remain incomplete until their dependencies can
/// be enumerated structurally.
pub fn postgres_function_dependencies_sql(
    name: &str,
    schema: Option<&str>,
    signature: Option<&str>,
) -> String {
    let name = sql_string(name);
    let schema_filter = schema
        .map(|schema| format!(" AND proc_ns.nspname = {}", sql_string(schema)))
        .unwrap_or_default();
    let signature_filter = signature
        .map(|signature| {
            format!(
                " AND pg_catalog.pg_get_function_identity_arguments(proc.oid) = {}",
                sql_string(signature)
            )
        })
        .unwrap_or_default();
    format!(
        r#"
        WITH selected_functions AS (
            SELECT proc.oid,
                   pg_catalog.regexp_replace(
                       pg_catalog.upper(pg_catalog.btrim(proc.prosrc)),
                       '[[:space:]]+', '', 'g'
                   )
                       IN ('BEGINRETURNNEW;END', 'BEGINRETURNNEW;END;')
                   AND lang.lanname = 'plpgsql' AS body_dependencies_proven_empty
            FROM pg_catalog.pg_proc AS proc
            JOIN pg_catalog.pg_namespace AS proc_ns ON proc_ns.oid = proc.pronamespace
            JOIN pg_catalog.pg_language AS lang ON lang.oid = proc.prolang
            WHERE proc.proname = {name}
              AND proc.prokind = 'f'{schema_filter}{signature_filter}
        ),
        catalog_dependencies AS (
            SELECT DISTINCT selected.oid AS selected_oid, 'type'::text AS kind,
                   type_ns.nspname AS dependency_schema, referenced_type.typname AS name,
                   NULL::text AS signature
            FROM selected_functions AS selected
            JOIN pg_catalog.pg_depend AS dependency
              ON dependency.classid = 'pg_catalog.pg_proc'::regclass
             AND dependency.objid = selected.oid
             AND dependency.refclassid = 'pg_catalog.pg_type'::regclass
            JOIN pg_catalog.pg_type AS referenced_type ON referenced_type.oid = dependency.refobjid
            JOIN pg_catalog.pg_namespace AS type_ns ON type_ns.oid = referenced_type.typnamespace
            WHERE type_ns.nspname NOT IN ('pg_catalog', 'information_schema')
              AND referenced_type.typtype IN ('e', 'd', 'r', 'c')

            UNION

            SELECT DISTINCT selected.oid AS selected_oid,
                   CASE referenced_relation.relkind
                       WHEN 'v' THEN 'view' WHEN 'm' THEN 'view'
                       WHEN 'S' THEN 'sequence' WHEN 'c' THEN 'type'
                       ELSE 'table'
                   END::text AS kind,
                   relation_ns.nspname AS dependency_schema, referenced_relation.relname AS name,
                   NULL::text AS signature
            FROM selected_functions AS selected
            JOIN pg_catalog.pg_depend AS dependency
              ON dependency.classid = 'pg_catalog.pg_proc'::regclass
             AND dependency.objid = selected.oid
             AND dependency.refclassid = 'pg_catalog.pg_class'::regclass
            JOIN pg_catalog.pg_class AS referenced_relation
              ON referenced_relation.oid = dependency.refobjid
            JOIN pg_catalog.pg_namespace AS relation_ns
              ON relation_ns.oid = referenced_relation.relnamespace
            WHERE relation_ns.nspname NOT IN ('pg_catalog', 'information_schema')
              AND referenced_relation.relkind IN ('r', 'p', 'f', 'v', 'm', 'S', 'c')

            UNION

            SELECT DISTINCT selected.oid AS selected_oid,
                   CASE referenced_proc.prokind WHEN 'p' THEN 'procedure' ELSE 'function' END::text AS kind,
                   referenced_ns.nspname AS dependency_schema, referenced_proc.proname AS name,
                   pg_catalog.pg_get_function_identity_arguments(referenced_proc.oid)::text AS signature
            FROM selected_functions AS selected
            JOIN pg_catalog.pg_depend AS dependency
              ON dependency.classid = 'pg_catalog.pg_proc'::regclass
             AND dependency.objid = selected.oid
             AND dependency.refclassid = 'pg_catalog.pg_proc'::regclass
            JOIN pg_catalog.pg_proc AS referenced_proc
              ON referenced_proc.oid = dependency.refobjid
            JOIN pg_catalog.pg_namespace AS referenced_ns
              ON referenced_ns.oid = referenced_proc.pronamespace
            WHERE referenced_ns.nspname NOT IN ('pg_catalog', 'information_schema')
              AND referenced_proc.prokind IN ('f', 'p')
        ),
        unsupported_dependencies AS (
            SELECT selected.oid AS selected_oid
            FROM selected_functions AS selected
            WHERE NOT selected.body_dependencies_proven_empty

            UNION

            SELECT DISTINCT selected.oid AS selected_oid
            FROM selected_functions AS selected
            JOIN pg_catalog.pg_depend AS dependency
              ON dependency.classid = 'pg_catalog.pg_proc'::regclass
             AND dependency.objid = selected.oid
            LEFT JOIN pg_catalog.pg_class AS referenced_relation
              ON dependency.refclassid = 'pg_catalog.pg_class'::regclass
             AND referenced_relation.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS relation_ns
              ON relation_ns.oid = referenced_relation.relnamespace
            LEFT JOIN pg_catalog.pg_type AS referenced_type
              ON dependency.refclassid = 'pg_catalog.pg_type'::regclass
             AND referenced_type.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS type_ns
              ON type_ns.oid = referenced_type.typnamespace
            LEFT JOIN pg_catalog.pg_proc AS referenced_proc
              ON dependency.refclassid = 'pg_catalog.pg_proc'::regclass
             AND referenced_proc.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS procedure_ns
              ON procedure_ns.oid = referenced_proc.pronamespace
            WHERE (
                dependency.refclassid = 'pg_catalog.pg_class'::regclass
                AND relation_ns.nspname NOT IN ('pg_catalog', 'information_schema')
                AND referenced_relation.relkind NOT IN ('r', 'p', 'f', 'v', 'm', 'S', 'c')
            ) OR (
                dependency.refclassid = 'pg_catalog.pg_type'::regclass
                AND type_ns.nspname NOT IN ('pg_catalog', 'information_schema')
                AND referenced_type.typtype NOT IN ('e', 'd', 'r', 'c')
            ) OR (
                dependency.refclassid = 'pg_catalog.pg_proc'::regclass
                AND procedure_ns.nspname NOT IN ('pg_catalog', 'information_schema')
                AND referenced_proc.prokind NOT IN ('f', 'p')
            ) OR dependency.refclassid NOT IN (
                'pg_catalog.pg_class'::regclass,
                'pg_catalog.pg_proc'::regclass,
                'pg_catalog.pg_type'::regclass,
                'pg_catalog.pg_namespace'::regclass,
                'pg_catalog.pg_language'::regclass,
                'pg_catalog.pg_authid'::regclass
            )
        )
        SELECT (SELECT count(*)::bigint FROM selected_functions) AS selected_count,
               (SELECT count(*)::bigint FROM unsupported_dependencies) AS unsupported_count,
               dependencies.kind, dependencies.dependency_schema, dependencies.name, dependencies.signature
        FROM (SELECT 1) AS seed
        LEFT JOIN selected_functions AS selected ON TRUE
        LEFT JOIN catalog_dependencies AS dependencies ON dependencies.selected_oid = selected.oid
        ORDER BY dependencies.kind, dependencies.dependency_schema, dependencies.name, dependencies.signature
        "#
    )
}
