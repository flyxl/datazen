use super::sql_string;

/// Build a PostgreSQL catalog query for a trigger's direct structured edges.
///
/// PostgreSQL stores the owning relation and trigger function OID directly on
/// `pg_trigger`. Dependencies introduced by a WHEN expression are also read
/// from `pg_depend`; if any cannot be represented by `DatabaseObject`, the
/// caller receives a non-zero unsupported count and must fail closed.
pub fn postgres_trigger_dependencies_sql(
    name: &str,
    schema: Option<&str>,
    target_schema: Option<&str>,
    target_name: Option<&str>,
) -> String {
    let name = sql_string(name);
    let trigger_schema_filter = schema
        .map(|schema| format!(" AND target_ns.nspname = {}", sql_string(schema)))
        .unwrap_or_default();
    let target_schema_filter = target_schema
        .map(|schema| format!(" AND target_ns.nspname = {}", sql_string(schema)))
        .unwrap_or_default();
    let target_name_filter = target_name
        .map(|name| format!(" AND target_rel.relname = {}", sql_string(name)))
        .unwrap_or_default();

    format!(
        r#"
        WITH selected_triggers AS (
            SELECT trigger.oid AS trigger_oid, trigger.tgrelid AS target_oid,
                   trigger.tgfoid AS function_oid, trigger.tgconstraint AS constraint_oid,
                   trigger.tgname AS trigger_name, target_ns.nspname AS target_schema,
                   target_rel.relname AS target_name
            FROM pg_catalog.pg_trigger AS trigger
            JOIN pg_catalog.pg_class AS target_rel ON target_rel.oid = trigger.tgrelid
            JOIN pg_catalog.pg_namespace AS target_ns ON target_ns.oid = target_rel.relnamespace
            WHERE trigger.tgname = {name}
              AND NOT trigger.tgisinternal{trigger_schema_filter}{target_schema_filter}{target_name_filter}
        ),
        catalog_dependencies AS (
            SELECT selected.trigger_oid,
                   CASE target_rel.relkind WHEN 'v' THEN 'view' WHEN 'm' THEN 'view' ELSE 'table' END AS kind,
                   target_ns.nspname AS dependency_schema, target_rel.relname AS name,
                   NULL::text AS signature
            FROM selected_triggers AS selected
            JOIN pg_catalog.pg_class AS target_rel ON target_rel.oid = selected.target_oid
            JOIN pg_catalog.pg_namespace AS target_ns ON target_ns.oid = target_rel.relnamespace

            UNION

            SELECT selected.trigger_oid,
                   CASE procedure.prokind WHEN 'p' THEN 'procedure' ELSE 'function' END AS kind,
                   procedure_ns.nspname AS dependency_schema, procedure.proname AS name,
                   pg_catalog.pg_get_function_identity_arguments(procedure.oid)::text AS signature
            FROM selected_triggers AS selected
            JOIN pg_catalog.pg_proc AS procedure ON procedure.oid = selected.function_oid
            JOIN pg_catalog.pg_namespace AS procedure_ns ON procedure_ns.oid = procedure.pronamespace
            WHERE procedure.prokind IN ('f', 'p')
        ),
        unsupported_dependencies AS (
            SELECT DISTINCT selected.trigger_oid
            FROM selected_triggers AS selected
            WHERE selected.constraint_oid <> 0

            UNION

            SELECT DISTINCT selected.trigger_oid
            FROM selected_triggers AS selected
            JOIN pg_catalog.pg_depend AS dependency
              ON dependency.classid = 'pg_catalog.pg_trigger'::regclass
             AND dependency.objid = selected.trigger_oid
            LEFT JOIN pg_catalog.pg_class AS referenced_rel
              ON dependency.refclassid = 'pg_catalog.pg_class'::regclass
             AND referenced_rel.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS rel_ns ON rel_ns.oid = referenced_rel.relnamespace
            LEFT JOIN pg_catalog.pg_proc AS referenced_proc
              ON dependency.refclassid = 'pg_catalog.pg_proc'::regclass
             AND referenced_proc.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS proc_ns ON proc_ns.oid = referenced_proc.pronamespace
            WHERE NOT (
                dependency.refclassid = 'pg_catalog.pg_class'::regclass
                AND dependency.refobjid = selected.target_oid
            )
              AND NOT (
                dependency.refclassid = 'pg_catalog.pg_proc'::regclass
                AND dependency.refobjid = selected.function_oid
            )
              AND (
                (dependency.refclassid = 'pg_catalog.pg_class'::regclass
                 AND rel_ns.nspname NOT IN ('pg_catalog', 'information_schema'))
                OR (dependency.refclassid = 'pg_catalog.pg_proc'::regclass
                    AND proc_ns.nspname NOT IN ('pg_catalog', 'information_schema'))
                OR dependency.refclassid NOT IN (
                    'pg_catalog.pg_class'::regclass, 'pg_catalog.pg_proc'::regclass
                )
              )
        )
        SELECT (SELECT count(*)::bigint FROM selected_triggers) AS selected_count,
               (SELECT count(*)::bigint FROM unsupported_dependencies) AS unsupported_count,
               dependencies.kind, dependencies.dependency_schema, dependencies.name, dependencies.signature
        FROM (SELECT 1) AS seed
        LEFT JOIN selected_triggers AS selected ON TRUE
        LEFT JOIN catalog_dependencies AS dependencies ON dependencies.trigger_oid = selected.trigger_oid
        ORDER BY dependencies.kind, dependencies.dependency_schema, dependencies.name, dependencies.signature
        "#
    )
}

/// Catalog-backed direct dependencies of an enum, domain, range, or composite
/// PostgreSQL type. User dependencies that cannot be represented as a
/// DatabaseObject make the catalog incomplete.
pub fn postgres_type_dependencies_sql(name: &str, schema: Option<&str>) -> String {
    let name = sql_string(name);
    let schema_filter = schema
        .map(|schema| format!(" AND type_ns.nspname = {}", sql_string(schema)))
        .unwrap_or_default();
    format!(
        r#"
        WITH selected_types AS (
            SELECT user_type.oid, user_type.typrelid
            FROM pg_catalog.pg_type AS user_type
            JOIN pg_catalog.pg_namespace AS type_ns ON type_ns.oid = user_type.typnamespace
            LEFT JOIN pg_catalog.pg_class AS composite_rel ON composite_rel.oid = user_type.typrelid
            WHERE user_type.typname = {name}{schema_filter}
              AND (user_type.typtype IN ('e', 'd', 'r')
                   OR (user_type.typtype = 'c' AND composite_rel.relkind = 'c'))
        ),
        dependency_owners AS (
            SELECT 'pg_catalog.pg_type'::regclass AS classid, selected.oid AS objid, selected.oid AS selected_oid
            FROM selected_types AS selected
            UNION
            SELECT 'pg_catalog.pg_class'::regclass, selected.typrelid, selected.oid
            FROM selected_types AS selected WHERE selected.typrelid <> 0
            UNION
            SELECT 'pg_catalog.pg_constraint'::regclass, domain_check.oid, selected.oid
            FROM selected_types AS selected
            JOIN pg_catalog.pg_constraint AS domain_check ON domain_check.contypid = selected.oid
        ),
        catalog_dependencies AS (
            SELECT DISTINCT owner.selected_oid,
                CASE referenced_rel.relkind WHEN 'v' THEN 'view' WHEN 'm' THEN 'view'
                    WHEN 'S' THEN 'sequence' WHEN 'c' THEN 'type' ELSE 'table' END AS kind,
                referenced_ns.nspname AS dependency_schema, referenced_rel.relname AS name,
                NULL::text AS signature
            FROM dependency_owners AS owner
            JOIN pg_catalog.pg_depend AS dependency
              ON dependency.classid = owner.classid AND dependency.objid = owner.objid
             AND dependency.refclassid = 'pg_catalog.pg_class'::regclass
            JOIN pg_catalog.pg_class AS referenced_rel ON referenced_rel.oid = dependency.refobjid
            JOIN pg_catalog.pg_namespace AS referenced_ns ON referenced_ns.oid = referenced_rel.relnamespace
            WHERE referenced_ns.nspname NOT IN ('pg_catalog', 'information_schema')
              AND referenced_rel.oid NOT IN (SELECT typrelid FROM selected_types WHERE typrelid <> 0)
            UNION
            SELECT DISTINCT owner.selected_oid, 'type' AS kind, referenced_ns.nspname AS dependency_schema,
                referenced_type.typname AS name, NULL::text AS signature
            FROM dependency_owners AS owner
            JOIN pg_catalog.pg_depend AS dependency
              ON dependency.classid = owner.classid AND dependency.objid = owner.objid
             AND dependency.refclassid = 'pg_catalog.pg_type'::regclass
            JOIN pg_catalog.pg_type AS referenced_type ON referenced_type.oid = dependency.refobjid
            JOIN pg_catalog.pg_namespace AS referenced_ns ON referenced_ns.oid = referenced_type.typnamespace
            WHERE referenced_ns.nspname NOT IN ('pg_catalog', 'information_schema')
              AND referenced_type.oid NOT IN (
                  SELECT selected_type.oid FROM selected_types AS selected_type
              )
              AND referenced_type.typtype IN ('e', 'd', 'r', 'c')
            UNION
            SELECT DISTINCT owner.selected_oid,
                CASE procedure.prokind WHEN 'p' THEN 'procedure' ELSE 'function' END AS kind,
                procedure_ns.nspname AS dependency_schema, procedure.proname AS name,
                pg_catalog.pg_get_function_identity_arguments(procedure.oid)::text AS signature
            FROM dependency_owners AS owner
            JOIN pg_catalog.pg_depend AS dependency
              ON dependency.classid = owner.classid AND dependency.objid = owner.objid
             AND dependency.refclassid = 'pg_catalog.pg_proc'::regclass
            JOIN pg_catalog.pg_proc AS procedure ON procedure.oid = dependency.refobjid
            JOIN pg_catalog.pg_namespace AS procedure_ns ON procedure_ns.oid = procedure.pronamespace
            WHERE procedure_ns.nspname NOT IN ('pg_catalog', 'information_schema')
              AND procedure.prokind IN ('f', 'p')
        ),
        unsupported_dependencies AS (
            SELECT DISTINCT owner.selected_oid, dependency.refclassid, dependency.refobjid
            FROM dependency_owners AS owner
            JOIN pg_catalog.pg_depend AS dependency
              ON dependency.classid = owner.classid AND dependency.objid = owner.objid
            LEFT JOIN pg_catalog.pg_class AS referenced_rel
              ON dependency.refclassid = 'pg_catalog.pg_class'::regclass
             AND referenced_rel.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS rel_ns ON rel_ns.oid = referenced_rel.relnamespace
            LEFT JOIN pg_catalog.pg_type AS referenced_type
              ON dependency.refclassid = 'pg_catalog.pg_type'::regclass
             AND referenced_type.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS type_ns ON type_ns.oid = referenced_type.typnamespace
            LEFT JOIN pg_catalog.pg_proc AS referenced_proc
              ON dependency.refclassid = 'pg_catalog.pg_proc'::regclass
             AND referenced_proc.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS proc_ns ON proc_ns.oid = referenced_proc.pronamespace
            LEFT JOIN pg_catalog.pg_collation AS referenced_collation
              ON dependency.refclassid = 'pg_catalog.pg_collation'::regclass
             AND referenced_collation.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS collation_ns
              ON collation_ns.oid = referenced_collation.collnamespace
            LEFT JOIN pg_catalog.pg_operator AS referenced_operator
              ON dependency.refclassid = 'pg_catalog.pg_operator'::regclass
             AND referenced_operator.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS operator_ns
              ON operator_ns.oid = referenced_operator.oprnamespace
            LEFT JOIN pg_catalog.pg_opclass AS referenced_opclass
              ON dependency.refclassid = 'pg_catalog.pg_opclass'::regclass
             AND referenced_opclass.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS opclass_ns
              ON opclass_ns.oid = referenced_opclass.opcnamespace
            WHERE
                (dependency.refclassid = 'pg_catalog.pg_class'::regclass
                 AND rel_ns.nspname NOT IN ('pg_catalog', 'information_schema')
                 AND referenced_rel.relkind NOT IN ('r', 'p', 'f', 'v', 'm', 'S', 'c'))
                OR (dependency.refclassid = 'pg_catalog.pg_type'::regclass
                    AND type_ns.nspname NOT IN ('pg_catalog', 'information_schema')
                    AND referenced_type.typtype NOT IN ('e', 'd', 'r', 'c'))
                OR (dependency.refclassid = 'pg_catalog.pg_proc'::regclass
                    AND proc_ns.nspname NOT IN ('pg_catalog', 'information_schema')
                    AND referenced_proc.prokind NOT IN ('f', 'p'))
                OR (dependency.refclassid = 'pg_catalog.pg_collation'::regclass
                    AND collation_ns.nspname NOT IN ('pg_catalog', 'information_schema'))
                OR (dependency.refclassid = 'pg_catalog.pg_operator'::regclass
                    AND operator_ns.nspname NOT IN ('pg_catalog', 'information_schema'))
                OR (dependency.refclassid = 'pg_catalog.pg_opclass'::regclass
                    AND opclass_ns.nspname NOT IN ('pg_catalog', 'information_schema'))
                OR dependency.refclassid NOT IN (
                    'pg_catalog.pg_namespace'::regclass, 'pg_catalog.pg_class'::regclass,
                    'pg_catalog.pg_type'::regclass, 'pg_catalog.pg_proc'::regclass,
                    'pg_catalog.pg_collation'::regclass, 'pg_catalog.pg_operator'::regclass,
                    'pg_catalog.pg_opclass'::regclass
                )
        )
        SELECT (SELECT count(*)::bigint FROM selected_types) AS selected_count,
               (SELECT count(*)::bigint FROM unsupported_dependencies) AS unsupported_count,
               dependencies.kind, dependencies.dependency_schema, dependencies.name, dependencies.signature
        FROM (SELECT 1) AS seed
        LEFT JOIN selected_types AS selected ON TRUE
        LEFT JOIN catalog_dependencies AS dependencies ON dependencies.selected_oid = selected.oid
        ORDER BY dependencies.kind, dependencies.dependency_schema, dependencies.name, dependencies.signature
        "#
    )
}

/// A standalone sequence is independent. Owned sequences expose their exact
/// owning relation; other dependency classes fail closed.
pub fn postgres_sequence_dependencies_sql(name: &str, schema: Option<&str>) -> String {
    let name = sql_string(name);
    let schema_filter = schema
        .map(|schema| format!(" AND sequence_ns.nspname = {}", sql_string(schema)))
        .unwrap_or_default();
    format!(
        r#"
        WITH selected_sequences AS (
            SELECT sequence.oid, sequence.relname AS sequence_name,
                   sequence_ns.nspname AS sequence_schema
            FROM pg_catalog.pg_class AS sequence
            JOIN pg_catalog.pg_namespace AS sequence_ns ON sequence_ns.oid = sequence.relnamespace
            WHERE sequence.relkind = 'S' AND sequence.relname = {name}
              {schema_filter}
        ),
        catalog_dependencies AS (
            SELECT DISTINCT selected.oid AS selected_oid,
                CASE owner_rel.relkind WHEN 'v' THEN 'view' WHEN 'm' THEN 'view' ELSE 'table' END AS kind,
                owner_ns.nspname AS dependency_schema, owner_rel.relname AS name,
                NULL::text AS signature,
                selected.sequence_schema, selected.sequence_name,
                owner_ns.nspname AS owner_table_schema, owner_rel.relname AS owner_table_name,
                owner_column.attname::text AS owner_column_name,
                'owned_by'::text AS sequence_usage
            FROM selected_sequences AS selected
            JOIN pg_catalog.pg_depend AS dependency
              ON dependency.classid = 'pg_catalog.pg_class'::regclass
             AND dependency.objid = selected.oid
             AND dependency.refclassid = 'pg_catalog.pg_class'::regclass
             AND dependency.deptype IN ('a', 'i')
            JOIN pg_catalog.pg_class AS owner_rel ON owner_rel.oid = dependency.refobjid
            JOIN pg_catalog.pg_namespace AS owner_ns ON owner_ns.oid = owner_rel.relnamespace
            LEFT JOIN pg_catalog.pg_attribute AS owner_column
              ON owner_column.attrelid = owner_rel.oid
             AND owner_column.attnum = dependency.refobjsubid
             AND NOT owner_column.attisdropped
            WHERE owner_rel.relkind IN ('r', 'p', 'f')
        ),
        unsupported_dependencies AS (
            SELECT DISTINCT selected.oid AS selected_oid, dependency.refclassid, dependency.refobjid
            FROM selected_sequences AS selected
            JOIN pg_catalog.pg_depend AS dependency
              ON dependency.classid = 'pg_catalog.pg_class'::regclass
             AND dependency.objid = selected.oid
            LEFT JOIN pg_catalog.pg_type AS referenced_type
              ON dependency.refclassid = 'pg_catalog.pg_type'::regclass
             AND referenced_type.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS type_ns ON type_ns.oid = referenced_type.typnamespace
            WHERE dependency.refclassid NOT IN (
                    'pg_catalog.pg_namespace'::regclass, 'pg_catalog.pg_class'::regclass
                )
              AND NOT (dependency.refclassid = 'pg_catalog.pg_type'::regclass
                       AND type_ns.nspname = 'pg_catalog')
            UNION
            SELECT DISTINCT selected.oid AS selected_oid, dependency.refclassid, dependency.refobjid
            FROM selected_sequences AS selected
            JOIN pg_catalog.pg_depend AS dependency
              ON dependency.classid = 'pg_catalog.pg_class'::regclass
             AND dependency.objid = selected.oid
             AND dependency.refclassid = 'pg_catalog.pg_class'::regclass
            LEFT JOIN pg_catalog.pg_class AS owner_rel ON owner_rel.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_attribute AS owner_column
              ON owner_column.attrelid = owner_rel.oid
             AND owner_column.attnum = dependency.refobjsubid
             AND NOT owner_column.attisdropped
            WHERE dependency.deptype NOT IN ('a', 'i')
               OR owner_rel.relkind NOT IN ('r', 'p', 'f')
               OR dependency.refobjsubid <= 0
               OR owner_column.attnum IS NULL
        )
        SELECT (SELECT count(*)::bigint FROM selected_sequences) AS selected_count,
               (SELECT count(*)::bigint FROM unsupported_dependencies) AS unsupported_count,
               dependencies.kind, dependencies.dependency_schema, dependencies.name, dependencies.signature,
               dependencies.sequence_schema, dependencies.sequence_name,
               dependencies.owner_table_schema, dependencies.owner_table_name,
               dependencies.owner_column_name, dependencies.sequence_usage
        FROM (SELECT 1) AS seed
        LEFT JOIN selected_sequences AS selected ON TRUE
        LEFT JOIN catalog_dependencies AS dependencies ON dependencies.selected_oid = selected.oid
        ORDER BY dependencies.kind, dependencies.dependency_schema, dependencies.name
        "#
    )
}

/// Dependencies a PostgreSQL table needs when created: custom column types,
/// FK target relations, sequence defaults, and referenced routines.
pub fn postgres_table_dependencies_sql(name: &str, schema: Option<&str>) -> String {
    let name = sql_string(name);
    let schema = sql_string(schema.unwrap_or("public"));
    format!(
        r#"
        WITH selected_tables AS (
            SELECT relation.oid, relation.relname AS table_name,
                   relation_ns.nspname AS schema_name
            FROM pg_catalog.pg_class AS relation
            JOIN pg_catalog.pg_namespace AS relation_ns ON relation_ns.oid = relation.relnamespace
            WHERE relation.relkind IN ('r', 'p', 'f') AND relation.relname = {name}
              AND relation_ns.nspname = {schema}
        ),
        dependency_owners AS (
            SELECT 'pg_catalog.pg_class'::regclass AS classid, selected.oid AS objid, selected.oid AS selected_oid
            FROM selected_tables AS selected
            UNION
            SELECT 'pg_catalog.pg_constraint'::regclass, constraint_row.oid, selected.oid
            FROM selected_tables AS selected
            JOIN pg_catalog.pg_constraint AS constraint_row ON constraint_row.conrelid = selected.oid
            UNION
            SELECT 'pg_catalog.pg_attrdef'::regclass, default_row.oid, selected.oid
            FROM selected_tables AS selected
            JOIN pg_catalog.pg_attrdef AS default_row ON default_row.adrelid = selected.oid
        ),
        catalog_dependencies AS (
            SELECT DISTINCT owner.selected_oid,
                CASE dependency_rel.relkind WHEN 'v' THEN 'view' WHEN 'm' THEN 'view'
                    WHEN 'S' THEN 'sequence' WHEN 'c' THEN 'type' ELSE 'table' END AS kind,
                dependency_ns.nspname AS dependency_schema, dependency_rel.relname AS name,
                NULL::text AS signature, NULL::text AS type_usage, NULL::text AS column_name,
                CASE WHEN dependency_rel.relkind = 'S'
                           AND owner.classid = 'pg_catalog.pg_attrdef'::regclass
                           AND default_column.attnum IS NOT NULL THEN 'column_default'
                     ELSE NULL::text END AS sequence_usage,
                CASE WHEN dependency_rel.relkind = 'S' THEN dependency_ns.nspname ELSE NULL::text END AS sequence_schema,
                CASE WHEN dependency_rel.relkind = 'S' THEN dependency_rel.relname ELSE NULL::text END AS sequence_name,
                CASE WHEN dependency_rel.relkind = 'S' THEN default_column.attname::text ELSE NULL::text END
                    AS owner_column_name
            FROM dependency_owners AS owner
            JOIN pg_catalog.pg_depend AS dependency
              ON dependency.classid = owner.classid AND dependency.objid = owner.objid
             AND dependency.refclassid = 'pg_catalog.pg_class'::regclass
            JOIN pg_catalog.pg_class AS referenced_rel ON referenced_rel.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_index AS referenced_index
              ON referenced_index.indexrelid = referenced_rel.oid
             AND referenced_rel.relkind IN ('i', 'I')
            JOIN pg_catalog.pg_class AS dependency_rel
              ON dependency_rel.oid = COALESCE(referenced_index.indrelid, referenced_rel.oid)
            JOIN pg_catalog.pg_namespace AS dependency_ns ON dependency_ns.oid = dependency_rel.relnamespace
            LEFT JOIN pg_catalog.pg_attrdef AS default_row
              ON owner.classid = 'pg_catalog.pg_attrdef'::regclass
             AND default_row.oid = owner.objid
            LEFT JOIN pg_catalog.pg_attribute AS default_column
              ON default_column.attrelid = default_row.adrelid
             AND default_column.attnum = default_row.adnum
             AND NOT default_column.attisdropped
            WHERE dependency_ns.nspname NOT IN ('pg_catalog', 'information_schema')
              AND dependency_rel.oid NOT IN (SELECT oid FROM selected_tables)
              AND dependency_rel.relkind IN ('r', 'p', 'f', 'v', 'm', 'S', 'c')
            UNION
            SELECT DISTINCT owner.selected_oid, 'type' AS kind, type_ns.nspname AS dependency_schema,
                referenced_type.typname AS name, NULL::text AS signature,
                CASE
                    WHEN owner.classid = 'pg_catalog.pg_class'::regclass AND dependency.objsubid > 0
                         AND dependency_column.attnum IS NOT NULL THEN 'column_type'
                    WHEN owner.classid = 'pg_catalog.pg_attrdef'::regclass THEN 'expression'
                    WHEN owner.classid = 'pg_catalog.pg_constraint'::regclass
                         AND type_constraint.contype = 'c' THEN 'constraint'
                    ELSE NULL
                END::text AS type_usage,
                CASE WHEN owner.classid = 'pg_catalog.pg_class'::regclass
                           AND dependency.objsubid > 0 THEN dependency_column.attname::text
                     ELSE NULL::text END AS column_name,
                NULL::text AS sequence_usage, NULL::text AS sequence_schema,
                NULL::text AS sequence_name, NULL::text AS owner_column_name
            FROM dependency_owners AS owner
            JOIN pg_catalog.pg_depend AS dependency
              ON dependency.classid = owner.classid AND dependency.objid = owner.objid
             AND dependency.refclassid = 'pg_catalog.pg_type'::regclass
            JOIN pg_catalog.pg_type AS referenced_type ON referenced_type.oid = dependency.refobjid
            JOIN pg_catalog.pg_namespace AS type_ns ON type_ns.oid = referenced_type.typnamespace
            LEFT JOIN pg_catalog.pg_attribute AS dependency_column
              ON owner.classid = 'pg_catalog.pg_class'::regclass
             AND dependency_column.attrelid = owner.objid
             AND dependency_column.attnum = dependency.objsubid
            LEFT JOIN pg_catalog.pg_constraint AS type_constraint
              ON owner.classid = 'pg_catalog.pg_constraint'::regclass
             AND type_constraint.oid = owner.objid
            WHERE type_ns.nspname NOT IN ('pg_catalog', 'information_schema')
              AND referenced_type.typtype IN ('e', 'd', 'r', 'c')
              AND referenced_type.typrelid NOT IN (SELECT oid FROM selected_tables)
            UNION
            SELECT DISTINCT owner.selected_oid,
                CASE procedure.prokind WHEN 'p' THEN 'procedure' ELSE 'function' END AS kind,
                procedure_ns.nspname AS dependency_schema, procedure.proname AS name,
                pg_catalog.pg_get_function_identity_arguments(procedure.oid)::text AS signature,
                NULL::text AS type_usage, NULL::text AS column_name,
                NULL::text AS sequence_usage, NULL::text AS sequence_schema,
                NULL::text AS sequence_name, NULL::text AS owner_column_name
            FROM dependency_owners AS owner
            JOIN pg_catalog.pg_depend AS dependency
              ON dependency.classid = owner.classid AND dependency.objid = owner.objid
             AND dependency.refclassid = 'pg_catalog.pg_proc'::regclass
            JOIN pg_catalog.pg_proc AS procedure ON procedure.oid = dependency.refobjid
            JOIN pg_catalog.pg_namespace AS procedure_ns ON procedure_ns.oid = procedure.pronamespace
            WHERE procedure_ns.nspname NOT IN ('pg_catalog', 'information_schema')
              AND procedure.prokind IN ('f', 'p')
        ),
        unsupported_dependencies AS (
            SELECT DISTINCT owner.selected_oid, dependency.refclassid, dependency.refobjid
            FROM dependency_owners AS owner
            JOIN pg_catalog.pg_depend AS dependency
              ON dependency.classid = owner.classid AND dependency.objid = owner.objid
            LEFT JOIN pg_catalog.pg_class AS referenced_rel
              ON dependency.refclassid = 'pg_catalog.pg_class'::regclass
             AND referenced_rel.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS rel_ns ON rel_ns.oid = referenced_rel.relnamespace
            LEFT JOIN pg_catalog.pg_index AS referenced_index
              ON referenced_index.indexrelid = referenced_rel.oid
             AND referenced_rel.relkind IN ('i', 'I')
            LEFT JOIN pg_catalog.pg_class AS index_owner
              ON index_owner.oid = referenced_index.indrelid
            LEFT JOIN pg_catalog.pg_namespace AS index_owner_ns ON index_owner_ns.oid = index_owner.relnamespace
            LEFT JOIN pg_catalog.pg_type AS referenced_type
              ON dependency.refclassid = 'pg_catalog.pg_type'::regclass
             AND referenced_type.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS type_ns ON type_ns.oid = referenced_type.typnamespace
            LEFT JOIN pg_catalog.pg_attribute AS dependency_column
              ON owner.classid = 'pg_catalog.pg_class'::regclass
             AND dependency_column.attrelid = owner.objid
             AND dependency_column.attnum = dependency.objsubid
            LEFT JOIN pg_catalog.pg_constraint AS type_constraint
              ON owner.classid = 'pg_catalog.pg_constraint'::regclass
             AND type_constraint.oid = owner.objid
            LEFT JOIN pg_catalog.pg_proc AS referenced_proc
              ON dependency.refclassid = 'pg_catalog.pg_proc'::regclass
             AND referenced_proc.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS proc_ns ON proc_ns.oid = referenced_proc.pronamespace
            LEFT JOIN pg_catalog.pg_collation AS referenced_collation
              ON dependency.refclassid = 'pg_catalog.pg_collation'::regclass
             AND referenced_collation.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS collation_ns
              ON collation_ns.oid = referenced_collation.collnamespace
            LEFT JOIN pg_catalog.pg_operator AS referenced_operator
              ON dependency.refclassid = 'pg_catalog.pg_operator'::regclass
             AND referenced_operator.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS operator_ns
              ON operator_ns.oid = referenced_operator.oprnamespace
            WHERE
                (dependency.refclassid = 'pg_catalog.pg_class'::regclass
                 AND rel_ns.nspname NOT IN ('pg_catalog', 'information_schema')
                 AND referenced_rel.relkind NOT IN ('r', 'p', 'f', 'v', 'm', 'S', 'c')
                 AND NOT (referenced_rel.relkind IN ('i', 'I')
                          AND index_owner.relkind IN ('r', 'p', 'f')
                          AND index_owner_ns.nspname NOT IN ('pg_catalog', 'information_schema')))
                OR (dependency.refclassid = 'pg_catalog.pg_type'::regclass
                    AND type_ns.nspname NOT IN ('pg_catalog', 'information_schema')
                    AND referenced_type.typrelid NOT IN (SELECT oid FROM selected_tables)
                    AND (referenced_type.typtype NOT IN ('e', 'd', 'r', 'c')
                         OR NOT (
                             (owner.classid = 'pg_catalog.pg_class'::regclass
                              AND dependency.objsubid > 0 AND dependency_column.attnum IS NOT NULL)
                             OR owner.classid = 'pg_catalog.pg_attrdef'::regclass
                             OR (owner.classid = 'pg_catalog.pg_constraint'::regclass
                                 AND type_constraint.contype = 'c')
                         )))
                OR (dependency.refclassid = 'pg_catalog.pg_proc'::regclass
                    AND proc_ns.nspname NOT IN ('pg_catalog', 'information_schema')
                    AND referenced_proc.prokind NOT IN ('f', 'p'))
                OR (dependency.refclassid = 'pg_catalog.pg_collation'::regclass
                    AND collation_ns.nspname NOT IN ('pg_catalog', 'information_schema'))
                OR (dependency.refclassid = 'pg_catalog.pg_operator'::regclass
                    AND operator_ns.nspname NOT IN ('pg_catalog', 'information_schema'))
                OR dependency.refclassid NOT IN (
                    'pg_catalog.pg_namespace'::regclass, 'pg_catalog.pg_class'::regclass,
                    'pg_catalog.pg_type'::regclass, 'pg_catalog.pg_proc'::regclass,
                    'pg_catalog.pg_collation'::regclass, 'pg_catalog.pg_operator'::regclass
                )
        )
        SELECT (SELECT count(*)::bigint FROM selected_tables) AS selected_count,
               (SELECT count(*)::bigint FROM unsupported_dependencies) AS unsupported_count,
               dependencies.kind, dependencies.dependency_schema, dependencies.name, dependencies.signature,
               dependencies.type_usage, dependencies.column_name,
               selected.schema_name AS owner_table_schema, selected.table_name AS owner_table_name,
               dependencies.sequence_schema, dependencies.sequence_name,
               dependencies.owner_column_name, dependencies.sequence_usage
        FROM (SELECT 1) AS seed
        LEFT JOIN selected_tables AS selected ON TRUE
        LEFT JOIN catalog_dependencies AS dependencies ON dependencies.selected_oid = selected.oid
        ORDER BY dependencies.kind, dependencies.dependency_schema, dependencies.name, dependencies.signature
        "#
    )
}

pub(super) fn postgres_view_dependencies_sql(name: &str, schema_filter: &str) -> String {
    format!(
        r#"
        WITH selected_views AS (
            SELECT view_rel.oid, view_ns.nspname AS schema_name, view_rel.relname AS object_name
            FROM pg_catalog.pg_class AS view_rel
            JOIN pg_catalog.pg_namespace AS view_ns ON view_ns.oid = view_rel.relnamespace
            WHERE view_rel.relkind IN ('v', 'm')
              AND view_rel.relname = {name}{schema_filter}
        ),
        catalog_dependencies AS (
            SELECT DISTINCT selected.oid AS view_oid,
                CASE referenced.relkind
                    WHEN 'v' THEN 'view'
                    WHEN 'm' THEN 'view'
                    WHEN 'S' THEN 'sequence'
                    WHEN 'c' THEN 'type'
                    ELSE 'table'
                END AS kind,
                referenced_ns.nspname AS dependency_schema,
                referenced.relname AS name,
                NULL::text AS signature
            FROM selected_views AS selected
            JOIN pg_catalog.pg_rewrite AS rewrite_rule
              ON rewrite_rule.ev_class = selected.oid AND rewrite_rule.rulename = '_RETURN'
            JOIN pg_catalog.pg_depend AS dependency
              ON dependency.classid = 'pg_catalog.pg_rewrite'::regclass
             AND dependency.objid = rewrite_rule.oid
             AND dependency.refclassid = 'pg_catalog.pg_class'::regclass
            JOIN pg_catalog.pg_class AS referenced ON referenced.oid = dependency.refobjid
            JOIN pg_catalog.pg_namespace AS referenced_ns ON referenced_ns.oid = referenced.relnamespace
            WHERE referenced.relkind IN ('r', 'p', 'f', 'v', 'm', 'S', 'c')
              AND referenced.oid <> selected.oid

            UNION

            SELECT DISTINCT selected.oid AS view_oid,
                CASE procedure.prokind WHEN 'p' THEN 'procedure' ELSE 'function' END AS kind,
                procedure_ns.nspname AS dependency_schema,
                procedure.proname AS name,
                pg_catalog.pg_get_function_identity_arguments(procedure.oid)::text AS signature
            FROM selected_views AS selected
            JOIN pg_catalog.pg_rewrite AS rewrite_rule
              ON rewrite_rule.ev_class = selected.oid AND rewrite_rule.rulename = '_RETURN'
            JOIN pg_catalog.pg_depend AS dependency
              ON dependency.classid = 'pg_catalog.pg_rewrite'::regclass
             AND dependency.objid = rewrite_rule.oid
             AND dependency.refclassid = 'pg_catalog.pg_proc'::regclass
            JOIN pg_catalog.pg_proc AS procedure ON procedure.oid = dependency.refobjid
            JOIN pg_catalog.pg_namespace AS procedure_ns ON procedure_ns.oid = procedure.pronamespace
            WHERE procedure.prokind IN ('f', 'p')

            UNION

            SELECT DISTINCT selected.oid AS view_oid, 'type' AS kind,
                type_ns.nspname AS dependency_schema, referenced_type.typname AS name,
                NULL::text AS signature
            FROM selected_views AS selected
            JOIN pg_catalog.pg_rewrite AS rewrite_rule
              ON rewrite_rule.ev_class = selected.oid AND rewrite_rule.rulename = '_RETURN'
            JOIN pg_catalog.pg_depend AS dependency
              ON dependency.classid = 'pg_catalog.pg_rewrite'::regclass
             AND dependency.objid = rewrite_rule.oid
             AND dependency.refclassid = 'pg_catalog.pg_type'::regclass
            JOIN pg_catalog.pg_type AS referenced_type ON referenced_type.oid = dependency.refobjid
            JOIN pg_catalog.pg_namespace AS type_ns ON type_ns.oid = referenced_type.typnamespace
            WHERE type_ns.nspname NOT IN ('pg_catalog', 'information_schema')
              AND referenced_type.typrelid = 0
              AND referenced_type.typtype IN ('e', 'd', 'r')

            UNION

            -- The SQL-standard usage views provide an independent catalog
            -- path. Their role-filtered subset is supplemented by pg_depend.
            SELECT DISTINCT selected.oid AS view_oid,
                CASE referenced.relkind WHEN 'v' THEN 'view' WHEN 'm' THEN 'view'
                    WHEN 'S' THEN 'sequence' WHEN 'c' THEN 'type' ELSE 'table' END AS kind,
                referenced_ns.nspname AS dependency_schema, referenced.relname AS name,
                NULL::text AS signature
            FROM selected_views AS selected
            JOIN information_schema.view_table_usage AS usage
              ON usage.view_schema = selected.schema_name AND usage.view_name = selected.object_name
            JOIN pg_catalog.pg_namespace AS referenced_ns ON referenced_ns.nspname = usage.table_schema
            JOIN pg_catalog.pg_class AS referenced
              ON referenced.relnamespace = referenced_ns.oid AND referenced.relname = usage.table_name
            WHERE referenced.oid <> selected.oid

            UNION

            SELECT DISTINCT selected.oid AS view_oid,
                CASE procedure.prokind WHEN 'p' THEN 'procedure' ELSE 'function' END AS kind,
                procedure_ns.nspname AS dependency_schema, procedure.proname AS name,
                pg_catalog.pg_get_function_identity_arguments(procedure.oid)::text AS signature
            FROM selected_views AS selected
            JOIN information_schema.view_routine_usage AS usage
              ON usage.table_schema = selected.schema_name AND usage.table_name = selected.object_name
            JOIN information_schema.routines AS routine
              ON routine.specific_schema = usage.specific_schema
             AND routine.specific_name = usage.specific_name
            JOIN pg_catalog.pg_proc AS procedure
              ON routine.specific_name = pg_catalog.nameconcatoid(procedure.proname, procedure.oid)
            JOIN pg_catalog.pg_namespace AS procedure_ns
              ON procedure_ns.oid = procedure.pronamespace
             AND procedure_ns.nspname = routine.specific_schema
            WHERE procedure.prokind IN ('f', 'p')
        ),
        unsupported_dependencies AS (
            SELECT DISTINCT selected.oid AS view_oid, dependency.refclassid, dependency.refobjid
            FROM selected_views AS selected
            JOIN pg_catalog.pg_rewrite AS rewrite_rule
              ON rewrite_rule.ev_class = selected.oid AND rewrite_rule.rulename = '_RETURN'
            JOIN pg_catalog.pg_depend AS dependency
              ON dependency.classid = 'pg_catalog.pg_rewrite'::regclass
             AND dependency.objid = rewrite_rule.oid
            LEFT JOIN pg_catalog.pg_class AS referenced_rel
              ON dependency.refclassid = 'pg_catalog.pg_class'::regclass
             AND referenced_rel.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS rel_ns ON rel_ns.oid = referenced_rel.relnamespace
            LEFT JOIN pg_catalog.pg_proc AS referenced_proc
              ON dependency.refclassid = 'pg_catalog.pg_proc'::regclass
             AND referenced_proc.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS proc_ns ON proc_ns.oid = referenced_proc.pronamespace
            LEFT JOIN pg_catalog.pg_type AS referenced_type
              ON dependency.refclassid = 'pg_catalog.pg_type'::regclass
             AND referenced_type.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS type_ns ON type_ns.oid = referenced_type.typnamespace
            LEFT JOIN pg_catalog.pg_operator AS referenced_operator
              ON dependency.refclassid = 'pg_catalog.pg_operator'::regclass
             AND referenced_operator.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS operator_ns
              ON operator_ns.oid = referenced_operator.oprnamespace
            LEFT JOIN pg_catalog.pg_collation AS referenced_collation
              ON dependency.refclassid = 'pg_catalog.pg_collation'::regclass
             AND referenced_collation.oid = dependency.refobjid
            LEFT JOIN pg_catalog.pg_namespace AS collation_ns
              ON collation_ns.oid = referenced_collation.collnamespace
            WHERE
                (dependency.refclassid = 'pg_catalog.pg_class'::regclass
                 AND rel_ns.nspname NOT IN ('pg_catalog', 'information_schema')
                 AND referenced_rel.relkind NOT IN ('r', 'p', 'f', 'v', 'm', 'S', 'c'))
                OR (dependency.refclassid = 'pg_catalog.pg_proc'::regclass
                    AND proc_ns.nspname NOT IN ('pg_catalog', 'information_schema')
                    AND referenced_proc.prokind NOT IN ('f', 'p'))
                OR (dependency.refclassid = 'pg_catalog.pg_type'::regclass
                    AND type_ns.nspname NOT IN ('pg_catalog', 'information_schema')
                    AND referenced_type.typrelid = 0
                    AND referenced_type.typtype NOT IN ('e', 'd', 'r'))
                OR (dependency.refclassid = 'pg_catalog.pg_operator'::regclass
                    AND operator_ns.nspname NOT IN ('pg_catalog', 'information_schema'))
                OR (dependency.refclassid = 'pg_catalog.pg_collation'::regclass
                    AND collation_ns.nspname NOT IN ('pg_catalog', 'information_schema'))
        )
        SELECT (SELECT count(*)::bigint FROM selected_views) AS selected_count,
               (SELECT count(*)::bigint FROM unsupported_dependencies) AS unsupported_count,
               dependencies.kind, dependencies.dependency_schema, dependencies.name, dependencies.signature
        FROM (SELECT 1) AS seed
        LEFT JOIN selected_views AS selected ON TRUE
        LEFT JOIN catalog_dependencies AS dependencies ON dependencies.view_oid = selected.oid
        ORDER BY dependencies.kind, dependencies.dependency_schema, dependencies.name, dependencies.signature
        "#
    )
}
