pub const VERIFIED_MIGRATION_TYPES: [&str; 2] = ["postgresql", "mysql"];

pub fn is_verified_migration_pair(source_type: &str, target_type: &str) -> bool {
    VERIFIED_MIGRATION_TYPES.iter().any(|candidate| candidate.eq_ignore_ascii_case(source_type))
        && VERIFIED_MIGRATION_TYPES.iter().any(|candidate| candidate.eq_ignore_ascii_case(target_type))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_sides_must_be_in_the_verified_set() {
        assert!(is_verified_migration_pair("postgresql", "mysql"));
        assert!(is_verified_migration_pair("MYSQL", "PostgreSQL"));
        assert!(!is_verified_migration_pair("postgresql", "sqlite"));
        assert!(!is_verified_migration_pair("sqlite", "mysql"));
        assert!(!is_verified_migration_pair("", "mysql"));
        assert!(!is_verified_migration_pair("postgresql", ""));
    }

    #[test]
    fn the_release_set_has_exactly_two_members() {
        assert_eq!(VERIFIED_MIGRATION_TYPES, ["postgresql", "mysql"]);
    }
}
