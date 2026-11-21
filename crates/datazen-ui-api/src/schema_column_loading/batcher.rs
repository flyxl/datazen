use std::collections::{BTreeMap, BTreeSet};

use super::{
    keys::{read_bucket, relation_columns_cache_key},
    RelationRef, SessionMetadataIdentity,
};

#[derive(Clone, Debug, PartialEq)]
pub struct BucketPlan {
    pub bucket: String,
    pub session: SessionMetadataIdentity,
    pub db_session_id: String,
    pub refs: BTreeMap<String, RelationRef>,
    pub waiter_keys: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnsurePlan {
    pub buckets: Vec<BucketPlan>,
}

impl EnsurePlan {
    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }

    pub fn ref_count(&self) -> usize {
        self.buckets.iter().map(|bucket| bucket.refs.len()).sum()
    }
}

pub fn build_ensure_plan(
    session: &SessionMetadataIdentity,
    db_session_id: &str,
    refs: &[RelationRef],
) -> EnsurePlan {
    let mut buckets: Vec<BucketPlan> = Vec::new();
    for relation in refs {
        let key = relation_columns_cache_key(session, db_session_id, relation);
        let bucket = read_bucket(session, db_session_id, relation);
        if let Some(index) = buckets.iter().position(|item| item.bucket == bucket) {
            let item = &mut buckets[index];
            item.refs.insert(key.clone(), relation.clone());
            item.waiter_keys.insert(key);
        } else {
            let mut plan = BucketPlan {
                bucket,
                session: session.clone(),
                db_session_id: db_session_id.to_owned(),
                refs: BTreeMap::new(),
                waiter_keys: BTreeSet::new(),
            };
            plan.refs.insert(key.clone(), relation.clone());
            plan.waiter_keys.insert(key);
            buckets.push(plan);
        }
    }
    EnsurePlan { buckets }
}

pub fn invalidate_plan(buckets: &[BucketPlan], db_session_id: &str) -> Vec<String> {
    buckets
        .iter()
        .filter(|bucket| bucket.db_session_id == db_session_id)
        .map(|bucket| bucket.bucket.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> SessionMetadataIdentity {
        SessionMetadataIdentity {
            connection_id: "c1".to_owned(),
            metadata_revision: 1,
        }
    }

    fn relation(schema: &str, name: &str) -> RelationRef {
        RelationRef {
            database: "shop".to_owned(),
            schema: Some(schema.to_owned()),
            name: name.to_owned(),
        }
    }

    #[test]
    fn empty_refs_produce_no_buckets() {
        let plan = build_ensure_plan(&session(), "s1", &[]);
        assert!(plan.is_empty());
        assert_eq!(plan.ref_count(), 0);
    }

    #[test]
    fn refs_are_grouped_by_namespace_and_deduplicated_by_key() {
        let refs = [
            relation("public", "users"),
            relation("public", "users"),
            relation("public", "orders"),
            relation("audit", "logs"),
        ];
        let plan = build_ensure_plan(&session(), "s1", &refs);
        assert_eq!(plan.buckets.len(), 2);
        let public = plan
            .buckets
            .iter()
            .find(|bucket| bucket.bucket.contains("public"))
            .expect("public bucket exists");
        assert_eq!(public.refs.len(), 2);
        assert_eq!(public.waiter_keys.len(), 2);
        assert_eq!(plan.ref_count(), 3);
    }

    #[test]
    fn session_id_is_part_of_the_bucket_identity() {
        let refs = [relation("public", "users")];
        let first = build_ensure_plan(&session(), "s1", &refs);
        let second = build_ensure_plan(&session(), "s2", &refs);
        assert_ne!(first.buckets[0].bucket, second.buckets[0].bucket);
    }

    #[test]
    fn invalidation_only_drops_matching_session_buckets() {
        let refs = [relation("public", "users")];
        let mut buckets = build_ensure_plan(&session(), "s1", &refs).buckets;
        buckets.extend(build_ensure_plan(&session(), "s2", &refs).buckets);
        let dropped = invalidate_plan(&buckets, "s1");
        assert_eq!(dropped.len(), 1);
        assert_eq!(dropped[0], buckets[0].bucket);
        assert!(invalidate_plan(&buckets, "s3").is_empty());
    }
}
