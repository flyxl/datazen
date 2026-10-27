use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use datazen_driver_api::namespace::{
    CaseFolding, CommandTargetRequirements, NamespaceLevelKind, NamespaceShape,
    TargetLevelRequirement,
};
use datazen_modules::application::target::TargetResolver;
use datazen_modules::{
    execution::target::{
        CanonicalTarget, ExecutionTarget, LayerRequirement, NamespaceTarget,
        TargetCanonicalIdRules, TargetCaseFolding, TargetCaseRules, TargetNamespaceLayer,
        TargetNamespacePathSegment, TargetNamespaceShape, TargetRequirements,
    },
    shared::id::ConnectionId,
};

#[derive(Clone)]
pub(crate) struct ConnectionTargetMetadata {
    pub shape: TargetNamespaceShape,
    pub commands: HashMap<String, TargetRequirements>,
}

pub(crate) type TargetMetadataCache = Arc<RwLock<HashMap<ConnectionId, ConnectionTargetMetadata>>>;

pub(crate) fn into_execution_target(canonical: CanonicalTarget) -> ExecutionTarget {
    let CanonicalTarget {
        connection_id,
        namespace,
        object,
    } = canonical;
    ExecutionTarget {
        connection_id,
        namespace: NamespaceTarget::new(
            namespace.database.map(|id| id.as_str().to_owned()),
            namespace.catalog.map(|id| id.as_str().to_owned()),
            namespace.schema.map(|id| id.as_str().to_owned()),
            namespace
                .path
                .into_iter()
                .map(|id| id.as_str().to_owned())
                .collect(),
        ),
        object,
    }
}

pub(crate) fn resolver(cache: TargetMetadataCache) -> TargetResolver {
    TargetResolver::new(Arc::new(move |connection_id, command| {
        let snapshot = cache.read().ok()?;
        let metadata = snapshot.get(connection_id)?;
        Some((
            metadata.shape.clone(),
            metadata.commands.get(command).copied(),
        ))
    }))
}

pub(crate) fn namespace_shape(shape: &NamespaceShape) -> TargetNamespaceShape {
    let mut required = Vec::new();
    let mut optional = Vec::new();
    for level in &shape.levels {
        if !level.exists {
            continue;
        }
        if level.required {
            required.push(namespace_layer(level.kind));
        } else {
            optional.push(namespace_layer(level.kind));
        }
    }
    TargetNamespaceShape {
        required,
        optional,
        path_segments: shape
            .path_segments
            .iter()
            .map(|segment| TargetNamespacePathSegment {
                position: segment.position,
                kind: namespace_layer(segment.kind),
                meaning: segment.meaning.clone(),
            })
            .collect(),
        case_rules: TargetCaseRules {
            unquoted: case_folding(shape.case_rules.unquoted),
            quoted: case_folding(shape.case_rules.quoted),
        },
        canonical_id_rules: TargetCanonicalIdRules {
            case_sensitive: shape.canonical_id_rules.case_sensitive,
            delimiter: shape.canonical_id_rules.delimiter.clone(),
        },
        aliases: shape.aliases.clone(),
    }
}

pub(crate) fn command_requirements(requirements: CommandTargetRequirements) -> TargetRequirements {
    TargetRequirements {
        database: layer_requirement(requirements.namespace.database),
        catalog: layer_requirement(requirements.namespace.catalog),
        schema: layer_requirement(requirements.namespace.schema),
        path: if requirements.namespace.path_forbidden {
            LayerRequirement::Forbidden
        } else {
            LayerRequirement::Optional
        },
        allows_object: requirements.allows_object,
        requires_object: requirements.requires_object,
        allows_session_defaults: requirements.allows_session_defaults,
    }
}

/// Schema metadata commands are intercepted by the host and removed from the
/// driver's discovered command definitions, so all of them need explicit
/// target admission rules here. They do not accept an object target or inherit
/// a session default; catalog listing additionally requires a database.
pub(crate) fn host_metadata_command_requirements() -> Vec<(&'static str, CommandTargetRequirements)>
{
    let namespace_only = CommandTargetRequirements::default();
    let mut catalog = CommandTargetRequirements::default();
    catalog.namespace.database = TargetLevelRequirement::Required;

    [
        ("list_catalog", catalog),
        ("read_relation_columns", namespace_only),
        ("read_relation_schema", namespace_only),
        ("refresh_schema_metadata", namespace_only),
        ("get_structure_capabilities", namespace_only),
        ("plan_table_structure_changes", namespace_only),
    ]
    .into()
}

fn namespace_layer(kind: NamespaceLevelKind) -> TargetNamespaceLayer {
    match kind {
        NamespaceLevelKind::Database => TargetNamespaceLayer::Database,
        NamespaceLevelKind::Catalog => TargetNamespaceLayer::Catalog,
        NamespaceLevelKind::Schema => TargetNamespaceLayer::Schema,
    }
}

fn case_folding(rule: CaseFolding) -> TargetCaseFolding {
    match rule {
        CaseFolding::Lowercases => TargetCaseFolding::Lowercases,
        CaseFolding::Preserved => TargetCaseFolding::Preserved,
        CaseFolding::Unknown => TargetCaseFolding::Unknown,
    }
}

fn layer_requirement(requirement: TargetLevelRequirement) -> LayerRequirement {
    match requirement {
        TargetLevelRequirement::Required => LayerRequirement::Required,
        TargetLevelRequirement::Optional => LayerRequirement::Optional,
        TargetLevelRequirement::Forbidden => LayerRequirement::Forbidden,
    }
}
