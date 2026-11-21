#[derive(Clone, Debug, PartialEq, Eq)]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(connection_id: &str, database: &str, schema: &str) -> EndpointIdentity {
        EndpointIdentity {
            connection_id: connection_id.to_owned(),
            database: database.to_owned(),
            schema: schema.to_owned(),
        }
    }

    #[test]
    fn identical_endpoints_match() {
        let source = endpoint("c1", "main", "public");
        assert!(is_same_endpoint_identity(&source, &source));
        assert!(is_same_endpoint_identity(
            &endpoint("c1", "main", "public"),
            &endpoint("c1", "main", "public")
        ));
    }

    #[test]
    fn different_connection_id_never_matches() {
        assert!(!is_same_endpoint_identity(
            &endpoint("c1", "main", "public"),
            &endpoint("c2", "main", "public")
        ));
    }

    #[test]
    fn empty_database_on_either_side_never_matches() {
        assert!(!is_same_endpoint_identity(
            &endpoint("c1", "", "public"),
            &endpoint("c1", "", "public")
        ));
        assert!(!is_same_endpoint_identity(
            &endpoint("c1", "", "public"),
            &endpoint("c1", "main", "public")
        ));
        assert!(!is_same_endpoint_identity(
            &endpoint("c1", "main", "public"),
            &endpoint("c1", "", "public")
        ));
    }

    #[test]
    fn different_database_never_matches() {
        assert!(!is_same_endpoint_identity(
            &endpoint("c1", "main", "public"),
            &endpoint("c1", "other", "public")
        ));
    }

    #[test]
    fn database_is_not_trimmed_but_schema_is() {
        assert!(!is_same_endpoint_identity(
            &endpoint("c1", " main ", "public"),
            &endpoint("c1", "main", "public")
        ));
        assert!(is_same_endpoint_identity(
            &endpoint("c1", "main", " public "),
            &endpoint("c1", "main", "public")
        ));
        assert!(is_same_endpoint_identity(
            &endpoint("c1", "main", "  "),
            &endpoint("c1", "main", "")
        ));
    }

    #[test]
    fn whitespace_only_database_is_still_a_nonempty_string() {
        assert!(is_same_endpoint_identity(
            &endpoint("c1", "  ", "public"),
            &endpoint("c1", "  ", "public")
        ));
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let source = crate::EndpointIdentity {
            connection_id: "c".to_owned(),
            database: "d".to_owned(),
            schema: "s".to_owned(),
        };
        assert!(crate::is_same_endpoint_identity(&source, &source.clone()));
    }
}
