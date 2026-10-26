//! Exact dependency-catalog comparisons retained by reviewed schema plans.

use crate::schema_diff::object_identity::{
    SchemaObjectDependencySnapshot, SchemaObjectIdentity, SequenceDependencyUsage,
    TypeDependencyUsage,
};
use std::collections::{BTreeSet, HashMap};

pub fn validate_object_dependency_catalog(
    reviewed: &[SchemaObjectDependencySnapshot],
    current: &[SchemaObjectDependencySnapshot],
) -> Result<(), String> {
    fn canonicalize(
        entries: &[SchemaObjectDependencySnapshot],
    ) -> Result<
        HashMap<
            SchemaObjectIdentity,
            (
                Option<Vec<SchemaObjectIdentity>>,
                Option<Vec<TypeDependencyUsage>>,
                Option<Vec<SequenceDependencyUsage>>,
            ),
        >,
        String,
    > {
        let mut catalog = HashMap::with_capacity(entries.len());
        for entry in entries {
            let mut dependencies = entry.dependencies.clone();
            if let Some(dependencies) = dependencies.as_mut() {
                dependencies.sort();
                dependencies.dedup();
            }
            let mut type_dependency_usages = entry.type_dependency_usages.clone();
            if let Some(usages) = type_dependency_usages.as_mut() {
                usages.sort();
                usages.dedup();
            }
            let mut sequence_dependency_usages = entry.sequence_dependency_usages.clone();
            if let Some(usages) = sequence_dependency_usages.as_mut() {
                usages.sort();
                usages.dedup();
            }
            if catalog
                .insert(
                    entry.identity.clone(),
                    (
                        dependencies,
                        type_dependency_usages,
                        sequence_dependency_usages,
                    ),
                )
                .is_some()
            {
                return Err(
                    "Target object catalog returned duplicate identities; compare again".into(),
                );
            }
        }
        Ok(catalog)
    }

    let reviewed = canonicalize(reviewed)?;
    let current = canonicalize(current)?;
    if reviewed != current {
        return Err(
            "Schema object or dependency catalog changed after review; compare again".into(),
        );
    }
    Ok(())
}

pub fn validate_table_identity_catalog(
    reviewed: &[SchemaObjectIdentity],
    current: &[SchemaObjectIdentity],
) -> Result<(), String> {
    let reviewed = reviewed.iter().cloned().collect::<BTreeSet<_>>();
    let current = current.iter().cloned().collect::<BTreeSet<_>>();
    if reviewed.len() != current.len() || reviewed != current {
        return Err("Target table catalog changed after review; compare again".into());
    }
    Ok(())
}
