//! Type-name parsing and dialect built-in classification for unified plans.

use super::{object_identity::SchemaObjectIdentity, types::normalize_dialect};

pub(super) fn type_parts(raw: &str, dialect: &str) -> Vec<(String, bool)> {
    let mut value = raw.trim();
    while let Some(stripped) = value.strip_suffix("[]") {
        value = stripped.trim_end();
    }
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let chars = value.char_indices().collect::<Vec<_>>();
    for (offset, character) in chars.iter().copied() {
        match (quote, character) {
            (Some('"'), '"') | (Some('`'), '`') => quote = None,
            (None, '"' | '`') => quote = Some(character),
            (None, '(') => {
                value = value[..offset].trim_end();
                break;
            }
            (None, '.') => {
                parts.push(value[start..offset].trim().to_owned());
                start = offset + character.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(value[start..].trim().to_owned());
    parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .map(|part| {
            let quoted = part.len() >= 2
                && ((part.starts_with('"') && part.ends_with('"'))
                    || (part.starts_with('`') && part.ends_with('`')));
            let unquoted = if quoted {
                part[1..part.len() - 1]
                    .replace("\"\"", "\"")
                    .replace("``", "`")
            } else if normalize_dialect(dialect) == "postgresql" {
                part.to_ascii_lowercase()
            } else {
                part
            };
            (unquoted, quoted)
        })
        .collect()
}

pub(super) fn type_name_matches(raw: &str, identity: &SchemaObjectIdentity, dialect: &str) -> bool {
    let parts = type_parts(raw, dialect);
    let Some(last) = parts.last() else {
        return false;
    };
    let is_builtin = (!last.1 && is_builtin_type(&last.0, dialect))
        || (parts.len() > 1
            && parts[parts.len() - 2].0 == "pg_catalog"
            && is_builtin_type(&last.0, dialect));
    !is_builtin
        && last.0 == identity.name
        && (parts.len() < 2
            || identity
                .schema
                .as_ref()
                .is_some_and(|schema| parts[parts.len() - 2].0 == *schema))
}

pub(super) fn is_builtin_type(raw: &str, dialect: &str) -> bool {
    let ty = raw.to_ascii_lowercase();
    let common = [
        "bool",
        "boolean",
        "tinyint",
        "smallint",
        "int",
        "integer",
        "bigint",
        "real",
        "float",
        "double",
        "numeric",
        "decimal",
        "number",
        "char",
        "character",
        "varchar",
        "text",
        "date",
        "time",
        "timestamp",
        "double precision",
        "character varying",
        "character",
    ];
    if common.contains(&ty.as_str()) {
        return true;
    }
    match normalize_dialect(dialect).as_str() {
        "postgresql" => [
            "int2",
            "int4",
            "int8",
            "float4",
            "float8",
            "smallserial",
            "serial",
            "bigserial",
            "timestamptz",
            "timetz",
            "varbit",
            "bit",
            "bit varying",
            "name",
            "inet",
            "cidr",
            "macaddr",
            "macaddr8",
            "point",
            "line",
            "lseg",
            "box",
            "path",
            "polygon",
            "circle",
            "tsvector",
            "tsquery",
            "gtsvector",
            "regclass",
            "regcollation",
            "regconfig",
            "regdictionary",
            "regnamespace",
            "regoper",
            "regoperator",
            "regproc",
            "regprocedure",
            "regrole",
            "regtype",
            "xid",
            "xid8",
            "cid",
            "tid",
            "pg_lsn",
            "pg_snapshot",
            "txid_snapshot",
            "aclitem",
            "record",
            "void",
            "internal",
            "any",
            "anyelement",
            "anyarray",
            "anynonarray",
            "anyenum",
            "anyrange",
            "anymultirange",
            "anycompatible",
            "anycompatiblearray",
            "anycompatiblenonarray",
            "anycompatiblerange",
            "anycompatiblemultirange",
            "money",
            "interval",
            "xml",
            "oid",
            "bytea",
            "json",
            "jsonb",
            "uuid",
            "timestamp with time zone",
            "timestamp without time zone",
            "time with time zone",
            "time without time zone",
            "int4range",
            "int8range",
            "numrange",
            "tsrange",
            "tstzrange",
            "daterange",
            "int4multirange",
            "int8multirange",
            "nummultirange",
            "tsmultirange",
            "tstzmultirange",
            "datemultirange",
        ]
        .contains(&ty.as_str()),
        "mysql" => [
            "tinyint",
            "mediumint",
            "nchar",
            "nvarchar",
            "longtext",
            "mediumtext",
            "tinytext",
            "tinyblob",
            "mediumblob",
            "longblob",
            "blob",
            "binary",
            "varbinary",
            "json",
            "year",
            "enum",
            "set",
            "geometry",
            "point",
            "linestring",
            "polygon",
            "multipoint",
            "multilinestring",
            "multipolygon",
            "geometrycollection",
            "inet4",
            "inet6",
            "serial",
            "datetime",
            "timestamp",
            "time",
            "date",
        ]
        .contains(&ty.as_str()),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::is_builtin_type;

    #[test]
    fn recognizes_postgres_geometric_network_text_search_and_reg_types() {
        for ty in [
            "point",
            "polygon",
            "inet",
            "macaddr8",
            "tsvector",
            "tsquery",
            "regclass",
            "regnamespace",
            "pg_lsn",
        ] {
            assert!(is_builtin_type(ty, "postgresql"), "{ty}");
        }
        assert!(!is_builtin_type("geometry", "postgresql"));
    }

    #[test]
    fn recognizes_mysql_spatial_and_json_types_without_pg_crossovers() {
        for ty in [
            "geometry",
            "point",
            "linestring",
            "multipolygon",
            "geometrycollection",
            "json",
            "inet6",
        ] {
            assert!(is_builtin_type(ty, "mysql"), "{ty}");
        }
        assert!(!is_builtin_type("jsonb", "mysql"));
    }
}
