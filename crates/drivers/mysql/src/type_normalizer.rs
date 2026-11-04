use datazen_driver_api::{format_type, parse_type_parts, TypeNormalizer};

pub struct MysqlTypeNormalizer;

impl TypeNormalizer for MysqlTypeNormalizer {
    fn normalize_type(&self, data_type: &str) -> String {
        let (base, args, suffix) = parse_type_parts(data_type);
        if base.is_empty() {
            return String::new();
        }
        let canonical = match base.as_str() {
            "INTEGER" => "INT",
            "BOOL" | "BOOLEAN" => "BOOLEAN",
            "DEC" | "NUMERIC" => "DECIMAL",
            other => other,
        };
        let effective_args = if canonical == "BOOLEAN" || is_mysql_integer(canonical) {
            None
        } else {
            args.as_deref()
        };
        format_type(canonical, effective_args, &suffix)
    }
}

fn is_mysql_integer(base: &str) -> bool {
    matches!(
        base,
        "TINYINT" | "SMALLINT" | "MEDIUMINT" | "INT" | "BIGINT"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_aliases_and_display_width() {
        let n = MysqlTypeNormalizer;
        assert_eq!(n.normalize_type("INTEGER"), "INT");
        assert_eq!(n.normalize_type("int(11)"), "INT");
        assert_eq!(n.normalize_type("BIGINT(20)"), "BIGINT");
    }

    #[test]
    fn bool_and_decimal_aliases() {
        let n = MysqlTypeNormalizer;
        assert_eq!(n.normalize_type("BOOL"), "BOOLEAN");
        assert_eq!(n.normalize_type("DECIMAL(10,2)"), "DECIMAL(10,2)");
        assert_eq!(n.normalize_type("NUMERIC(10,2)"), "DECIMAL(10,2)");
    }

    #[test]
    fn unsigned_suffix_preserved() {
        let n = MysqlTypeNormalizer;
        assert_eq!(n.normalize_type("INT(11) UNSIGNED"), "INT UNSIGNED");
        assert_eq!(
            n.normalize_type("INTEGER UNSIGNED"),
            n.normalize_type("INT UNSIGNED")
        );
    }

    #[test]
    fn distinct_types_remain_distinct() {
        let n = MysqlTypeNormalizer;
        assert_ne!(n.normalize_type("INT"), n.normalize_type("BIGINT"));
        assert_ne!(n.normalize_type("INT"), n.normalize_type("INT UNSIGNED"));
        assert_ne!(n.normalize_type("TINYINT(1)"), n.normalize_type("BOOLEAN"));
    }
}
