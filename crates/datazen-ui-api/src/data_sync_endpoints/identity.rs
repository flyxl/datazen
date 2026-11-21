#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EndpointIdentity {
    pub connection_id: String,
    pub database: String,
    pub schema: String,
}

pub fn is_same_endpoint_identity(source: &EndpointIdentity, target: &EndpointIdentity) -> bool {
    source.connection_id == target.connection_id
        && !source.database.is_empty()
        && !target.database.is_empty()
        && source.database == target.database
        && source.schema.trim() == target.schema.trim()
}

pub const VERIFIED_MIGRATION_TYPES: [&str; 2] = ["postgresql", "mysql"];

pub fn is_verified_migration_pair(source_type: &str, target_type: &str) -> bool {
    let source = source_type.to_lowercase();
    let target = target_type.to_lowercase();
    VERIFIED_MIGRATION_TYPES.contains(&source.as_str())
        && VERIFIED_MIGRATION_TYPES.contains(&target.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(conn: &str, db: &str, schema: &str) -> EndpointIdentity {
        EndpointIdentity {
            connection_id: conn.to_owned(),
            database: db.to_owned(),
            schema: schema.to_owned(),
        }
    }

    #[test]
    fn identity_requires_a_nonempty_shared_database() {
        assert!(is_same_endpoint_identity(
            &identity("c", "d", " public "),
            &identity("c", "d", "public")
        ));
        assert!(!is_same_endpoint_identity(
            &identity("c", "d", "a"),
            &identity("c", "d", "b")
        ));
        assert!(!is_same_endpoint_identity(
            &identity("c", "", "a"),
            &identity("c", "", "a")
        ));
        assert!(!is_same_endpoint_identity(
            &identity("c1", "d", "a"),
            &identity("c2", "d", "a")
        ));
    }

    #[test]
    fn verified_pairs_are_both_postgresql_or_mysql() {
        assert!(is_verified_migration_pair("PostgreSQL", "MYSQL"));
        assert!(is_verified_migration_pair("mysql", "postgresql"));
        assert!(!is_verified_migration_pair("postgresql", "sqlite"));
        assert!(!is_verified_migration_pair("sqlite", "postgresql"));
    }
}
