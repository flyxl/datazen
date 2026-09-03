use datazen_driver_api::{format_type, parse_type_parts, TypeNormalizer};

pub struct PostgresTypeNormalizer;

impl TypeNormalizer for PostgresTypeNormalizer {
    fn normalize_type(&self, data_type: &str) -> String {
        let (base, args, suffix) = parse_type_parts(data_type);
        if base.is_empty() {
            return String::new();
        }
        let canonical = match base.as_str() {
            "INT" | "INT4" => "INTEGER",
            "INT8" => "BIGINT",
            "INT2" => "SMALLINT",
            "FLOAT8" | "DOUBLE PRECISION" => "DOUBLE PRECISION",
            "FLOAT4" => "REAL",
            "BOOL" | "BOOLEAN" => "BOOLEAN",
            "CHARACTER VARYING" => "VARCHAR",
            "CHARACTER" => "CHAR",
            "TIMESTAMPTZ" => "TIMESTAMP WITH TIME ZONE",
            "TIMESTAMP WITHOUT TIME ZONE" => "TIMESTAMP",
            other => other,
        };
        let effective_args = if canonical == "BOOLEAN" {
            None
        } else {
            args.as_deref()
        };
        format_type(canonical, effective_args, &suffix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int4_integer_equivalent() {
        let n = PostgresTypeNormalizer;
        assert_eq!(n.normalize_type("int4"), "INTEGER");
        assert_eq!(n.normalize_type("integer"), "INTEGER");
        assert_eq!(n.normalize_type("INT"), "INTEGER");
    }

    #[test]
    fn varchar_and_timestamp_aliases() {
        let n = PostgresTypeNormalizer;
        assert_eq!(
            n.normalize_type("character varying(20)"),
            "VARCHAR(20)"
        );
        assert_eq!(
            n.normalize_type("timestamptz"),
            "TIMESTAMP WITH TIME ZONE"
        );
        assert_eq!(
            n.normalize_type("timestamp without time zone"),
            "TIMESTAMP"
        );
        assert_eq!(n.normalize_type("float8"), "DOUBLE PRECISION");
        assert_eq!(n.normalize_type("float4"), "REAL");
        assert_eq!(n.normalize_type("int2"), "SMALLINT");
        assert_eq!(n.normalize_type("character(2)"), "CHAR(2)");
    }

    #[test]
    fn bool_aliases_drop_args() {
        let n = PostgresTypeNormalizer;
        assert_eq!(n.normalize_type("BOOL"), "BOOLEAN");
        assert_eq!(n.normalize_type("boolean"), "BOOLEAN");
    }

    #[test]
    fn distinct_types_remain_distinct() {
        let n = PostgresTypeNormalizer;
        assert_ne!(n.normalize_type("INTEGER"), n.normalize_type("BIGINT"));
        assert_ne!(n.normalize_type("VARCHAR(20)"), n.normalize_type("VARCHAR(21)"));
    }
}
