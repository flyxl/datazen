use datazen_driver_api::{format_type, parse_type_parts, TypeNormalizer};

/// Normalizes only SQL Server synonyms whose storage and comparison semantics
/// are equivalent. Dimensions and suffixes remain part of the comparison.
pub struct SqlServerTypeNormalizer;

impl TypeNormalizer for SqlServerTypeNormalizer {
    fn normalize_type(&self, data_type: &str) -> String {
        let (base, args, suffix) = parse_type_parts(data_type);
        if base.is_empty() {
            return String::new();
        }
        let (canonical, effective_args) = match base.as_str() {
            "INTEGER" => ("INT", args),
            "DEC" | "NUMERIC" => ("DECIMAL", args),
            "CHARACTER VARYING" => ("VARCHAR", args),
            "CHARACTER" => ("CHAR", args),
            "NATIONAL CHARACTER VARYING" => ("NVARCHAR", args),
            "NATIONAL CHARACTER" => ("NCHAR", args),
            "DOUBLE PRECISION" => ("FLOAT", Some("53".into())),
            "FLOAT" if args.is_none() => ("FLOAT", Some("53".into())),
            "REAL" => ("FLOAT", Some("24".into())),
            "ROWVERSION" | "TIMESTAMP" => ("ROWVERSION", None),
            other => (other, args),
        };
        let normalized_args = effective_args
            .as_deref()
            .map(|args| args.split(',').map(str::trim).collect::<Vec<_>>().join(","));
        format_type(canonical, normalized_args.as_deref(), &suffix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_only_sql_server_type_synonyms() {
        let normalizer = SqlServerTypeNormalizer;
        assert_eq!(normalizer.normalize_type("integer"), "INT");
        assert_eq!(normalizer.normalize_type("NUMERIC(12, 4)"), "DECIMAL(12,4)");
        assert_eq!(
            normalizer.normalize_type("character varying(32)"),
            "VARCHAR(32)"
        );
        assert_eq!(
            normalizer.normalize_type("national character(4)"),
            "NCHAR(4)"
        );
        assert_eq!(normalizer.normalize_type("double precision"), "FLOAT(53)");
        assert_eq!(normalizer.normalize_type("real"), "FLOAT(24)");
        assert_eq!(normalizer.normalize_type("timestamp"), "ROWVERSION");
    }

    #[test]
    fn preserves_dimensions_collation_and_unicode_distinctions() {
        let normalizer = SqlServerTypeNormalizer;
        assert_ne!(
            normalizer.normalize_type("VARCHAR(20)"),
            normalizer.normalize_type("VARCHAR(21)")
        );
        assert_ne!(
            normalizer.normalize_type("VARCHAR(20)"),
            normalizer.normalize_type("NVARCHAR(20)")
        );
        assert_ne!(
            normalizer.normalize_type("DECIMAL(12,2)"),
            normalizer.normalize_type("DECIMAL(12,3)")
        );
        assert_ne!(
            normalizer.normalize_type("VARCHAR(20) COLLATE Latin1_General_100_CI_AS"),
            normalizer.normalize_type("VARCHAR(20) COLLATE Latin1_General_100_CS_AS")
        );
    }
}
