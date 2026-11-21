use crate::{
    metadata_batch_bucket, metadata_cache_key, ExecutionTarget, MetadataIdentityInput,
    MetadataTarget, NamespaceTarget, ObjectTarget,
};

use super::{RelationRef, SessionMetadataIdentity};

pub fn read_identity(
    session: &SessionMetadataIdentity,
    db_session_id: &str,
    relation: &RelationRef,
) -> MetadataIdentityInput {
    MetadataIdentityInput {
        connection_id: session.connection_id.clone(),
        config_revision: session.metadata_revision,
        db_session_id: db_session_id.to_owned(),
        target: MetadataTarget::Execution(ExecutionTarget {
            connection_id: session.connection_id.clone(),
            namespace: NamespaceTarget {
                database: Some(relation.database.clone()),
                catalog: None,
                schema: relation.schema.clone(),
                path: Vec::new(),
            },
            object: Some(ObjectTarget::new("relation", &relation.name)),
        }),
    }
}

pub fn relation_columns_cache_key(
    session: &SessionMetadataIdentity,
    db_session_id: &str,
    relation: &RelationRef,
) -> String {
    metadata_cache_key(&read_identity(session, db_session_id, relation))
}

pub fn read_bucket(
    session: &SessionMetadataIdentity,
    db_session_id: &str,
    relation: &RelationRef,
) -> String {
    metadata_batch_bucket(&read_identity(session, db_session_id, relation))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> SessionMetadataIdentity {
        SessionMetadataIdentity {
            connection_id: "c1".to_owned(),
            metadata_revision: 7,
        }
    }

    fn relation(schema: Option<&str>, name: &str) -> RelationRef {
        RelationRef {
            database: "shop".to_owned(),
            schema: schema.map(str::to_owned),
            name: name.to_owned(),
        }
    }

    #[test]
    fn cache_key_comes_from_the_shared_metadata_derivation() {
        let relation = relation(Some("public"), "users");
        let key = relation_columns_cache_key(&session(), "s1", &relation);
        assert_eq!(
            key,
            metadata_cache_key(&read_identity(&session(), "s1", &relation))
        );
    }

    #[test]
    fn isolation_dimensions_change_the_bucket() {
        let base = relation(Some("public"), "users");
        let baseline = read_bucket(&session(), "s1", &base);
        assert_eq!(baseline, read_bucket(&session(), "s1", &base));
        assert_ne!(baseline, read_bucket(&session(), "s2", &base));
        assert_ne!(
            baseline,
            read_bucket(
                &SessionMetadataIdentity {
                    connection_id: "c1".to_owned(),
                    metadata_revision: 8
                },
                "s1",
                &base
            )
        );
        assert_ne!(
            baseline,
            read_bucket(&session(), "s1", &relation(Some("audit"), "users"))
        );
        assert_ne!(
            baseline,
            read_bucket(&session(), "s1", &relation(None, "users"))
        );
    }
}
