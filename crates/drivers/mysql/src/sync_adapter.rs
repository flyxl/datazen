//! MySQL / MariaDB sync adapter.

use datazen_driver_api::{
    BoxedSyncAdapter, ColumnSchema, IRColumn, IRDefault, IRType, SyncAdapterFactory,
    SyncSourceAdapter, SyncTargetAdapter, Value,
};

pub struct MysqlSyncAdapter {
    pub is_mariadb: bool,
}

fn create_mysql() -> BoxedSyncAdapter {
    BoxedSyncAdapter::both(MysqlSyncAdapter { is_mariadb: false })
}

fn create_mariadb() -> BoxedSyncAdapter {
    BoxedSyncAdapter::both(MysqlSyncAdapter { is_mariadb: true })
}

datazen_driver_api::inventory::submit! {
    SyncAdapterFactory {
        // doris / starrocks / manticore / ob_oracle: MySQL wire + information_schema
        db_types: &["mysql", "doris", "starrocks", "manticore", "ob_oracle"],
        create: create_mysql,
    }
}

datazen_driver_api::inventory::submit! {
    SyncAdapterFactory {
        db_types: &["mariadb"],
        create: create_mariadb,
    }
}

// ── helpers ────────────────────────────────────────────────────────

fn strip_modifiers(s: &str) -> String {
    s.to_lowercase()
        .replace(" unsigned", "")
        .replace(" zerofill", "")
}

fn parse_length(s: &str, prefix: &str) -> Option<u32> {
    s.strip_prefix(prefix)
        .and_then(|r| r.trim().strip_prefix('('))
        .and_then(|r| r.strip_suffix(')'))
        .and_then(|n| n.trim().parse().ok())
}

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

fn parse_decimal_capacity(native: &str, prefix: &str) -> Result<Option<(u32, u32)>, String> {
    let lower = native.trim().to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix(prefix) else {
        return Ok(None);
    };
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok(None);
    }
    let Some(args) = rest
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
    else {
        return Err(format!(
            "cannot prove numeric precision for source type '{native}'"
        ));
    };
    let parts: Vec<_> = args.split(',').map(str::trim).collect();
    if parts.is_empty() || parts.len() > 2 {
        return Err(format!(
            "cannot prove numeric precision for source type '{native}'"
        ));
    }
    let precision = parts[0]
        .parse::<u32>()
        .map_err(|_| format!("cannot prove numeric precision for source type '{native}'"))?;
    let scale = parts
        .get(1)
        .map(|value| value.parse::<i32>())
        .transpose()
        .map_err(|_| format!("cannot prove numeric scale for source type '{native}'"))?
        .unwrap_or(0);
    if scale < 0 {
        return Err(format!(
            "source numeric type '{native}' uses a negative scale that MySQL cannot preserve"
        ));
    }
    Ok(Some((precision, scale as u32)))
}

fn mysql_text_capacity_bytes(native: &str) -> Option<u64> {
    let normalized = native.trim().to_ascii_lowercase();
    let (base, args) = normalized
        .split_once('(')
        .map(|(base, args)| (base.trim(), args.split(')').next().unwrap_or_default()))
        .unwrap_or((normalized.as_str(), ""));
    match base {
        "tinytext" | "tinyblob" => Some(255),
        "text" | "blob" => Some(65_535),
        "mediumtext" | "mediumblob" => Some(16_777_215),
        "longtext" | "longblob" => Some(4_294_967_295),
        "char" | "varchar" => args
            .split(',')
            .next()
            .and_then(|length| length.trim().parse::<u64>().ok())
            .and_then(|length| length.checked_mul(4))
            .map(|bytes| bytes.min(65_535)),
        _ => None,
    }
}

fn mysql_integer_capacity(native: &str) -> Option<(u8, bool)> {
    let normalized = native.trim().to_ascii_lowercase();
    let unsigned = normalized.contains("unsigned") || normalized.contains("zerofill");
    let base = normalized
        .split('(')
        .next()
        .unwrap_or_default()
        .trim()
        .trim_end_matches(" unsigned")
        .trim_end_matches(" zerofill");
    let bits = match base {
        "tinyint" => 8,
        "smallint" => 16,
        "mediumint" => 24,
        "int" | "integer" => 32,
        "bigint" => 64,
        _ => return None,
    };
    Some((bits, unsigned))
}

fn parse_mysql_default(raw: &str) -> Option<IRDefault> {
    let d = raw.trim();
    if d.is_empty() {
        return None;
    }
    if d == "CURRENT_TIMESTAMP" || d == "current_timestamp()" {
        return Some(IRDefault::CurrentTimestamp);
    }
    if d.parse::<i64>().is_ok()
        || d.parse::<f64>().is_ok()
        || matches!(d.to_ascii_lowercase().as_str(), "true" | "false" | "null")
        || (d.starts_with('\'') && d.ends_with('\''))
    {
        return Some(IRDefault::Literal(d.to_string()));
    }
    if d.contains('(')
        || d.contains(')')
        || d.contains("::")
        || d.contains('+')
        || d.contains('*')
        || d.contains('/')
        || d.contains('`')
    {
        return Some(IRDefault::RawExpression(d.to_string()));
    }
    Some(IRDefault::Literal(format!("'{}'", d.replace('\'', "''"))))
}

fn mysql_index_column_max_bytes(ir_type: &IRType, native: &str) -> Result<usize, String> {
    let checked_product = |length: usize, bytes_per_char: usize| {
        length
            .checked_mul(bytes_per_char)
            .ok_or_else(|| format!("target index key size overflows for type '{native}'"))
    };
    let native_length = || {
        native
            .trim()
            .split_once('(')
            .and_then(|(_, args)| args.split_once(')'))
            .and_then(|(length, _)| length.split(',').next())
            .and_then(|length| length.trim().parse::<usize>().ok())
    };
    let decimal_bytes = |precision: usize| precision.div_ceil(9).saturating_mul(4);
    let native_base = || {
        native
            .trim()
            .split('(')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_uppercase()
    };

    match ir_type {
        IRType::Bool | IRType::Int8 => Ok(1),
        IRType::Int16 => Ok(2),
        IRType::Int32 | IRType::Float32 => Ok(4),
        IRType::Int64 | IRType::Float64 => Ok(8),
        IRType::Decimal { precision, .. } => Ok(decimal_bytes(if *precision == 0 {
            65
        } else {
            usize::from(*precision)
        })),
        IRType::Char { length } => checked_product(*length as usize, 4),
        IRType::Varchar { length } => checked_product(length.map_or(255, |n| n as usize), 4),
        IRType::Binary { length: Some(length) } => Ok(*length as usize),
        IRType::Bit { length } => Ok((*length as usize).div_ceil(8)),
        IRType::Date => Ok(3),
        IRType::Time { .. } | IRType::Timestamp { .. } => Ok(8),
        IRType::Uuid => Ok(36 * 4),
        IRType::Other(_) => match native_base().as_str() {
            "CHAR" | "VARCHAR" => checked_product(native_length().ok_or_else(|| {
                format!("cannot prove target index key size for type '{native}'")
            })?, 4),
            "BINARY" | "VARBINARY" => native_length()
                .ok_or_else(|| format!("cannot prove target index key size for type '{native}'")),
            "TINYINT" | "BOOL" | "BOOLEAN" => Ok(1),
            "SMALLINT" => Ok(2),
            "MEDIUMINT" => Ok(3),
            "INT" | "INTEGER" | "FLOAT" => Ok(4),
            "BIGINT" | "DOUBLE" => Ok(8),
            "DECIMAL" | "NUMERIC" => Ok(decimal_bytes(native_length().ok_or_else(|| {
                format!("cannot prove target index key size for type '{native}'")
            })?)),
            "DATE" => Ok(3),
            "TIME" | "DATETIME" | "TIMESTAMP" => Ok(8),
            "YEAR" => Ok(1),
            _ => Err(format!(
                "cannot prove target index key size for type '{native}'; map the column to a known bounded type"
            )),
        },
        IRType::Text | IRType::Blob | IRType::Binary { length: None } | IRType::Json => {
            Err(format!(
                "ordinary MySQL indexes cannot represent target type '{native}' without a prefix or generated expression, which Data Transfer does not model"
            ))
        }
    }
}

// ── SyncSourceAdapter ──────────────────────────────────────────────

impl SyncSourceAdapter for MysqlSyncAdapter {
    fn transfer_source_text_limit_bytes(&self, column: &ColumnSchema) -> Option<u64> {
        mysql_text_capacity_bytes(&column.data_type)
    }

    fn transfer_source_requires_collation_preservation(&self, column: &ColumnSchema) -> bool {
        let native = column.data_type.trim().to_ascii_lowercase();
        let base = native.split('(').next().unwrap_or_default().trim();
        matches!(
            base,
            "char"
                | "character"
                | "varchar"
                | "character varying"
                | "nchar"
                | "nvarchar"
                | "tinytext"
                | "text"
                | "mediumtext"
                | "longtext"
                | "enum"
                | "set"
        ) || native.starts_with("national char")
            || native.starts_with("national varchar")
    }

    fn sync_key_order_expression(
        &self,
        quoted_column: &str,
        contract: &datazen_driver_api::SyncKeyContract,
    ) -> String {
        match &contract.kind {
            datazen_driver_api::SyncKeyKind::Text {
                collation: datazen_driver_api::SyncKeyCollation::Binary,
            } => format!("BINARY {quoted_column}"),
            _ => quoted_column.to_string(),
        }
    }

    fn column_to_ir(&self, column: &ColumnSchema, _native_full_type: Option<&str>) -> IRColumn {
        let base = strip_modifiers(&column.data_type);

        let ir_type = if base.starts_with("tinyint(1)") {
            IRType::Bool
        } else if base.starts_with("tinyint") {
            IRType::Int8
        } else if base.starts_with("smallint") {
            IRType::Int16
        } else if base.starts_with("mediumint") {
            IRType::Int32
        } else if base.starts_with("bigint") {
            IRType::Int64
        } else if base.starts_with("int(") || base == "int" || base == "integer" {
            IRType::Int32
        } else if base.starts_with("varchar") {
            let len = parse_length(&base, "varchar");
            IRType::Varchar { length: len }
        } else if base.starts_with("char(") {
            let len = parse_length(&base, "char").unwrap_or(1);
            IRType::Char { length: len }
        } else if base.starts_with("decimal") {
            let (p, s) = parse_precision(&base, "decimal");
            IRType::Decimal {
                precision: p,
                scale: s,
            }
        } else if base.starts_with("enum(")
            || base.starts_with("set(")
            || base == "enum"
            || base == "set"
        {
            IRType::Text
        } else if base.starts_with("varbinary") {
            let len = parse_length(&base, "varbinary");
            IRType::Binary { length: len }
        } else if base.starts_with("binary") {
            let len = parse_length(&base, "binary");
            IRType::Binary { length: len }
        } else if base.starts_with("bit(") || base == "bit" {
            if base == "bit(1)" || base == "bit" {
                IRType::Bool
            } else {
                let len = parse_length(&base, "bit").unwrap_or(1);
                IRType::Bit { length: len }
            }
        } else {
            match base.as_str() {
                "float" => IRType::Float32,
                "double" => IRType::Float64,
                "datetime" | "timestamp" => IRType::Timestamp {
                    with_timezone: false,
                },
                "date" => IRType::Date,
                "time" => IRType::Time {
                    with_timezone: false,
                },
                "year" => IRType::Int16,
                "text" | "longtext" | "mediumtext" | "tinytext" => IRType::Text,
                "blob" | "longblob" | "mediumblob" | "tinyblob" => IRType::Blob,
                "json" => IRType::Json,
                _ => IRType::Other(column.data_type.clone()),
            }
        };

        IRColumn {
            name: column.name.clone(),
            ir_type,
            nullable: column.nullable,
            default_expr: column
                .default_value
                .as_deref()
                .and_then(parse_mysql_default),
            is_primary_key: column.is_primary_key,
            is_auto_increment: column.is_auto_increment,
            comment: column.comment.clone(),
        }
    }

    fn unsupported_transfer_structure_query(
        &self,
        database: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Option<String> {
        // Hex + explicit UTF-8 conversion is independent of SQL mode, unlike
        // backslash escaping which changes under NO_BACKSLASH_ESCAPES.
        let catalog = if database.trim().is_empty() {
            schema.filter(|value| !value.trim().is_empty())
        } else {
            Some(database)
        };
        let schema_expr = catalog
            .map(mysql_utf8_hex_expression)
            .unwrap_or_else(|| "DATABASE()".into());
        let name_expr = mysql_utf8_hex_expression(table);
        Some(format!(
            "SELECT CONCAT('generated column ', COLUMN_NAME) AS unsupported_object \
             FROM information_schema.COLUMNS \
             WHERE TABLE_SCHEMA = {schema_expr} AND TABLE_NAME = {name_expr} \
               AND (UPPER(EXTRA) LIKE '%VIRTUAL GENERATED%' \
                    OR UPPER(EXTRA) LIKE '%STORED GENERATED%') \
             UNION ALL \
             SELECT CONCAT('index ', INDEX_NAME, ' (prefix, functional, or descending column)') \
             FROM information_schema.STATISTICS \
             WHERE TABLE_SCHEMA = {schema_expr} AND TABLE_NAME = {name_expr} \
               AND (SUB_PART IS NOT NULL OR COLUMN_NAME IS NULL OR COLLATION = 'D') \
             UNION ALL \
             SELECT CONCAT('column ', c.COLUMN_NAME, ' (column-specific collation ', c.COLLATION_NAME, ')') \
             FROM information_schema.COLUMNS c \
             JOIN information_schema.TABLES t \
               ON t.TABLE_SCHEMA = c.TABLE_SCHEMA AND t.TABLE_NAME = c.TABLE_NAME \
             WHERE c.TABLE_SCHEMA = {schema_expr} AND c.TABLE_NAME = {name_expr} \
               AND c.COLLATION_NAME IS NOT NULL AND t.TABLE_COLLATION IS NOT NULL \
               AND c.COLLATION_NAME <> t.TABLE_COLLATION \
             UNION ALL \
             SELECT CONCAT('table collation ', t.TABLE_COLLATION, ' (not the database default)') \
             FROM information_schema.TABLES t \
             JOIN information_schema.SCHEMATA s ON s.SCHEMA_NAME = t.TABLE_SCHEMA \
             WHERE t.TABLE_SCHEMA = {schema_expr} AND t.TABLE_NAME = {name_expr} \
               AND s.DEFAULT_COLLATION_NAME IS NOT NULL \
               AND t.TABLE_COLLATION <> s.DEFAULT_COLLATION_NAME"
        ))
    }
}

// ── SyncTargetAdapter ──────────────────────────────────────────────

impl SyncTargetAdapter for MysqlSyncAdapter {
    fn ir_type_to_native(&self, ir_type: &IRType) -> String {
        match ir_type {
            IRType::Bool => "TINYINT(1)".into(),
            IRType::Int8 => "TINYINT".into(),
            IRType::Int16 => "SMALLINT".into(),
            IRType::Int32 => "INT".into(),
            IRType::Int64 => "BIGINT".into(),
            IRType::Float32 => "FLOAT".into(),
            IRType::Float64 => "DOUBLE".into(),
            IRType::Decimal { precision: 0, .. } => "DECIMAL(65,30)".into(),
            IRType::Decimal { precision, scale } => format!("DECIMAL({precision},{scale})"),
            IRType::Char { length } => format!("CHAR({length})"),
            IRType::Varchar { length: Some(n) } => format!("VARCHAR({n})"),
            IRType::Varchar { length: None } => "VARCHAR(255)".into(),
            IRType::Text => "TEXT".into(),
            IRType::Binary { length: Some(n) } => format!("VARBINARY({n})"),
            IRType::Binary { length: None } | IRType::Blob => "LONGBLOB".into(),
            IRType::Date => "DATE".into(),
            IRType::Time { .. } => "TIME".into(),
            IRType::Timestamp { .. } => "DATETIME".into(),
            IRType::Json => "JSON".into(),
            IRType::Uuid => "CHAR(36)".into(),
            IRType::Bit { length } => format!("BIT({length})"),
            IRType::Other(native) => native.clone(),
        }
    }

    fn format_default(&self, default: &IRDefault) -> Option<String> {
        match default {
            IRDefault::CurrentTimestamp => Some("CURRENT_TIMESTAMP".into()),
            IRDefault::Literal(s) => Some(s.clone()),
            IRDefault::RawExpression(_) => None,
        }
    }

    fn allows_column_default(&self, ir_type: &IRType) -> bool {
        !matches!(
            ir_type,
            IRType::Text
                | IRType::Blob
                | IRType::Json
                | IRType::Binary { length: None }
                | IRType::Other(_)
        )
    }

    fn default_capable_type_for(&self, ir_type: &IRType) -> Option<IRType> {
        match ir_type {
            IRType::Text | IRType::Other(_) => Some(IRType::Varchar {
                // Shared sync/schema-diff renderer behavior; Data Transfer
                // uses the stricter transfer_default_capable_type_for below.
                length: Some(16_383),
            }),
            _ => None,
        }
    }

    fn transfer_ir_type_to_native(&self, ir_type: &IRType) -> String {
        match ir_type {
            IRType::Varchar { length: None } | IRType::Text => "LONGTEXT".into(),
            _ => self.ir_type_to_native(ir_type),
        }
    }

    fn transfer_allows_column_default(&self, ir_type: &IRType) -> bool {
        if matches!(ir_type, IRType::Text | IRType::Varchar { length: None }) {
            false
        } else {
            self.allows_column_default(ir_type)
        }
    }

    fn transfer_default_capable_type_for(&self, ir_type: &IRType) -> Option<IRType> {
        match ir_type {
            IRType::Text | IRType::Varchar { length: None } | IRType::Other(_) => None,
            _ => self.default_capable_type_for(ir_type),
        }
    }

    fn transfer_native_type_allows_column_default(&self, native_type: &str) -> Option<bool> {
        let normalized = native_type.trim().to_ascii_lowercase();
        let base = normalized
            .split('(')
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .next()
            .unwrap_or_default();
        match base {
            "tinytext" | "text" | "mediumtext" | "longtext" | "tinyblob" | "blob"
            | "mediumblob" | "longblob" | "json" | "geometry" | "point" | "linestring"
            | "polygon" | "multipoint" | "multilinestring" | "multipolygon"
            | "geometrycollection" => Some(false),
            "char" | "varchar" | "binary" | "varbinary" | "tinyint" | "smallint" | "mediumint"
            | "int" | "integer" | "bigint" | "decimal" | "numeric" | "float" | "double"
            | "real" | "date" | "time" | "datetime" | "timestamp" | "year" | "bit" | "enum"
            | "set" => Some(true),
            _ => None,
        }
    }

    fn validate_transfer_column_type(
        &self,
        source_column: &ColumnSchema,
        source_ir: &IRColumn,
        source_text_limit_bytes: Option<u64>,
        source_requires_collation_preservation: bool,
        source_type_is_native_only: bool,
        target_native_type: Option<&str>,
        creating_target: bool,
    ) -> Result<(), String> {
        let native_source = source_column.data_type.trim();
        let lower_source = native_source.to_ascii_lowercase();
        if source_type_is_native_only {
            return Err(format!(
                "source type '{native_source}' is represented only by its native name and has no supported MySQL mapping; convert custom types such as PostgreSQL enums to a portable scalar type at the source before transferring"
            ));
        }
        let source_integer_bits = match &source_ir.ir_type {
            IRType::Bool | IRType::Int8 => Some(8),
            IRType::Int16 => Some(16),
            IRType::Int32 => Some(32),
            IRType::Int64 => Some(64),
            _ => None,
        };
        if let Some(source_bits) = source_integer_bits {
            let target_native = match target_native_type
                .map(str::trim)
                .filter(|native| !native.is_empty())
            {
                Some(native) => native.to_string(),
                None if creating_target => self.transfer_ir_type_to_native(&source_ir.ir_type),
                None => {
                    return Err(
                        "target integer type could not be inspected; preview the existing target schema before transferring".into(),
                    );
                }
            };
            let (target_bits, unsigned) = mysql_integer_capacity(&target_native).ok_or_else(|| {
                format!("target type '{target_native}' cannot prove an integer range compatible with '{native_source}'")
            })?;
            if unsigned || target_bits < source_bits {
                return Err(format!(
                    "target type '{target_native}' cannot preserve the signed range of source type '{native_source}'"
                ));
            }
        }
        let decimal_source = if lower_source.starts_with("numeric") {
            parse_decimal_capacity(native_source, "numeric")?
        } else if lower_source.starts_with("decimal") {
            parse_decimal_capacity(native_source, "decimal")?
        } else if let IRType::Decimal { precision, scale } = &source_ir.ir_type {
            (*precision > 0).then_some((u32::from(*precision), u32::from(*scale)))
        } else {
            None
        };
        if lower_source == "numeric" || lower_source == "decimal" {
            if decimal_source.is_none() {
                return Err(format!(
                    "unbounded {native_source} cannot be represented exactly by MySQL DECIMAL (maximum precision 65 and scale 30)"
                ));
            }
        }
        if let Some((precision, scale)) = decimal_source {
            if precision == 0 || precision > 65 || scale > 30 || scale > precision {
                return Err(format!(
                    "source numeric type '{native_source}' exceeds MySQL DECIMAL capacity (precision 65, scale 30)"
                ));
            }
            let target_native = match target_native_type
                .map(str::trim)
                .filter(|native| !native.is_empty())
            {
                Some(native) => native.to_string(),
                None if creating_target => self.transfer_ir_type_to_native(&source_ir.ir_type),
                None => {
                    return Err(
                        "target numeric type could not be inspected; preview the existing target schema before transferring".into(),
                    );
                }
            };
            let lower_target = target_native.to_ascii_lowercase();
            let target_capacity = if lower_target.starts_with("decimal") {
                parse_decimal_capacity(&target_native, "decimal")?
            } else if lower_target.starts_with("numeric") {
                parse_decimal_capacity(&target_native, "numeric")?
            } else {
                None
            }
            .ok_or_else(|| {
                format!(
                    "target type '{target_native}' cannot preserve exact numeric values; map it to a sufficiently wide DECIMAL"
                )
            })?;
            let (target_precision, target_scale) = target_capacity;
            if target_precision < precision
                || target_scale < scale
                || target_precision.saturating_sub(target_scale) < precision.saturating_sub(scale)
            {
                return Err(format!(
                    "target type '{target_native}' is narrower than source numeric type '{native_source}'"
                ));
            }
        }

        match &source_ir.ir_type {
            IRType::Time {
                with_timezone: true,
            }
            | IRType::Timestamp {
                with_timezone: true,
            } => {
                return Err(format!(
                    "MySQL {native_source} has no timezone-preserving column type; convert the source to a timezone-free value before transfer"
                ));
            }
            _ => {}
        }

        if matches!(
            source_ir.ir_type,
            IRType::Text | IRType::Varchar { .. } | IRType::Char { .. }
        ) {
            if creating_target && source_requires_collation_preservation {
                return Err(format!(
                    "source type '{native_source}' requires collation preservation, but MySQL column collation equivalence is not represented or proven; choose a source/target collation mapping with explicit support before creating the table"
                ));
            }
            let target_native = match target_native_type
                .map(str::trim)
                .filter(|native| !native.is_empty())
            {
                Some(native) => native.to_string(),
                None if creating_target => self.transfer_ir_type_to_native(&source_ir.ir_type),
                None => {
                    return Err(
                        "target string type could not be inspected; preview the existing target schema before transferring".into(),
                    );
                }
            };
            let available_bytes = mysql_text_capacity_bytes(&target_native).ok_or_else(|| {
                format!("cannot prove target string capacity for type '{target_native}'")
            })?;
            if let Some(required_bytes) = source_text_limit_bytes {
                if available_bytes < required_bytes {
                    return Err(format!(
                        "target type '{target_native}' holds at most {available_bytes} bytes, below the source type's {required_bytes}-byte limit"
                    ));
                }
            }
        }
        Ok(())
    }

    fn validate_transfer_target_character_metadata(
        &self,
        source_ir: &IRColumn,
        target_native_type: &str,
        target_character_set: Option<&str>,
        target_collation: Option<&str>,
        creating_target: bool,
    ) -> Result<(), String> {
        if creating_target
            || !matches!(
                &source_ir.ir_type,
                IRType::Text | IRType::Varchar { .. } | IRType::Char { .. }
            )
        {
            return Ok(());
        }
        let character_set = target_character_set
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!(
                    "target column '{target_native_type}' character set was not inspected, so source text conversion cannot be proven"
                )
            })?;
        let collation = target_collation
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!(
                    "target column '{target_native_type}' collation was not inspected, so its character semantics cannot be confirmed"
                )
            })?;
        if !character_set.eq_ignore_ascii_case("utf8mb4") {
            return Err(format!(
                "target character set '{character_set}' cannot preserve every Unicode character from source text; use utf8mb4"
            ));
        }
        if !collation
            .to_ascii_lowercase()
            .starts_with(&format!("{}_", character_set.to_ascii_lowercase()))
        {
            return Err(format!(
                "target collation '{collation}' does not belong to character set '{character_set}'"
            ));
        }
        Ok(())
    }

    fn transfer_target_character_metadata_query(
        &self,
        database: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Option<String> {
        let catalog = if database.trim().is_empty() {
            schema.filter(|value| !value.trim().is_empty())
        } else {
            Some(database)
        };
        let schema_expr = catalog
            .map(mysql_utf8_hex_expression)
            .unwrap_or_else(|| "DATABASE()".into());
        let table_expr = mysql_utf8_hex_expression(table);
        Some(format!(
            "SELECT COLUMN_NAME, CHARACTER_SET_NAME, COLLATION_NAME \
             FROM information_schema.COLUMNS \
             WHERE TABLE_SCHEMA = {schema_expr} AND TABLE_NAME = {table_expr} \
             ORDER BY ORDINAL_POSITION"
        ))
    }

    fn format_literal(&self, value: &Option<Value>, _ir_type: &IRType) -> String {
        match value {
            None | Some(Value::Null) => "NULL".into(),
            Some(Value::Bool(b)) => if *b { "1" } else { "0" }.into(),
            Some(Value::Integer(n)) => n.to_string(),
            Some(Value::Float(f)) => f.to_string(),
            Some(Value::String(s)) => format!("'{}'", s.replace('\'', "''")),
            Some(Value::Timestamp(s)) => format!("'{}'", s),
            Some(Value::Json(j)) => format!("'{}'", j.to_string().replace('\'', "''")),
            Some(Value::Bytes(b)) => {
                format!(
                    "X'{}'",
                    b.iter()
                        .map(|byte| format!("{:02x}", byte))
                        .collect::<String>()
                )
            }
        }
    }

    fn quote_char(&self) -> char {
        '`'
    }

    fn qualify_relation(&self, database: &str, schema: Option<&str>, table: &str) -> String {
        let catalog = if database.trim().is_empty() {
            schema.filter(|value| !value.trim().is_empty())
        } else {
            Some(database)
        };
        match catalog {
            Some(catalog) => format!("{}.{}", self.quote_ident(catalog), self.quote_ident(table)),
            None => self.quote_ident(table),
        }
    }

    fn auto_increment_keyword(&self) -> Option<&str> {
        Some("AUTO_INCREMENT")
    }

    fn supports_explicit_identity_values(&self) -> bool {
        true
    }

    fn index_names_are_table_scoped(&self) -> bool {
        true
    }

    fn validate_index_column_type(&self, column: &IRColumn) -> Result<(), String> {
        let native = self.transfer_ir_type_to_native(&column.ir_type);
        let base_type = native
            .trim()
            .split('(')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_uppercase();
        if matches!(
            base_type.as_str(),
            "TINYTEXT"
                | "TEXT"
                | "MEDIUMTEXT"
                | "LONGTEXT"
                | "TINYBLOB"
                | "BLOB"
                | "MEDIUMBLOB"
                | "LONGBLOB"
                | "JSON"
        ) {
            return Err(format!(
                "target column '{}' has type '{native}'; ordinary MySQL indexes need a prefix or generated expression that Data Transfer does not model; map it to a bounded VARCHAR/BINARY type or omit the index",
                column.name
            ));
        }
        Ok(())
    }

    fn validate_index_columns(&self, columns: &[IRColumn]) -> Result<(), String> {
        let mut key_bytes = 0usize;
        for column in columns {
            self.validate_index_column_type(column)?;
            let native = self.transfer_ir_type_to_native(&column.ir_type);
            let column_bytes = mysql_index_column_max_bytes(&column.ir_type, &native)?;
            key_bytes = key_bytes
                .checked_add(column_bytes)
                .ok_or_else(|| "target index key size overflows".to_string())?;
        }

        // Use the 767-byte InnoDB key ceiling as a conservative bound. Newer
        // servers with dynamic row format may allow more, but this transfer
        // plan does not inspect page size or target row-format settings.
        const CONSERVATIVE_INNODB_KEY_LIMIT: usize = 767;
        if key_bytes > CONSERVATIVE_INNODB_KEY_LIMIT {
            return Err(format!(
                "target index key may require {key_bytes} bytes, exceeding this planner's conservative {CONSERVATIVE_INNODB_KEY_LIMIT}-byte InnoDB bound; this may reject keys on newer servers that support larger limits because target page size and row format are not inspected; narrow the mapped columns or wait for target-capability probing before relying on a larger limit"
            ));
        }
        Ok(())
    }

    fn render_source_table_options(
        &self,
        options: &datazen_driver_api::TableOptions,
    ) -> Result<Option<String>, String> {
        if let Some(collation) = options
            .collation
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Err(format!(
                "source table collation '{collation}' is not verified as available with equivalent semantics on the target MySQL-family server; confirm a supported target collation before retrying"
            ));
        }
        let mut parts = Vec::new();
        if let Some(engine) = options.engine.as_deref() {
            if !engine.eq_ignore_ascii_case("innodb") {
                return Err(format!("source table engine '{engine}' is unsupported"));
            }
            parts.push("ENGINE=InnoDB".to_string());
        }
        if let Some(charset) = options.charset.as_deref() {
            if charset.is_empty()
                || !charset
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            {
                return Err("source table character set is not a safe identifier".into());
            }
            parts.push(format!("DEFAULT CHARACTER SET={charset}"));
        }
        if let Some(comment) = options.comment.as_deref() {
            parts.push(format!("COMMENT='{}'", comment.replace('\'', "''")));
        }
        Ok((!parts.is_empty()).then(|| parts.join(" ")))
    }
}

fn mysql_utf8_hex_expression(value: &str) -> String {
    let hex = value
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<String>();
    format!("CONVERT(X'{hex}' USING utf8mb4)")
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

    fn adapter() -> MysqlSyncAdapter {
        MysqlSyncAdapter { is_mariadb: false }
    }

    fn ir_column(name: &str, ir_type: IRType) -> IRColumn {
        IRColumn {
            name: name.into(),
            ir_type,
            nullable: true,
            default_expr: None,
            is_primary_key: false,
            is_auto_increment: false,
            comment: None,
        }
    }

    #[test]
    fn mysql_tinyint1_is_bool() {
        let ir = adapter().column_to_ir(&col("active", "tinyint(1)"), None);
        assert_eq!(ir.ir_type, IRType::Bool);
    }

    #[test]
    fn mysql_varchar_to_ir() {
        let ir = adapter().column_to_ir(&col("name", "varchar(100)"), None);
        assert_eq!(ir.ir_type, IRType::Varchar { length: Some(100) });
    }

    #[test]
    fn mysql_unsigned_int() {
        let ir = adapter().column_to_ir(&col("age", "int unsigned"), None);
        assert_eq!(ir.ir_type, IRType::Int32);
    }

    #[test]
    fn mysql_enum_to_text() {
        let ir = adapter().column_to_ir(&col("status", "enum('a','b','c')"), None);
        assert_eq!(ir.ir_type, IRType::Text);
    }

    #[test]
    fn mysql_json() {
        let ir = adapter().column_to_ir(&col("data", "json"), None);
        assert_eq!(ir.ir_type, IRType::Json);
    }

    #[test]
    fn mysql_target_types() {
        let a = adapter();
        assert_eq!(a.ir_type_to_native(&IRType::Bool), "TINYINT(1)");
        assert_eq!(a.ir_type_to_native(&IRType::Int32), "INT");
        assert_eq!(a.ir_type_to_native(&IRType::Uuid), "CHAR(36)");
        assert_eq!(a.ir_type_to_native(&IRType::Json), "JSON");
        assert_eq!(a.ir_type_to_native(&IRType::Blob), "LONGBLOB");
        assert_eq!(
            a.ir_type_to_native(&IRType::Varchar { length: Some(255) }),
            "VARCHAR(255)"
        );
    }

    #[test]
    fn mysql_format_bool_literal() {
        let a = adapter();
        assert_eq!(
            a.format_literal(&Some(Value::Bool(true)), &IRType::Bool),
            "1"
        );
        assert_eq!(
            a.format_literal(&Some(Value::Bool(false)), &IRType::Bool),
            "0"
        );
    }

    #[test]
    fn mysql_mediumint_set_varbinary_bit() {
        let ir = adapter().column_to_ir(&col("n", "mediumint"), None);
        assert_eq!(ir.ir_type, IRType::Int32);
        let ir = adapter().column_to_ir(&col("s", "set('a','b')"), None);
        assert_eq!(ir.ir_type, IRType::Text);
        let ir = adapter().column_to_ir(&col("b", "varbinary(255)"), None);
        assert_eq!(ir.ir_type, IRType::Binary { length: Some(255) });
        let ir = adapter().column_to_ir(&col("f", "bit(8)"), None);
        assert_eq!(ir.ir_type, IRType::Bit { length: 8 });
    }

    #[test]
    fn mysql_default_and_other_type() {
        let mut c = col("ts", "timestamp");
        c.default_value = Some("current_timestamp()".into());
        let ir = adapter().column_to_ir(&c, None);
        assert_eq!(ir.default_expr, Some(IRDefault::CurrentTimestamp));

        let ir = adapter().column_to_ir(&col("x", "geometry"), None);
        assert_eq!(ir.ir_type, IRType::Other("geometry".into()));
    }

    #[test]
    fn mysql_unbounded_text_does_not_fall_back_to_a_narrower_default_type() {
        let a = adapter();
        assert_eq!(
            a.default_capable_type_for(&IRType::Text),
            Some(IRType::Varchar {
                length: Some(16_383)
            })
        );
        assert_eq!(a.transfer_default_capable_type_for(&IRType::Text), None);
        assert_eq!(
            a.transfer_default_capable_type_for(&IRType::Varchar { length: None }),
            None
        );
        assert!(a.allows_column_default(&IRType::Varchar { length: None }));
        assert!(!a.transfer_allows_column_default(&IRType::Varchar { length: None }));
        assert_eq!(
            a.transfer_default_capable_type_for(&IRType::Other("VARCHAR(64)".into())),
            None,
            "an explicit native type must never be rewritten to a fallback type"
        );
        assert_eq!(
            a.transfer_native_type_allows_column_default("VARCHAR(64)"),
            Some(true)
        );
        assert_eq!(
            a.transfer_native_type_allows_column_default("LONGTEXT"),
            Some(false)
        );
    }

    #[test]
    fn mysql_source_marks_character_columns_for_collation_preservation() {
        let adapter = adapter();
        for data_type in [
            "varchar(64)",
            "char(8)",
            "text",
            "tinytext",
            "enum('active','disabled')",
            "set('read','write')",
        ] {
            assert!(
                adapter.transfer_source_requires_collation_preservation(&col("value", data_type)),
                "{data_type} must fail closed when its column collation is unavailable"
            );
        }
        for data_type in ["integer", "varbinary(32)", "blob"] {
            assert!(
                !adapter.transfer_source_requires_collation_preservation(&col("value", data_type)),
                "{data_type} is not a character-collated MySQL type"
            );
        }
    }

    #[test]
    fn mysql_transfer_preflight_rejects_existing_string_targets_with_unknown_charset() {
        let adapter = adapter();
        let source_ir = ir_column("label", IRType::Varchar { length: Some(100) });
        adapter
            .validate_transfer_column_type(
                &col("label", "character varying(100)"),
                &source_ir,
                Some(400),
                true,
                false,
                Some("VARCHAR(255)"),
                false,
            )
            .expect("target type capacity should be checked independently of character metadata");
        let error = adapter
            .validate_transfer_target_character_metadata(
                &source_ir,
                "VARCHAR(255)",
                None,
                None,
                false,
            )
            .expect_err("uninspected target charset cannot prove source conversion");
        assert!(error.contains("character set was not inspected"), "{error}");
    }

    #[test]
    fn mysql_transfer_target_text_requires_verified_unicode_metadata() {
        let adapter = adapter();
        let source_ir = ir_column("body", IRType::Text);
        adapter
            .validate_transfer_target_character_metadata(
                &source_ir,
                "LONGTEXT",
                Some("utf8mb4"),
                Some("utf8mb4_0900_ai_ci"),
                false,
            )
            .expect("utf8mb4 preserves PostgreSQL Unicode text");

        let latin1 = adapter
            .validate_transfer_target_character_metadata(
                &source_ir,
                "LONGTEXT",
                Some("latin1"),
                Some("latin1_swedish_ci"),
                false,
            )
            .expect_err("latin1 cannot represent every PostgreSQL Unicode value");
        assert!(latin1.contains("cannot preserve every Unicode"), "{latin1}");

        let mismatched = adapter
            .validate_transfer_target_character_metadata(
                &source_ir,
                "LONGTEXT",
                Some("utf8mb4"),
                Some("latin1_swedish_ci"),
                false,
            )
            .expect_err("collation must belong to its inspected character set");
        assert!(mismatched.contains("does not belong"), "{mismatched}");
    }

    #[test]
    fn mysql_transfer_target_character_query_scopes_catalog_and_escapes_names() {
        let sql = adapter()
            .transfer_target_character_metadata_query("datazen_sync_mysql_tgt", None, "données'\\x")
            .expect("MySQL target character metadata query");
        assert!(sql.contains("information_schema.COLUMNS"));
        assert!(sql.contains("CHARACTER_SET_NAME, COLLATION_NAME"));
        assert!(
            sql.contains("CONVERT(X'646174617A656E5F73796E635F6D7973716C5F746774' USING utf8mb4)")
        );
        assert!(sql.contains("CONVERT(X'646F6E6EC3A96573275C78' USING utf8mb4)"));
    }

    #[test]
    fn mysql_transfer_preflight_rejects_unbounded_and_overflow_numeric() {
        let adapter = adapter();
        let unbounded = adapter
            .validate_transfer_column_type(
                &col("amount", "numeric"),
                &ir_column(
                    "amount",
                    IRType::Decimal {
                        precision: 0,
                        scale: 0,
                    },
                ),
                None,
                false,
                false,
                Some("DECIMAL(65,30)"),
                true,
            )
            .unwrap_err();
        assert!(unbounded.contains("unbounded numeric"), "{unbounded}");

        let overflow = adapter
            .validate_transfer_column_type(
                &col("amount", "numeric(256,2)"),
                &ir_column(
                    "amount",
                    IRType::Decimal {
                        precision: 0,
                        scale: 2,
                    },
                ),
                None,
                false,
                false,
                Some("DECIMAL(65,30)"),
                true,
            )
            .unwrap_err();
        assert!(
            overflow.contains("exceeds MySQL DECIMAL capacity"),
            "{overflow}"
        );
    }

    #[test]
    fn mysql_transfer_preflight_rejects_source_native_only_types() {
        let adapter = adapter();
        let error = adapter
            .validate_transfer_column_type(
                &col("mood", "mood"),
                &ir_column("mood", IRType::Other("mood".into())),
                None,
                false,
                true,
                Some("mood"),
                true,
            )
            .unwrap_err();
        assert!(error.contains("no supported MySQL mapping"), "{error}");
    }

    #[test]
    fn mysql_transfer_preflight_treats_zerofill_targets_as_unsigned() {
        let adapter = adapter();
        let error = adapter
            .validate_transfer_column_type(
                &col("id", "integer"),
                &ir_column("id", IRType::Int32),
                None,
                false,
                false,
                Some("INT ZEROFILL"),
                false,
            )
            .unwrap_err();
        assert!(
            error.contains("cannot preserve the signed range"),
            "{error}"
        );
    }

    #[test]
    fn mysql_transfer_preflight_compares_exact_decimal_capacity() {
        let adapter = adapter();
        let source = col("amount", "numeric(18,4)");
        let source_ir = ir_column(
            "amount",
            IRType::Decimal {
                precision: 18,
                scale: 4,
            },
        );
        adapter
            .validate_transfer_column_type(
                &source,
                &source_ir,
                None,
                false,
                false,
                Some("DECIMAL(20,6)"),
                false,
            )
            .expect("wider exact DECIMAL target preserves source values");
        let error = adapter
            .validate_transfer_column_type(
                &source,
                &source_ir,
                None,
                false,
                false,
                Some("DECIMAL(18,5)"),
                false,
            )
            .expect_err("wider scale with less integer capacity is narrower");
        assert!(error.contains("narrower"), "{error}");
        let missing_target = adapter
            .validate_transfer_column_type(&source, &source_ir, None, false, false, None, false)
            .expect_err("data-only numeric mappings require an inspected target type");
        assert!(
            missing_target.contains("target numeric type"),
            "{missing_target}"
        );
    }

    #[test]
    fn mysql_transfer_preflight_rejects_narrow_or_uninspected_integer_targets() {
        let adapter = adapter();
        let source = col("id", "integer");
        let source_ir = ir_column("id", IRType::Int32);
        let narrow = adapter
            .validate_transfer_column_type(
                &source,
                &source_ir,
                None,
                false,
                false,
                Some("SMALLINT"),
                false,
            )
            .unwrap_err();
        assert!(
            narrow.contains("cannot preserve the signed range"),
            "{narrow}"
        );

        let missing = adapter
            .validate_transfer_column_type(&source, &source_ir, None, false, false, None, false)
            .unwrap_err();
        assert!(
            missing.contains("target integer type could not be inspected"),
            "{missing}"
        );
    }

    #[test]
    fn mysql_transfer_preflight_rejects_timezone_and_narrow_text_types() {
        let adapter = adapter();
        for ir_type in [
            IRType::Time {
                with_timezone: true,
            },
            IRType::Timestamp {
                with_timezone: true,
            },
        ] {
            let error = adapter
                .validate_transfer_column_type(
                    &col("occurred_at", "timestamp with time zone"),
                    &ir_column("occurred_at", ir_type),
                    None,
                    false,
                    false,
                    Some("DATETIME"),
                    true,
                )
                .unwrap_err();
            assert!(error.contains("no timezone-preserving"), "{error}");
        }

        let source = col("body", "text");
        let text_ir = ir_column("body", IRType::Text);
        let pg_text_limit = Some(1_073_741_823);
        let error = adapter
            .validate_transfer_column_type(
                &source,
                &text_ir,
                pg_text_limit,
                false,
                false,
                Some("TEXT"),
                false,
            )
            .unwrap_err();
        assert!(error.contains("below the source type's"), "{error}");
        adapter
            .validate_transfer_column_type(
                &source,
                &text_ir,
                pg_text_limit,
                false,
                false,
                Some("LONGTEXT"),
                true,
            )
            .expect("LONGTEXT preserves PostgreSQL's maximum text size");
        let collation_error = adapter
            .validate_transfer_column_type(
                &source,
                &text_ir,
                pg_text_limit,
                true,
                false,
                Some("LONGTEXT"),
                true,
            )
            .unwrap_err();
        assert!(
            collation_error.contains("collation equivalence"),
            "{collation_error}"
        );
    }

    #[test]
    fn mysql_text_column_default_not_allowed() {
        let a = adapter();
        assert!(!a.allows_column_default(&IRType::Text));
        assert!(a.allows_column_default(&IRType::Varchar { length: None }));
        assert!(!a.allows_column_default(&IRType::Blob));
        assert!(!a.allows_column_default(&IRType::Json));
        assert!(a.allows_column_default(&IRType::Varchar { length: Some(100) }));
    }

    #[test]
    fn mysql_target_and_format_helpers() {
        let a = adapter();
        assert_eq!(a.ir_type_to_native(&IRType::Int8), "TINYINT");
        assert_eq!(
            a.ir_type_to_native(&IRType::Decimal {
                precision: 0,
                scale: 0
            }),
            "DECIMAL(65,30)"
        );
        assert_eq!(
            a.ir_type_to_native(&IRType::Varchar { length: None }),
            "VARCHAR(255)"
        );
        assert_eq!(
            a.transfer_ir_type_to_native(&IRType::Varchar { length: None }),
            "LONGTEXT"
        );
        assert_eq!(a.ir_type_to_native(&IRType::Text), "TEXT");
        assert_eq!(a.transfer_ir_type_to_native(&IRType::Text), "LONGTEXT");
        assert_eq!(
            a.ir_type_to_native(&IRType::Binary { length: Some(16) }),
            "VARBINARY(16)"
        );
        assert_eq!(a.ir_type_to_native(&IRType::Bit { length: 4 }), "BIT(4)");
        assert_eq!(
            a.format_default(&IRDefault::Literal("0".into())),
            Some("0".into())
        );
        assert!(a
            .format_default(&IRDefault::RawExpression("x".into()))
            .is_none());
        assert_eq!(a.quote_char(), '`');
        assert_eq!(a.auto_increment_keyword(), Some("AUTO_INCREMENT"));
        assert_eq!(a.format_literal(&None, &IRType::Int32), "NULL");
        assert_eq!(
            a.format_literal(&Some(Value::Bytes(vec![0xFF])), &IRType::Blob),
            "X'ff'"
        );
        assert!(a.index_names_are_table_scoped());
        assert!(!a.foreign_key_names_are_table_scoped());
    }

    #[test]
    fn mysql_structure_preflight_literals_are_sql_mode_independent() {
        let query = adapter()
            .unsupported_transfer_structure_query("données", None, "tbl'\\x")
            .expect("MySQL preflight query");
        assert!(query.contains(&mysql_utf8_hex_expression("données")));
        assert!(query.contains(&mysql_utf8_hex_expression("tbl'\\x")));
        assert!(!query.contains("tbl'\\x"));
        assert!(query.contains("COLLATION = 'D'"));
        assert!(query.contains("VIRTUAL GENERATED"));
        assert!(query.contains("STORED GENERATED"));
        assert!(!query.contains("EXTRA LIKE '%GENERATED%'"));
        assert!(query.contains("column-specific collation"));
        assert!(query.contains("not the database default"));
    }

    #[test]
    fn mysql_transfer_renders_captured_table_options_and_rejects_unknown_engine() {
        let a = adapter();
        let options = datazen_driver_api::TableOptions {
            engine: Some("InnoDB".into()),
            charset: Some("utf8mb4".into()),
            comment: Some("orders' archive".into()),
            ..Default::default()
        };
        assert_eq!(
            a.render_source_table_options(&options).unwrap().as_deref(),
            Some("ENGINE=InnoDB DEFAULT CHARACTER SET=utf8mb4 COMMENT='orders'' archive'")
        );

        let unsupported = datazen_driver_api::TableOptions {
            engine: Some("MyISAM".into()),
            ..Default::default()
        };
        assert!(a.render_source_table_options(&unsupported).is_err());

        let unverified_collation = datazen_driver_api::TableOptions {
            engine: Some("InnoDB".into()),
            charset: Some("utf8mb4".into()),
            collation: Some("utf8mb4_0900_ai_ci".into()),
            ..Default::default()
        };
        let error = a
            .render_source_table_options(&unverified_collation)
            .expect_err("target server version/collation availability is not proven");
        assert!(error.contains("utf8mb4_0900_ai_ci"), "{error}");
        assert!(error.contains("not verified as available"), "{error}");
    }

    #[test]
    fn mysql_index_preflight_rejects_unbounded_text_and_allows_bounded_varchar() {
        let adapter = adapter();
        let text = IRColumn {
            name: "label".into(),
            ir_type: IRType::Text,
            nullable: false,
            default_expr: None,
            is_primary_key: false,
            is_auto_increment: false,
            comment: None,
        };
        let error = adapter
            .validate_index_column_type(&text)
            .expect_err("MySQL requires a prefix to index unbounded TEXT");
        assert!(error.contains("TEXT"), "{error}");
        assert!(error.contains("prefix"), "{error}");

        let varchar = IRColumn {
            ir_type: IRType::Varchar { length: Some(120) },
            ..text
        };
        adapter
            .validate_index_column_type(&varchar)
            .expect("bounded VARCHAR can use an ordinary MySQL index");
    }

    #[test]
    fn mysql_sync_key_contract_is_binary_and_exact() {
        let a = adapter();
        let text = a.sync_key_contract(&col("name", "varchar(100)")).unwrap();
        assert_eq!(
            a.sync_key_order_expression("`name`", &text),
            "BINARY `name`"
        );
        let decimal = a
            .sync_key_contract(&col("amount", "decimal(18,4)"))
            .unwrap();
        let left = a
            .normalize_sync_key(&Some(Value::String("1.20".into())), &decimal)
            .unwrap();
        let right = a
            .normalize_sync_key(&Some(Value::String("1.2".into())), &decimal)
            .unwrap();
        assert_eq!(left, right);
        let unsupported = a.sync_key_contract(&col("id", "float")).unwrap_err();
        assert!(unsupported.contains("normalized"));
    }
}
