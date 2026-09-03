use datazen_driver_api::{format_type, parse_type_parts, TypeNormalizer};

pub struct SqliteTypeNormalizer;

impl TypeNormalizer for SqliteTypeNormalizer {
    fn normalize_type(&self, data_type: &str) -> String {
        let (base, args, suffix) = parse_type_parts(data_type);
        format_type(&base, args.as_deref(), &suffix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_normalization() {
        let n = SqliteTypeNormalizer;
        assert_eq!(n.normalize_type("text"), "TEXT");
        assert_eq!(n.normalize_type("TEXT"), "TEXT");
        assert_eq!(n.normalize_type("  integer  "), "INTEGER");
    }
}
