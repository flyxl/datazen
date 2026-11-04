//! PostgreSQL sync adapter.

use datazen_driver_api::{
    BoxedSyncAdapter, ColumnSchema, IRColumn, IRDefault, IRType, SyncAdapterFactory,
    SyncSourceAdapter, SyncTargetAdapter, Value,
};

pub struct PgSyncAdapter;

fn create() -> BoxedSyncAdapter {
    BoxedSyncAdapter::both(PgSyncAdapter)
}

datazen_driver_api::inventory::submit! {
    SyncAdapterFactory {
        // cloudberry: PG wire + catalogs; safe alias of PgSyncAdapter
        // questdb: PG wire + catalogs (ReuseDriver); cloudberry same family
        db_types: &["postgresql", "postgres", "cloudberry", "questdb"],
        create,
    }
}

// ── helpers ────────────────────────────────────────────────────────

fn parse_precision(s: &str, prefix: &str) -> (u8, u8) {
    if let Some(rest) = s.strip_prefix(prefix) {
        let rest = rest.trim();
        if let Some(inner) = rest.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
            let parts: Vec<&str> = inner.split(',').collect();
            let p = parts
                .first()
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0);
            let s = parts
                .get(1)
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0);
            return (p, s);
        }
    }
    (0, 0)
}

fn parse_char_length(s: &str, prefix: &str) -> Option<u32> {
    s.strip_prefix(prefix)
        .and_then(|r| r.trim().strip_prefix('('))
        .and_then(|r| r.strip_suffix(')'))
        .and_then(|n| n.trim().parse().ok())
}

fn parse_pg_default(raw: &str, col: &ColumnSchema) -> Option<IRDefault> {
    let d = raw.trim();
    if d.is_empty() {
        return None;
    }
    if d.contains("nextval(") {
        return None;
    }
    if d == "now()" || d == "CURRENT_TIMESTAMP" || d == "current_timestamp" {
        return Some(IRDefault::CurrentTimestamp);
    }
    if d.contains("::") {
        let stripped = d.split("::").next().unwrap_or(d);
        return if is_literal_default(stripped) {
            Some(IRDefault::Literal(stripped.to_string()))
        } else {
            Some(IRDefault::RawExpression(d.to_string()))
        };
    }
    let _ = col;
    if is_literal_default(d) {
        Some(IRDefault::Literal(d.to_string()))
    } else {
        Some(IRDefault::RawExpression(d.to_string()))
    }
}

fn is_literal_default(value: &str) -> bool {
    let value = value.trim();
    value.parse::<i64>().is_ok()
        || value.parse::<f64>().is_ok()
        || matches!(
            value.to_ascii_lowercase().as_str(),
            "true" | "false" | "null"
        )
        || (value.starts_with('\'') && value.ends_with('\''))
}

// ── SyncSourceAdapter ──────────────────────────────────────────────

impl SyncSourceAdapter for PgSyncAdapter {
    fn validate_transfer_source_column(&self, column: &ColumnSchema) -> Result<(), String> {
        let native = column.data_type.trim().to_ascii_lowercase();
        if native == "array" || native.ends_with("[]") || native.ends_with(" array") {
            return Err(format!(
                "PostgreSQL array type '{}' is not supported because the PostgreSQL transfer decoder does not yet decode arrays; convert it to a scalar JSON value before transferring",
                column.data_type
            ));
        }
        Ok(())
    }

    fn transfer_source_type_is_native_only(
        &self,
        _column: &ColumnSchema,
        source_ir: &IRColumn,
    ) -> bool {
        matches!(source_ir.ir_type, IRType::Other(_))
    }

    fn transfer_source_text_limit_bytes(&self, column: &ColumnSchema) -> Option<u64> {
        const PG_MAX_FIELD_BYTES: u64 = 1_073_741_823;
        let native = column.data_type.trim().to_ascii_lowercase();
        if native == "text" || native == "character varying" || native == "varchar" {
            return Some(PG_MAX_FIELD_BYTES);
        }
        for prefix in ["character varying(", "varchar(", "character("] {
            if let Some(length) = native
                .strip_prefix(prefix)
                .and_then(|rest| rest.strip_suffix(')'))
                .and_then(|length| length.parse::<u64>().ok())
            {
                return length.checked_mul(4);
            }
        }
        None
    }

    fn transfer_source_requires_collation_preservation(&self, column: &ColumnSchema) -> bool {
        let native = column.data_type.trim().to_ascii_lowercase();
        native == "text"
            || native.starts_with("character varying")
            || native.starts_with("varchar")
            || native == "character"
            || native.starts_with("character(")
            || native == "char"
            || native.starts_with("char(")
            || native == "bpchar"
    }

    fn sync_key_order_expression(
        &self,
        quoted_column: &str,
        contract: &datazen_driver_api::SyncKeyContract,
    ) -> String {
        match &contract.kind {
            datazen_driver_api::SyncKeyKind::Text {
                collation: datazen_driver_api::SyncKeyCollation::Binary,
            } => format!(r#"{quoted_column} COLLATE "C""#),
            _ => quoted_column.to_string(),
        }
    }

    fn full_column_types_query(&self, table: &str) -> Option<String> {
        let (schema, name) = crate::sql::parse_pg_table_ref(table);
        let relation = match schema {
            Some(schema) => format!("{}.{}", self.quote_ident(schema), self.quote_ident(name)),
            None => self.quote_ident(name),
        };
        let escaped = relation.replace('\'', "''");
        Some(format!(
            r#"SELECT a.attname::text AS col_name,
                  format_type(a.atttypid, a.atttypmod) AS full_type
           FROM pg_attribute a
           WHERE a.attrelid = '{escaped}'::regclass
             AND a.attnum > 0
             AND NOT a.attisdropped
           ORDER BY a.attnum"#
        ))
    }

    fn unsupported_transfer_structure_query(
        &self,
        _database: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Option<String> {
        let (parsed_schema, name) = crate::sql::parse_pg_table_ref(table);
        let schema = schema.or(parsed_schema);
        let relation = match schema {
            Some(schema) => format!("{}.{}", self.quote_ident(schema), self.quote_ident(name)),
            None => self.quote_ident(name),
        };
        let escaped = relation.replace('\'', "''");
        Some(format!(
            "SELECT CASE WHEN COALESCE(to_jsonb(a)->>'attidentity', '') = 'a' \
                       THEN format('identity column %I (GENERATED ALWAYS)', a.attname)::text \
                       ELSE format('generated column %I', a.attname)::text END AS unsupported_object \
             FROM pg_catalog.pg_attribute a \
             WHERE a.attrelid = '{escaped}'::regclass AND a.attnum > 0 \
               AND NOT a.attisdropped \
               AND (COALESCE(to_jsonb(a)->>'attgenerated', '') <> '' \
                    OR COALESCE(to_jsonb(a)->>'attidentity', '') = 'a') \
             UNION ALL \
             SELECT format('index %I (expression, predicate, or INCLUDE column)', ix.relname)::text \
             FROM pg_catalog.pg_index i \
             JOIN pg_catalog.pg_class ix ON ix.oid = i.indexrelid \
             WHERE i.indrelid = '{escaped}'::regclass \
               AND (i.indexprs IS NOT NULL OR i.indpred IS NOT NULL \
                    OR COALESCE((to_jsonb(i)->>'indnkeyatts')::integer, i.indnatts) < i.indnatts) \
             UNION ALL \
             SELECT format('index %I (sort order, operator class, or collation)', ix.relname)::text \
             FROM pg_catalog.pg_index i \
             JOIN pg_catalog.pg_class ix ON ix.oid = i.indexrelid \
             JOIN LATERAL unnest(i.indclass) WITH ORDINALITY AS classes(opclass_oid, ordinality) ON true \
             JOIN pg_catalog.pg_opclass opc ON opc.oid = classes.opclass_oid \
             JOIN LATERAL unnest(i.indkey) WITH ORDINALITY AS keys(attnum, ordinality) \
               ON keys.ordinality = classes.ordinality \
             LEFT JOIN pg_catalog.pg_attribute a \
               ON a.attrelid = i.indrelid AND a.attnum = keys.attnum \
             LEFT JOIN LATERAL unnest(i.indcollation) WITH ORDINALITY AS collations(collation_oid, ordinality) \
               ON collations.ordinality = classes.ordinality \
             LEFT JOIN LATERAL unnest(i.indoption) WITH ORDINALITY AS options(option_bits, ordinality) \
               ON options.ordinality = classes.ordinality \
             WHERE i.indrelid = '{escaped}'::regclass \
               AND (NOT opc.opcdefault \
                    OR collations.collation_oid IS DISTINCT FROM a.attcollation \
                    OR COALESCE(options.option_bits, 0) <> 0) \
             UNION ALL \
             SELECT format('foreign key %I (MATCH type, validation state, or SET NULL column list)', con.conname)::text \
             FROM pg_catalog.pg_constraint con \
             WHERE con.conrelid = '{escaped}'::regclass AND con.contype = 'f' \
               AND (con.confmatchtype <> 's' OR NOT con.convalidated \
                    OR NULLIF(to_jsonb(con)->>'confdelsetcols', 'null') IS NOT NULL) \
             UNION ALL \
             SELECT format('sequence on column %I (unowned, shared, or non-default sequence options)', a.attname)::text \
             FROM pg_catalog.pg_attribute a \
             LEFT JOIN pg_catalog.pg_attrdef ad \
               ON ad.adrelid = a.attrelid AND ad.adnum = a.attnum \
             CROSS JOIN LATERAL ( \
               SELECT pg_get_expr(ad.adbin, ad.adrelid) AS expression \
             ) default_expression \
             LEFT JOIN LATERAL ( \
               SELECT count(DISTINCT d.refobjid)::integer AS sequence_count, \
                      (array_agg(DISTINCT d.refobjid))[1] AS sequence_oid \
               FROM pg_catalog.pg_depend d \
               JOIN pg_catalog.pg_class candidate_seq \
                 ON candidate_seq.oid = d.refobjid AND candidate_seq.relkind = 'S' \
               WHERE d.classid = 'pg_attrdef'::regclass AND d.objid = ad.oid \
                 AND d.refclassid = 'pg_class'::regclass \
             ) default_sequence ON true \
             LEFT JOIN LATERAL ( \
               SELECT count(DISTINCT d.objid)::integer AS sequence_count, \
                      (array_agg(DISTINCT d.objid))[1] AS sequence_oid \
               FROM pg_catalog.pg_depend d \
               JOIN pg_catalog.pg_class candidate_seq \
                 ON candidate_seq.oid = d.objid AND candidate_seq.relkind = 'S' \
               WHERE d.classid = 'pg_class'::regclass AND d.objsubid = 0 \
                 AND d.refclassid = 'pg_class'::regclass \
                 AND d.refobjid = a.attrelid AND d.refobjsubid = a.attnum \
                 AND d.deptype IN ('a', 'i') \
             ) owned_sequence ON true \
             CROSS JOIN LATERAL ( \
               SELECT CASE WHEN COALESCE(to_jsonb(a)->>'attidentity', '') <> '' \
                           THEN owned_sequence.sequence_oid \
                           ELSE default_sequence.sequence_oid END AS sequence_oid \
             ) sequence_candidate \
             LEFT JOIN pg_catalog.pg_class seq \
               ON seq.oid = sequence_candidate.sequence_oid AND seq.relkind = 'S' \
             LEFT JOIN pg_catalog.pg_sequence seq_options ON seq_options.seqrelid = seq.oid \
             LEFT JOIN LATERAL ( \
               SELECT count(*)::integer AS owner_count, \
                      bool_or(d.refobjid = a.attrelid AND d.refobjsubid = a.attnum) AS owner_matches_column, \
                      bool_or(d.deptype = 'a') AS has_auto_owner, \
                      bool_or(d.deptype = 'i') AS has_internal_owner \
               FROM pg_catalog.pg_depend d \
               WHERE d.classid = 'pg_class'::regclass AND d.objid = seq.oid \
                 AND d.objsubid = 0 AND d.refclassid = 'pg_class'::regclass \
                 AND d.deptype IN ('a', 'i') \
             ) sequence_owners ON true \
             LEFT JOIN LATERAL ( \
               SELECT count(DISTINCT d.objid)::integer AS default_count, \
                      bool_or(d.objid = ad.oid) AS includes_current_default \
               FROM pg_catalog.pg_depend d \
               WHERE d.classid = 'pg_attrdef'::regclass \
                 AND d.refclassid = 'pg_class'::regclass \
                 AND d.refobjid = seq.oid \
             ) sequence_defaults ON true \
             WHERE a.attrelid = '{escaped}'::regclass AND a.attnum > 0 \
               AND NOT a.attisdropped \
               AND (COALESCE(to_jsonb(a)->>'attidentity', '') <> '' \
                    OR COALESCE(lower(default_expression.expression), '') LIKE '%nextval(%') \
               AND (sequence_candidate.sequence_oid IS NULL \
                    OR seq.oid IS NULL \
                    OR (CASE WHEN COALESCE(to_jsonb(a)->>'attidentity', '') <> '' THEN \
                           owned_sequence.sequence_count <> 1 \
                           OR sequence_defaults.default_count <> 0 \
                         ELSE default_sequence.sequence_count <> 1 \
                           OR owned_sequence.sequence_count <> 1 \
                           OR owned_sequence.sequence_oid IS DISTINCT FROM default_sequence.sequence_oid \
                           OR sequence_defaults.default_count <> 1 \
                           OR sequence_defaults.includes_current_default IS DISTINCT FROM true \
                           OR lower(default_expression.expression) NOT LIKE 'nextval(%::regclass)' \
                           OR length(lower(default_expression.expression)) \
                              - length(replace(lower(default_expression.expression), 'nextval(', '')) \
                              <> length('nextval(') \
                         END) \
                    OR sequence_owners.owner_count <> 1 \
                    OR sequence_owners.owner_matches_column IS DISTINCT FROM true \
                    OR sequence_owners.has_auto_owner IS DISTINCT FROM \
                       (COALESCE(to_jsonb(a)->>'attidentity', '') = '') \
                    OR sequence_owners.has_internal_owner IS DISTINCT FROM \
                       (COALESCE(to_jsonb(a)->>'attidentity', '') <> '') \
                    OR seq_options.seqtypid IS DISTINCT FROM a.atttypid \
                    OR seq_options.seqstart IS DISTINCT FROM 1 \
                    OR seq_options.seqincrement IS DISTINCT FROM 1 \
                    OR seq_options.seqmin IS DISTINCT FROM 1 \
                    OR seq_options.seqmax IS DISTINCT FROM CASE a.atttypid \
                         WHEN 'smallint'::regtype THEN 32767::bigint \
                         WHEN 'integer'::regtype THEN 2147483647::bigint \
                         WHEN 'bigint'::regtype THEN 9223372036854775807::bigint \
                         ELSE NULL::bigint END \
                    OR seq_options.seqcache IS DISTINCT FROM 1 \
                    OR seq_options.seqcycle IS DISTINCT FROM false)"
        ))
    }

    fn column_to_ir(&self, column: &ColumnSchema, native_full_type: Option<&str>) -> IRColumn {
        let raw = native_full_type.unwrap_or(&column.data_type);
        let lower = raw.trim().to_lowercase();

        let ir_type = if lower.ends_with("[]") || lower == "array" {
            IRType::Json
        } else if lower.starts_with("character varying") {
            let len = parse_char_length(&lower, "character varying");
            IRType::Varchar { length: len }
        } else if lower.starts_with("character(") || lower == "character" {
            let len = parse_char_length(&lower, "character").unwrap_or(1);
            IRType::Char { length: len }
        } else if lower.starts_with("numeric") || lower.starts_with("decimal") {
            let prefix = if lower.starts_with("numeric") {
                "numeric"
            } else {
                "decimal"
            };
            let (p, s) = parse_precision(&lower, prefix);
            IRType::Decimal {
                precision: p,
                scale: s,
            }
        } else if lower.starts_with("bit varying") {
            IRType::Blob
        } else if lower.starts_with("bit(") || lower == "bit" {
            let len = parse_char_length(&lower, "bit").unwrap_or(1);
            IRType::Bit { length: len }
        } else {
            match lower.as_str() {
                "integer" | "int" | "int4" => IRType::Int32,
                "bigint" | "int8" => IRType::Int64,
                "smallint" | "int2" => IRType::Int16,
                "text" => IRType::Text,
                "boolean" | "bool" => IRType::Bool,
                "real" | "float4" => IRType::Float32,
                "double precision" | "float8" => IRType::Float64,
                "bytea" => IRType::Blob,
                "json" | "jsonb" => IRType::Json,
                "uuid" => IRType::Uuid,
                "date" => IRType::Date,
                "time without time zone" | "time" => IRType::Time {
                    with_timezone: false,
                },
                "time with time zone" | "timetz" => IRType::Time {
                    with_timezone: true,
                },
                "timestamp without time zone" | "timestamp" => IRType::Timestamp {
                    with_timezone: false,
                },
                "timestamp with time zone" | "timestamptz" => IRType::Timestamp {
                    with_timezone: true,
                },
                "inet" => IRType::Varchar { length: Some(45) },
                "cidr" => IRType::Varchar { length: Some(43) },
                "macaddr" | "macaddr8" => IRType::Varchar { length: Some(17) },
                "interval" => IRType::Varchar { length: Some(255) },
                "money" => IRType::Decimal {
                    precision: 19,
                    scale: 2,
                },
                "oid" => IRType::Int32,
                "xml" => IRType::Text,
                _ => IRType::Other(raw.to_string()),
            }
        };

        IRColumn {
            name: column.name.clone(),
            ir_type,
            nullable: column.nullable,
            default_expr: column
                .default_value
                .as_deref()
                .and_then(|d| parse_pg_default(d, column)),
            is_primary_key: column.is_primary_key,
            is_auto_increment: column.is_auto_increment,
            comment: column.comment.clone(),
        }
    }
}

// ── SyncTargetAdapter ──────────────────────────────────────────────

impl SyncTargetAdapter for PgSyncAdapter {
    fn ir_type_to_native(&self, ir_type: &IRType) -> String {
        match ir_type {
            IRType::Bool => "boolean".into(),
            IRType::Int8 => "smallint".into(),
            IRType::Int16 => "smallint".into(),
            IRType::Int32 => "integer".into(),
            IRType::Int64 => "bigint".into(),
            IRType::Float32 => "real".into(),
            IRType::Float64 => "double precision".into(),
            IRType::Decimal { precision: 0, .. } => "numeric".into(),
            IRType::Decimal { precision, scale } => format!("numeric({precision},{scale})"),
            IRType::Char { length } => format!("character({length})"),
            IRType::Varchar { length: Some(n) } => format!("character varying({n})"),
            IRType::Varchar { length: None } | IRType::Text => "text".into(),
            IRType::Binary { .. } | IRType::Blob => "bytea".into(),
            IRType::Date => "date".into(),
            IRType::Time {
                with_timezone: false,
            } => "time without time zone".into(),
            IRType::Time {
                with_timezone: true,
            } => "time with time zone".into(),
            IRType::Timestamp {
                with_timezone: false,
            } => "timestamp without time zone".into(),
            IRType::Timestamp {
                with_timezone: true,
            } => "timestamp with time zone".into(),
            IRType::Json => "jsonb".into(),
            IRType::Uuid => "uuid".into(),
            IRType::Bit { length } => format!("bit({length})"),
            IRType::Other(_) => "text".into(),
        }
    }

    fn format_default(&self, default: &IRDefault) -> Option<String> {
        match default {
            IRDefault::CurrentTimestamp => Some("now()".into()),
            IRDefault::Literal(s) => Some(s.clone()),
            IRDefault::RawExpression(s) => Some(s.clone()),
        }
    }

    fn format_literal(&self, value: &Option<Value>, _ir_type: &IRType) -> String {
        match value {
            None | Some(Value::Null) => "NULL".into(),
            Some(Value::Bool(b)) => if *b { "TRUE" } else { "FALSE" }.into(),
            Some(Value::Integer(n)) => n.to_string(),
            Some(Value::Float(f)) => f.to_string(),
            Some(Value::String(s)) => format!("'{}'", s.replace('\'', "''")),
            Some(Value::Timestamp(s)) => format!("'{}'", s),
            Some(Value::Json(j)) => format!("'{}'", j.to_string().replace('\'', "''")),
            Some(Value::Bytes(b)) => {
                format!(
                    "'\\x{}'",
                    b.iter()
                        .map(|byte| format!("{:02x}", byte))
                        .collect::<String>()
                )
            }
        }
    }

    fn auto_increment_keyword(&self) -> Option<&str> {
        Some("GENERATED BY DEFAULT AS IDENTITY")
    }

    fn supports_explicit_identity_values(&self) -> bool {
        true
    }

    fn foreign_key_names_are_table_scoped(&self) -> bool {
        true
    }

    fn object_names_are_case_sensitive(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(name: &str, data_type: &str) -> ColumnSchema {
        ColumnSchema {
            name: name.into(),
            data_type: data_type.into(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        }
    }

    #[test]
    fn pg_varchar_to_ir() {
        let adapter = PgSyncAdapter;
        let c = col("name", "character varying");
        let ir = adapter.column_to_ir(&c, Some("character varying(255)"));
        assert_eq!(ir.ir_type, IRType::Varchar { length: Some(255) });
    }

    #[test]
    fn pg_numeric_to_ir() {
        let adapter = PgSyncAdapter;
        let c = col("price", "numeric");
        let ir = adapter.column_to_ir(&c, Some("numeric(10,2)"));
        assert_eq!(
            ir.ir_type,
            IRType::Decimal {
                precision: 10,
                scale: 2
            }
        );
    }

    #[test]
    fn transfer_preflight_rejects_postgres_arrays() {
        let adapter = PgSyncAdapter;
        assert!(adapter
            .validate_transfer_source_column(&col("tags", "text[]"))
            .unwrap_err()
            .contains("does not yet decode arrays"));
        assert!(adapter
            .validate_transfer_source_column(&col("tags", "ARRAY"))
            .unwrap_err()
            .contains("does not yet decode arrays"));
        assert!(adapter
            .validate_transfer_source_column(&col("name", "text"))
            .is_ok());
    }

    #[test]
    fn transfer_preflight_marks_custom_native_types_for_target_validation() {
        let adapter = PgSyncAdapter;
        let column = col("mood", "mood");
        let enum_ir = adapter.column_to_ir(&column, None);
        assert!(matches!(enum_ir.ir_type, IRType::Other(_)));
        assert!(adapter.transfer_source_type_is_native_only(&column, &enum_ir));
    }

    #[test]
    fn transfer_preflight_marks_postgres_character_collation_as_unproven() {
        let adapter = PgSyncAdapter;
        assert!(adapter.transfer_source_requires_collation_preservation(&col("body", "text")));
        assert!(
            adapter.transfer_source_requires_collation_preservation(&col(
                "label",
                "character varying(120)"
            ))
        );
        assert!(
            adapter.transfer_source_requires_collation_preservation(&col("code", "character(8)"))
        );
        assert!(!adapter.transfer_source_requires_collation_preservation(&col("id", "integer")));
    }

    #[test]
    fn pg_array_to_json() {
        let adapter = PgSyncAdapter;
        let c = col("tags", "text[]");
        let ir = adapter.column_to_ir(&c, Some("text[]"));
        assert_eq!(ir.ir_type, IRType::Json);
    }

    #[test]
    fn pg_bool_to_ir() {
        let adapter = PgSyncAdapter;
        let c = col("active", "boolean");
        let ir = adapter.column_to_ir(&c, None);
        assert_eq!(ir.ir_type, IRType::Bool);
    }

    #[test]
    fn pg_timestamp_tz() {
        let adapter = PgSyncAdapter;
        let c = col("created", "timestamptz");
        let ir = adapter.column_to_ir(&c, Some("timestamp with time zone"));
        assert_eq!(
            ir.ir_type,
            IRType::Timestamp {
                with_timezone: true
            }
        );
    }

    #[test]
    fn pg_default_nextval_skipped() {
        let adapter = PgSyncAdapter;
        let mut c = col("id", "integer");
        c.default_value = Some("nextval('users_id_seq'::regclass)".into());
        let ir = adapter.column_to_ir(&c, None);
        assert!(ir.default_expr.is_none());
    }

    #[test]
    fn pg_default_now() {
        let adapter = PgSyncAdapter;
        let mut c = col("created", "timestamp");
        c.default_value = Some("now()".into());
        let ir = adapter.column_to_ir(&c, None);
        assert_eq!(ir.default_expr, Some(IRDefault::CurrentTimestamp));
    }

    #[test]
    fn pg_default_cast_stripped() {
        let adapter = PgSyncAdapter;
        let mut c = col("status", "text");
        c.default_value = Some("'active'::text".into());
        let ir = adapter.column_to_ir(&c, None);
        assert_eq!(ir.default_expr, Some(IRDefault::Literal("'active'".into())));
    }

    #[test]
    fn pg_full_column_types_query_uses_format_type() {
        let adapter = PgSyncAdapter;
        let sql = adapter.full_column_types_query("public.users").unwrap();
        assert!(sql.contains("format_type"));
        assert!(sql.contains("\"public\".\"users\""));
        assert!(!sql.contains("database_type"));
    }

    #[test]
    fn full_types_preserve_schema_case_and_literal_dot_in_table() {
        let sql = PgSyncAdapter
            .full_column_types_query("Selected.literal.table")
            .unwrap();
        assert!(sql.contains("'\"Selected\".\"literal.table\"'::regclass"));
    }

    #[test]
    fn pg_target_roundtrip() {
        let adapter = PgSyncAdapter;
        assert_eq!(adapter.ir_type_to_native(&IRType::Bool), "boolean");
        assert_eq!(adapter.ir_type_to_native(&IRType::Int32), "integer");
        assert_eq!(adapter.ir_type_to_native(&IRType::Json), "jsonb");
        assert_eq!(adapter.ir_type_to_native(&IRType::Uuid), "uuid");
        assert_eq!(adapter.ir_type_to_native(&IRType::Blob), "bytea");
        assert_eq!(
            adapter.ir_type_to_native(&IRType::Varchar { length: Some(100) }),
            "character varying(100)"
        );
    }

    #[test]
    fn pg_char_and_bit_types() {
        let adapter = PgSyncAdapter;
        let char_col = col("code", "character(3)");
        let ir = adapter.column_to_ir(&char_col, Some("character(3)"));
        assert_eq!(ir.ir_type, IRType::Char { length: 3 });

        let bit_col = col("flags", "bit(4)");
        let ir = adapter.column_to_ir(&bit_col, Some("bit(4)"));
        assert_eq!(ir.ir_type, IRType::Bit { length: 4 });
    }

    #[test]
    fn pg_inet_and_money_map_to_ir() {
        let adapter = PgSyncAdapter;
        let inet = adapter.column_to_ir(&col("ip", "inet"), None);
        assert_eq!(inet.ir_type, IRType::Varchar { length: Some(45) });
        let money = adapter.column_to_ir(&col("price", "money"), None);
        assert_eq!(
            money.ir_type,
            IRType::Decimal {
                precision: 19,
                scale: 2
            }
        );
    }

    #[test]
    fn pg_sync_key_contract_covers_text_decimal_and_timestamp() {
        let adapter = PgSyncAdapter;
        let text = adapter.sync_key_contract(&col("name", "text")).unwrap();
        assert_eq!(
            text.kind,
            datazen_driver_api::SyncKeyKind::Text {
                collation: datazen_driver_api::SyncKeyCollation::Binary
            }
        );
        assert_eq!(
            adapter.sync_key_order_expression(r#""name""#, &text),
            r#""name" COLLATE "C""#
        );
        let decimal = adapter
            .sync_key_contract(&col("amount", "numeric(18,4)"))
            .unwrap();
        assert_eq!(
            decimal.kind,
            datazen_driver_api::SyncKeyKind::Decimal { scale: Some(4) }
        );
        let timestamp = adapter
            .sync_key_contract(&col("created", "timestamp with time zone"))
            .unwrap();
        assert_eq!(
            adapter
                .normalize_sync_key(
                    &Some(Value::String("2026-01-01T00:00:00+08:00".into())),
                    &timestamp,
                )
                .unwrap(),
            datazen_driver_api::SyncKeyValue::Timestamp("2025-12-31 16:00:00Z".into())
        );
    }

    #[test]
    fn pg_unknown_type_becomes_other() {
        let adapter = PgSyncAdapter;
        let ir = adapter.column_to_ir(&col("x", "hstore"), Some("hstore"));
        assert_eq!(ir.ir_type, IRType::Other("hstore".into()));
    }

    #[test]
    fn pg_format_default_and_literal() {
        let adapter = PgSyncAdapter;
        assert_eq!(
            adapter.format_default(&IRDefault::CurrentTimestamp),
            Some("now()".into())
        );
        assert_eq!(
            adapter.format_default(&IRDefault::Literal("'x'".into())),
            Some("'x'".into())
        );
        assert_eq!(adapter.format_literal(&None, &IRType::Text), "NULL");
        assert_eq!(
            adapter.format_literal(&Some(Value::Bool(true)), &IRType::Bool),
            "TRUE"
        );
        assert_eq!(
            adapter.format_literal(&Some(Value::String("O'Brien".into())), &IRType::Text),
            "'O''Brien'"
        );
        assert_eq!(
            adapter.format_literal(&Some(Value::Bytes(vec![0xde, 0xad])), &IRType::Blob),
            "'\\xdead'"
        );
    }

    #[test]
    fn pg_time_types_with_and_without_tz() {
        let adapter = PgSyncAdapter;
        let time = adapter.column_to_ir(&col("t", "time"), Some("time without time zone"));
        assert_eq!(
            time.ir_type,
            IRType::Time {
                with_timezone: false
            }
        );
        let timetz = adapter.column_to_ir(&col("tz", "timetz"), Some("time with time zone"));
        assert_eq!(
            timetz.ir_type,
            IRType::Time {
                with_timezone: true
            }
        );
    }

    #[test]
    fn pg_structure_preflight_detects_always_identity_and_name_scope_contract() {
        let adapter = PgSyncAdapter;
        let query = adapter
            .unsupported_transfer_structure_query("application", Some("public"), "users")
            .expect("PostgreSQL structure preflight");
        assert!(query.contains("to_jsonb(a)->>'attidentity'"));
        assert!(query.contains("GENERATED ALWAYS"));
        assert!(query.contains("opc.opcdefault"));
        assert!(query.contains("i.indoption"));
        assert!(query.contains("collations.collation_oid IS DISTINCT FROM a.attcollation"));
        assert!(query.contains("con.confmatchtype <> 's'"));
        assert!(query.contains("NOT con.convalidated"));
        assert!(query.contains("confdelsetcols"));
        assert!(!adapter.index_names_are_table_scoped());
        assert!(adapter.foreign_key_names_are_table_scoped());
        assert!(adapter.object_names_are_case_sensitive());
    }

    #[test]
    fn pg_structure_refuses_unmapped_mysql_collation_despite_utf8_encoding() {
        let options = datazen_driver_api::TableOptions {
            engine: Some("InnoDB".into()),
            charset: Some("utf8mb4".into()),
            collation: Some("utf8mb4_0900_ai_ci".into()),
            ..Default::default()
        };
        let error = PgSyncAdapter
            .render_source_table_options(&options)
            .expect_err("UTF8-compatible encoding does not prove collation equivalence");
        assert!(error.contains("utf8mb4_0900_ai_ci"), "{error}");
        assert!(error.contains("no proven equivalent"), "{error}");
        assert!(error.contains("UTF8MB4 character encoding can map to PostgreSQL UTF8"));
        assert!(error.contains("sort, case, or accent rules"));
    }

    #[test]
    fn test_tester_pg_structure_preflight_covers_sequence_ownership_and_options() {
        let query = PgSyncAdapter
            .unsupported_transfer_structure_query("application", Some("public"), "users")
            .expect("PostgreSQL structure preflight");
        let lower = query.to_ascii_lowercase();
        for required_catalog_evidence in [
            "pg_depend",
            "pg_sequence",
            "deptype in ('a', 'i')",
            "sequence_defaults.default_count <> 1",
            "sequence_owners.owner_count <> 1",
            "sequence_owners.owner_matches_column",
            "seqtypid",
            "seqstart",
            "seqincrement",
            "seqmin",
            "seqmax",
            "seqcache",
            "seqcycle",
            "nextval(",
        ] {
            assert!(
                lower.contains(required_catalog_evidence),
                "sequence structure preflight must inspect {required_catalog_evidence} so custom, shared, or non-default sequences are rejected before DDL"
            );
        }
    }
}
