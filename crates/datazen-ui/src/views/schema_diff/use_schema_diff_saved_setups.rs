//! Port of `packages/frontend/src/windows/schema-diff/
//! useSchemaDiffSavedSetups.ts` (T411).
//!
//! The wizard's saved setups: the profile list and its save/load/
//! delete flows, the v2 config import/export, and the endpoint-change
//! preservation rule that keeps a pending setup's picks alive while
//! the endpoints it prescribed settle in.
//!
//! ## What maps where
//!
//! - [`parse_object_selections`] is the source's own validator,
//!   branch for branch: the six kinds, the non-empty name, the
//!   optional identity parts (`signature` allows empty, the others do
//!   not), the function/procedure signature rule, the trigger
//!   `targetName` rule, and the identity-key dedup.
//! - `shouldPreserveEndpointChange` is [`should_preserve_endpoint_
//!   change`]: a pending profile/config (or its connection change)
//!   preserves, a settled endpoint match preserves once and consumes
//!   the ref, an unrelated change does not.
//! - `applyImportedConfig` is [`apply_imported_config`]: the v2 gate
//!   (version 2, both connection ids), the object-selection parse,
//!   the target-only/source-only pick mapping, and the state resets
//!   the returned [`ImportPlan`] enumerates for the host.
//! - `handleLoadProfile` is [`profile_picks`] (the initial picks) +
//!   [`ProfileLoad::endpoint_targets`] (the six endpoint setters) +
//!   the profile field adoptions.
//! - The two settle effects are [`profile_applies_when`] (the full
//!   six-field endpoint match) and [`config_applies_when`] (the two
//!   connection ids).
//! - `handleSaveProfile` is [`build_profile`]: the name rule, the
//!   selection re-validation, the `validateEndpoints` + non-empty
//!   selection gate, and the profile assembly (existing id kept, the
//!   `schema-diff-{now}` mint, the enabled-table names splits).
//! - `handleExportConfig` is [`build_config_json`].
//! - The async halves (ensureConnected, listTables, the unified
//!   object load, the profile persistence, the config file write) are
//!   host seams; the dialogs' open flags and texts are plain state.
//!
//! Known deltas: `JSON.parse` becomes `serde_json::from_str` into a
//! permissive `Value` first (the v2 gate and the object parse read
//! the same fields the React code does); `new Date().toISOString()`
//! timestamps are supplied by the host.

use serde_json::Value;

use crate::commands::schema_diff::wire::SchemaDiffProfileVm;
use crate::views::schema_diff::schema_diff_object_identity::{
    SchemaDiffObjectIdentityVm, SchemaDiffObjectKindVm, schema_diff_object_identity_key,
};
use crate::views::schema_diff::schema_diff_table_names::{
    SchemaDiffTablePickVm, TablePickOriginVm, enabled_source_table_names,
    enabled_target_only_table_names,
};

/// The kinds an object selection may carry (the source's set).
const SCHEMA_DIFF_OBJECT_KINDS: [SchemaDiffObjectKindVm; 6] = [
    SchemaDiffObjectKindVm::View,
    SchemaDiffObjectKindVm::Type,
    SchemaDiffObjectKindVm::Sequence,
    SchemaDiffObjectKindVm::Function,
    SchemaDiffObjectKindVm::Procedure,
    SchemaDiffObjectKindVm::Trigger,
];

/// `parseObjectSelections(value, invalidConfigMessage)`: validate a
/// config's object array. Every failure is the caller's message.
pub(crate) fn parse_object_selections(
    value: Option<&Value>,
    invalid_config_message: &str,
) -> Result<Vec<SchemaDiffObjectIdentityVm>, String> {
    let Some(value) = value else { return Ok(Vec::new()) };
    let Some(candidates) = value.as_array() else {
        return Err(invalid_config_message.to_owned());
    };
    let mut identities: Vec<SchemaDiffObjectIdentityVm> = Vec::new();
    for candidate in candidates {
        let Some(object) = candidate.as_object() else {
            return Err(invalid_config_message.to_owned());
        };
        let kind = object.get("kind").and_then(Value::as_str);
        let name = object.get("name").and_then(Value::as_str);
        let (kind, name) = match (kind, name) {
            (Some(kind), Some(name))
                if SchemaDiffObjectKindVm::from_wire_str(kind).is_some_and(|k| {
                    SCHEMA_DIFF_OBJECT_KINDS.contains(&k)
                }) && !name.trim().is_empty() =>
            {
                (SchemaDiffObjectKindVm::from_wire_str(kind).expect("checked"), name)
            }
            _ => return Err(invalid_config_message.to_owned()),
        };
        // `optionalIdentityPart(key, allowEmpty)`: null/absent →
        // None; a non-string or an empty (when disallowed) string →
        // the error; `signature` allows the empty string.
        let part = |key: &str, allow_empty: bool| -> Result<Option<String>, String> {
            match object.get(key) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::String(part)) => {
                    if !allow_empty && part.trim().is_empty() {
                        Err(invalid_config_message.to_owned())
                    } else {
                        Ok(Some(part.clone()))
                    }
                }
                Some(_) => Err(invalid_config_message.to_owned()),
            }
        };
        let identity = SchemaDiffObjectIdentityVm {
            kind,
            schema: part("schema", false)?,
            name: name.to_owned(),
            signature: part("signature", true)?,
            target_schema: part("targetSchema", false)?,
            target_name: part("targetName", false)?,
        };
        // A function/procedure needs a signature (empty allowed once
        // present — the length>0 && !trim() arm refuses a blank).
        if matches!(identity.kind, SchemaDiffObjectKindVm::Function | SchemaDiffObjectKindVm::Procedure)
            && (identity.signature.is_none()
                || identity
                    .signature
                    .as_deref()
                    .is_some_and(|s| !s.is_empty() && s.trim().is_empty()))
        {
            return Err(invalid_config_message.to_owned());
        }
        if identity.kind == SchemaDiffObjectKindVm::Trigger && identity.target_name.is_none() {
            return Err(invalid_config_message.to_owned());
        }
        identities.push(identity);
    }
    // The dedup: no two entries share an identity key.
    let mut keys: Vec<String> = identities.iter().map(schema_diff_object_identity_key).collect();
    let total = keys.len();
    keys.sort_unstable();
    keys.dedup();
    if keys.len() != total {
        return Err(invalid_config_message.to_owned());
    }
    Ok(identities)
}

/// The endpoint identity a config load recorded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SchemaDiffConfigEndpointIdentity {
    pub(crate) source_connection_id: String,
    pub(crate) target_connection_id: String,
    pub(crate) source_database: String,
    pub(crate) target_database: String,
    pub(crate) source_schema: String,
    pub(crate) target_schema: String,
}

/// The endpoints the preservation rule reads (the six fields).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EndpointSnapshot {
    pub(crate) source_id: String,
    pub(crate) target_id: String,
    pub(crate) source_database: String,
    pub(crate) target_database: String,
    pub(crate) source_schema: String,
    pub(crate) target_schema: String,
}

/// `shouldPreserveEndpointChange`: whether an endpoint change must
/// keep the current picks (a pending setup is settling). `profile_ref`
/// / `config_ref` are the two "did the endpoint match once" refs —
/// consumed (set `None`) exactly when they preserve.
pub(crate) fn should_preserve_endpoint_change(
    current: &EndpointSnapshot,
    pending_profile: Option<&SchemaDiffProfileVm>,
    pending_config: Option<&SchemaDiffConfigEndpointIdentity>,
    profile_ref: &mut Option<SchemaDiffProfileVm>,
    config_ref: &mut Option<SchemaDiffConfigEndpointIdentity>,
) -> bool {
    let profile_matches = profile_ref.as_ref().is_some_and(|profile| {
        current.source_id == profile.source_connection_id
            && current.target_id == profile.target_connection_id
            && current.source_database == profile.source_database
            && current.target_database == profile.target_database
            && current.source_schema == profile.source_schema.clone().unwrap_or_default()
            && current.target_schema == profile.target_schema.clone().unwrap_or_default()
    });
    let config_matches = config_ref.as_ref().is_some_and(|config| {
        current.source_id == config.source_connection_id
            && current.target_id == config.target_connection_id
            && current.source_database == config.source_database
            && current.target_database == config.target_database
            && current.source_schema == config.source_schema
            && current.target_schema == config.target_schema
    });
    let pending_config_connection_change = pending_config.is_some_and(|config| {
        current.source_id != config.source_connection_id
            || current.target_id != config.target_connection_id
    });
    if pending_profile.is_some() || pending_config_connection_change || profile_matches || config_matches {
        if profile_matches {
            *profile_ref = None;
        }
        if config_matches {
            *config_ref = None;
        }
        return true;
    }
    if config_ref.is_some() {
        *config_ref = None;
    }
    false
}

/// The import plan `applyImportedConfig` hands the host.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ImportPlan {
    pub(crate) source_connection_id: String,
    pub(crate) target_connection_id: String,
    /// The picks built from `cfg.tables` + `cfg.targetOnlyTables`.
    pub(crate) table_picks: Vec<SchemaDiffTablePickVm>,
    pub(crate) allow_destructive: bool,
    /// `cfg.includeIndexes ?? true`.
    pub(crate) include_indexes: bool,
    pub(crate) require_rollback: bool,
    /// The endpoint identity the settle gate waits for.
    pub(crate) config_identity: SchemaDiffConfigEndpointIdentity,
    /// The parsed object selections (the pending config's payload).
    pub(crate) source_objects: Vec<SchemaDiffObjectIdentityVm>,
    pub(crate) target_objects: Vec<SchemaDiffObjectIdentityVm>,
}

/// `applyImportedConfig(text)`: the v2 gate, the object parse, the
/// pick mapping and the state resets (the plan the host applies;
/// `Err` is the import-config error message).
pub(crate) fn apply_imported_config(
    text: &str,
    endpoints: &EndpointSnapshot,
    invalid_config_message: &str,
) -> Result<ImportPlan, String> {
    let cfg: Value = serde_json::from_str(text).map_err(|_| invalid_config_message.to_owned())?;
    let source_connection_id =
        cfg.get("sourceConnectionId").and_then(Value::as_str).unwrap_or_default();
    let target_connection_id =
        cfg.get("targetConnectionId").and_then(Value::as_str).unwrap_or_default();
    if cfg.get("version").and_then(Value::as_i64) != Some(2)
        || source_connection_id.is_empty()
        || target_connection_id.is_empty()
    {
        return Err(invalid_config_message.to_owned());
    }
    let source_objects = parse_object_selections(cfg.get("sourceObjects"), invalid_config_message)?;
    let target_objects = parse_object_selections(cfg.get("targetObjects"), invalid_config_message)?;
    let target_only: Vec<String> = cfg
        .get("targetOnlyTables")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter().filter_map(Value::as_str).map(str::to_owned).collect()
        })
        .unwrap_or_default();
    let tables: Vec<String> = cfg
        .get("tables")
        .and_then(Value::as_array)
        .map(|rows| rows.iter().filter_map(Value::as_str).map(str::to_owned).collect())
        .unwrap_or_default();
    let picks = tables
        .iter()
        .map(|name| {
            if target_only.contains(name) {
                SchemaDiffTablePickVm {
                    name: name.clone(),
                    enabled: true,
                    origin: TablePickOriginVm::TargetOnly,
                    source_name: None,
                    target_name: Some(name.clone()),
                }
            } else {
                SchemaDiffTablePickVm {
                    name: name.clone(),
                    enabled: true,
                    origin: TablePickOriginVm::SourceOnly,
                    source_name: Some(name.clone()),
                    target_name: None,
                }
            }
        })
        .collect();
    Ok(ImportPlan {
        source_connection_id: source_connection_id.to_owned(),
        target_connection_id: target_connection_id.to_owned(),
        table_picks: picks,
        allow_destructive: cfg.get("allowDestructive").and_then(Value::as_bool).unwrap_or(false),
        include_indexes: cfg.get("includeIndexes").and_then(Value::as_bool).unwrap_or(true),
        require_rollback: cfg.get("requireRollback").and_then(Value::as_bool).unwrap_or(false),
        config_identity: SchemaDiffConfigEndpointIdentity {
            source_connection_id: source_connection_id.to_owned(),
            target_connection_id: target_connection_id.to_owned(),
            source_database: endpoints.source_database.clone(),
            target_database: endpoints.target_database.clone(),
            source_schema: endpoints.source_schema.clone(),
            target_schema: endpoints.target_schema.clone(),
        },
        source_objects,
        target_objects,
    })
}

/// The profile-load plan `handleLoadProfile` hands the host.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProfileLoad {
    /// The six endpoint fields to set.
    pub(crate) source_connection_id: String,
    pub(crate) target_connection_id: String,
    pub(crate) source_database: String,
    pub(crate) target_database: String,
    /// `profile.sourceSchema ?? ''`.
    pub(crate) source_schema: String,
    pub(crate) target_schema: String,
    pub(crate) table_picks: Vec<SchemaDiffTablePickVm>,
    pub(crate) allow_destructive: bool,
    pub(crate) include_indexes: bool,
    pub(crate) require_rollback: bool,
    /// `profile.typeOverrides ?? []`.
    pub(crate) type_overrides_len: usize,
    pub(crate) profile: SchemaDiffProfileVm,
}

/// `handleLoadProfile(profile)`: the plan (the host applies the
/// endpoint setters, the picks, the flags, and parks the profile as
/// the pending load).
pub(crate) fn profile_load(profile: &SchemaDiffProfileVm) -> ProfileLoad {
    let source_only = profile.tables.iter().map(|name| SchemaDiffTablePickVm {
        name: name.clone(),
        enabled: true,
        origin: TablePickOriginVm::SourceOnly,
        source_name: Some(name.clone()),
        target_name: None,
    });
    ProfileLoad {
        source_connection_id: profile.source_connection_id.clone(),
        target_connection_id: profile.target_connection_id.clone(),
        source_database: profile.source_database.clone(),
        target_database: profile.target_database.clone(),
        source_schema: profile.source_schema.clone().unwrap_or_default(),
        target_schema: profile.target_schema.clone().unwrap_or_default(),
        table_picks: source_only.chain(target_only_fixup(profile)).collect(),
        allow_destructive: profile.allow_destructive,
        include_indexes: profile.include_indexes,
        require_rollback: profile.require_rollback,
        type_overrides_len: profile.type_overrides.as_ref().map_or(0, Vec::len),
        profile: profile.clone(),
    }
}

/// The target-only picks with their own names (kept apart so the
/// closure above stays readable).
fn target_only_fixup(profile: &SchemaDiffProfileVm) -> Vec<SchemaDiffTablePickVm> {
    profile
        .target_only_tables
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|name| SchemaDiffTablePickVm {
            name: name.clone(),
            enabled: true,
            origin: TablePickOriginVm::TargetOnly,
            source_name: None,
            target_name: Some(name),
        })
        .collect()
}

/// The settle effect for a pending profile: it applies once the
/// endpoints match all six fields (`sourceSchema ?? ''`).
pub(crate) fn profile_applies_when(
    endpoints: &EndpointSnapshot,
    profile: &SchemaDiffProfileVm,
) -> bool {
    endpoints.source_id == profile.source_connection_id
        && endpoints.target_id == profile.target_connection_id
        && endpoints.source_database == profile.source_database
        && endpoints.target_database == profile.target_database
        && endpoints.source_schema == profile.source_schema.clone().unwrap_or_default()
        && endpoints.target_schema == profile.target_schema.clone().unwrap_or_default()
}

/// The settle effect for a pending config: the two connection ids.
pub(crate) fn config_applies_when(
    endpoints: &EndpointSnapshot,
    config: &SchemaDiffConfigEndpointIdentity,
) -> bool {
    endpoints.source_id == config.source_connection_id
        && endpoints.target_id == config.target_connection_id
}

/// What `handleSaveProfile` needs to decide and build.
pub(crate) struct SaveProfileInput<'a> {
    pub(crate) profile_name: &'a str,
    pub(crate) profiles: &'a [SchemaDiffProfileVm],
    pub(crate) selected_profile_id: &'a str,
    pub(crate) endpoints_valid: bool,
    pub(crate) selected_tables: &'a [String],
    pub(crate) table_picks: &'a [SchemaDiffTablePickVm],
    pub(crate) selected_source_objects: &'a [SchemaDiffObjectIdentityVm],
    pub(crate) selected_target_objects: &'a [SchemaDiffObjectIdentityVm],
    pub(crate) allow_destructive: bool,
    pub(crate) include_indexes: bool,
    pub(crate) require_rollback: bool,
    pub(crate) type_overrides: Vec<crate::commands::schema_diff::wire::ColumnTypeOverrideVm>,
    /// `new Date().toISOString()` — the host's clock.
    pub(crate) now: &'a str,
    /// The id mint seed (`Date.now()`).
    pub(crate) now_ms: i64,
}

/// `handleSaveProfile`'s refusals (the i18n keys React reports).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SaveProfileRefusal {
    ProfileNameRequired,
    ObjectIdentityIncomplete,
    ProfileSetupRequired,
}

/// `handleSaveProfile`: validate and assemble the profile. `Err` is
/// the refusal the dialog reports; `Ok` is the profile the host
/// persists (existing id kept, the `schema-diff-{now_ms}` mint).
pub(crate) fn build_profile(
    input: &SaveProfileInput<'_>,
) -> Result<SchemaDiffProfileVm, SaveProfileRefusal> {
    let name = input.profile_name.trim();
    if name.is_empty() {
        return Err(SaveProfileRefusal::ProfileNameRequired);
    }
    // The selections re-validate through the same parser the config
    // import uses (the React code serializes the typed objects back
    // through `parseObjectSelections`).
    let serialize = |objects: &[SchemaDiffObjectIdentityVm]| -> Value {
        serde_json::to_value(objects).unwrap_or_default()
    };
    let source_objects = parse_object_selections(
        Some(&serialize(input.selected_source_objects)),
        "incomplete",
    )
    .map_err(|_| SaveProfileRefusal::ObjectIdentityIncomplete)?;
    let target_objects = parse_object_selections(
        Some(&serialize(input.selected_target_objects)),
        "incomplete",
    )
    .map_err(|_| SaveProfileRefusal::ObjectIdentityIncomplete)?;
    if !input.endpoints_valid
        || (input.selected_tables.is_empty()
            && source_objects.is_empty()
            && target_objects.is_empty())
    {
        return Err(SaveProfileRefusal::ProfileSetupRequired);
    }
    let existing = input.profiles.iter().find(|item| item.id == input.selected_profile_id);
    Ok(SchemaDiffProfileVm {
        version: 1,
        id: existing
            .map(|item| item.id.clone())
            .unwrap_or_else(|| format!("schema-diff-{}", input.now_ms)),
        name: name.to_owned(),
        source_connection_id: String::new(), // set by the caller's endpoints snapshot
        target_connection_id: String::new(),
        source_database: String::new(),
        target_database: String::new(),
        source_schema: None,
        target_schema: None,
        tables: enabled_source_table_names(input.table_picks),
        target_only_tables: Some(enabled_target_only_table_names(input.table_picks)),
        source_objects: Some(source_objects),
        target_objects: Some(target_objects),
        allow_destructive: input.allow_destructive,
        include_indexes: input.include_indexes,
        require_rollback: input.require_rollback,
        type_overrides: Some(input.type_overrides.clone()),
        created_at: existing.map(|item| item.created_at.clone()).unwrap_or_else(|| input.now.to_owned()),
        updated_at: input.now.to_owned(),
    })
}

/// `handleExportConfig`'s assembly (the v2 config the host writes).
pub(crate) fn build_config_json(
    selected_tables: &[String],
    table_picks: &[SchemaDiffTablePickVm],
    selected_source_objects: &[SchemaDiffObjectIdentityVm],
    selected_target_objects: &[SchemaDiffObjectIdentityVm],
    allow_destructive: bool,
    include_indexes: bool,
    require_rollback: bool,
) -> Result<Value, SaveProfileRefusal> {
    let serialize = |objects: &[SchemaDiffObjectIdentityVm]| -> Value {
        serde_json::to_value(objects).unwrap_or_default()
    };
    let source_objects = parse_object_selections(Some(&serialize(selected_source_objects)), "incomplete")
        .map_err(|_| SaveProfileRefusal::ObjectIdentityIncomplete)?;
    let target_objects = parse_object_selections(Some(&serialize(selected_target_objects)), "incomplete")
        .map_err(|_| SaveProfileRefusal::ObjectIdentityIncomplete)?;
    Ok(serde_json::json!({
        "version": 2,
        "tables": selected_tables,
        "targetOnlyTables": enabled_target_only_table_names(table_picks),
        "sourceObjects": source_objects,
        "targetObjects": target_objects,
        "allowDestructive": allow_destructive,
        "includeIndexes": include_indexes,
        "requireRollback": require_rollback,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn identity_value(kind: &str, name: &str) -> Value {
        json!({ "kind": kind, "name": name })
    }

    #[test]
    fn object_selections_validate_the_full_shape() {
        // A view with a schema.
        let parsed = parse_object_selections(
            Some(&json!([identity_value("view", "v1")])),
            "invalid",
        )
        .expect("parses");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "v1");
        // Absent is the empty selection.
        assert!(parse_object_selections(None, "invalid").unwrap().is_empty());
        // Not an array, not an object, a bad kind, a blank name.
        assert!(parse_object_selections(Some(&json!("x")), "invalid").is_err());
        assert!(parse_object_selections(Some(&json!([1])), "invalid").is_err());
        assert!(
            parse_object_selections(Some(&json!([identity_value("table", "t")])), "invalid").is_err()
        );
        assert!(
            parse_object_selections(Some(&json!([identity_value("view", "  ")])), "invalid").is_err()
        );
        // A non-string or blank optional part (signature may be blank).
        assert!(parse_object_selections(
            Some(&json!([{ "kind": "view", "name": "v", "schema": 3 }])),
            "invalid"
        )
        .is_err());
        assert!(parse_object_selections(
            Some(&json!([{ "kind": "view", "name": "v", "schema": " " }])),
            "invalid"
        )
        .is_err());
        assert!(parse_object_selections(
            Some(&json!([{ "kind": "function", "name": "f", "signature": "" }])),
            "invalid"
        )
        .is_ok());
    }

    #[test]
    fn functions_need_a_signature_and_triggers_need_a_target() {
        // A function without a signature is refused.
        assert!(parse_object_selections(
            Some(&json!([identity_value("function", "f1")])),
            "invalid"
        )
        .is_err());
        // A procedure with a blank signature is refused (the
        // length>0 && !trim() arm).
        assert!(parse_object_selections(
            Some(&json!([{ "kind": "procedure", "name": "p", "signature": "  " }])),
            "invalid"
        )
        .is_err());
        // A trigger without targetName is refused.
        assert!(parse_object_selections(
            Some(&json!([identity_value("trigger", "trg")])),
            "invalid"
        )
        .is_err());
        let trigger = parse_object_selections(
            Some(&json!([{ "kind": "trigger", "name": "trg", "targetName": "orders" }])),
            "invalid",
        )
        .expect("parses");
        assert_eq!(trigger[0].target_name.as_deref(), Some("orders"));
    }

    #[test]
    fn duplicate_identities_are_refused() {
        // The same view twice: one identity key.
        assert!(parse_object_selections(
            Some(&json!([identity_value("view", "v"), identity_value("view", "v")])),
            "invalid"
        )
        .is_err());
        // Different schemas: two keys.
        assert!(parse_object_selections(
            Some(&json!([
                { "kind": "view", "name": "v", "schema": "a" },
                { "kind": "view", "name": "v", "schema": "b" }
            ])),
            "invalid"
        )
        .is_ok());
    }

    fn profile() -> SchemaDiffProfileVm {
        SchemaDiffProfileVm {
            version: 1,
            id: "p1".to_owned(),
            name: "My setup".to_owned(),
            source_connection_id: "pg".to_owned(),
            target_connection_id: "my".to_owned(),
            source_database: "app".to_owned(),
            target_database: "shop".to_owned(),
            source_schema: Some("public".to_owned()),
            target_schema: None,
            tables: vec!["users".to_owned()],
            target_only_tables: Some(vec!["audit".to_owned()]),
            source_objects: None,
            target_objects: None,
            allow_destructive: true,
            include_indexes: false,
            require_rollback: true,
            type_overrides: None,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: "2026-01-01T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn the_preservation_rule_keeps_a_settling_setup_once() {
        let profile = profile();
        let mut profile_ref: Option<SchemaDiffProfileVm> = None;
        let mut config_ref: Option<SchemaDiffConfigEndpointIdentity> = None;
        let unrelated = EndpointSnapshot {
            source_id: "a".to_owned(),
            ..Default::default()
        };
        // An unrelated change with nothing pending: not preserved.
        assert!(!should_preserve_endpoint_change(
            &unrelated,
            None,
            None,
            &mut profile_ref,
            &mut config_ref
        ));
        // A pending profile preserves while it settles…
        let settling = EndpointSnapshot {
            source_id: "pg".to_owned(),
            target_id: "my".to_owned(),
            source_database: "app".to_owned(),
            target_database: "shop".to_owned(),
            source_schema: "public".to_owned(),
            target_schema: String::new(),
        };
        assert!(should_preserve_endpoint_change(
            &settling,
            Some(&profile),
            None,
            &mut profile_ref,
            &mut config_ref
        ));
        // …and the endpoint match consumes the ref: the next
        // unrelated change does not preserve.
        profile_ref = Some(profile.clone());
        assert!(should_preserve_endpoint_change(
            &settling,
            None,
            None,
            &mut profile_ref,
            &mut config_ref
        ));
        assert!(profile_ref.is_none(), "the match consumed the ref");
        assert!(!should_preserve_endpoint_change(
            &unrelated,
            None,
            None,
            &mut profile_ref,
            &mut config_ref
        ));
    }

    #[test]
    fn a_pending_config_connection_change_preserves() {
        let config = SchemaDiffConfigEndpointIdentity {
            source_connection_id: "pg".to_owned(),
            target_connection_id: "my".to_owned(),
            source_database: String::new(),
            target_database: String::new(),
            source_schema: String::new(),
            target_schema: String::new(),
        };
        let mut profile_ref = None;
        let mut config_ref: Option<SchemaDiffConfigEndpointIdentity> = None;
        let moving = EndpointSnapshot {
            source_id: "old".to_owned(),
            target_id: "my".to_owned(),
            ..Default::default()
        };
        assert!(should_preserve_endpoint_change(
            &moving,
            None,
            Some(&config),
            &mut profile_ref,
            &mut config_ref
        ));
    }

    #[test]
    fn the_import_gate_demands_version_two_and_both_connections() {
        let endpoints = EndpointSnapshot::default();
        assert!(apply_imported_config("not json", &endpoints, "invalid").is_err());
        assert!(apply_imported_config(
            json!({ "version": 1, "sourceConnectionId": "a", "targetConnectionId": "b" }).to_string().as_str(),
            &endpoints,
            "invalid"
        )
        .is_err());
        assert!(apply_imported_config(
            json!({ "version": 2, "sourceConnectionId": "", "targetConnectionId": "b" }).to_string().as_str(),
            &endpoints,
            "invalid"
        )
        .is_err());
        let plan = apply_imported_config(
            json!({
                "version": 2,
                "sourceConnectionId": "pg",
                "targetConnectionId": "my",
                "tables": ["users", "audit"],
                "targetOnlyTables": ["audit"],
                "allowDestructive": true,
                "requireRollback": true
            })
            .to_string()
            .as_str(),
            &endpoints,
            "invalid",
        )
        .expect("imports");
        assert_eq!(plan.source_connection_id, "pg");
        assert_eq!(plan.table_picks.len(), 2);
        assert_eq!(plan.table_picks[0].origin, TablePickOriginVm::SourceOnly);
        assert_eq!(plan.table_picks[1].origin, TablePickOriginVm::TargetOnly);
        assert!(plan.allow_destructive);
        assert!(plan.include_indexes, "includeIndexes defaults true");
        assert!(plan.require_rollback);
        assert_eq!(plan.config_identity.source_connection_id, "pg");
    }

    #[test]
    fn the_profile_load_plan_maps_the_tables_and_the_endpoint_fields() {
        let plan = profile_load(&profile());
        assert_eq!(plan.source_connection_id, "pg");
        assert_eq!(plan.target_schema, "", "the absent schema maps to empty");
        assert_eq!(plan.table_picks.len(), 2);
        assert_eq!(plan.table_picks[0].name, "users");
        assert_eq!(plan.table_picks[0].origin, TablePickOriginVm::SourceOnly);
        assert_eq!(plan.table_picks[1].name, "audit");
        assert_eq!(plan.table_picks[1].target_name.as_deref(), Some("audit"));
        assert!(plan.allow_destructive);
        assert!(!plan.include_indexes);
        assert!(plan.require_rollback);
        assert_eq!(plan.type_overrides_len, 0, "typeOverrides ?? []");
    }

    #[test]
    fn the_settle_gates_match_their_fields() {
        let profile = profile();
        let matched = EndpointSnapshot {
            source_id: "pg".to_owned(),
            target_id: "my".to_owned(),
            source_database: "app".to_owned(),
            target_database: "shop".to_owned(),
            source_schema: "public".to_owned(),
            target_schema: String::new(),
        };
        assert!(profile_applies_when(&matched, &profile));
        // One field off: not yet.
        let mut drifted = matched.clone();
        drifted.target_database = "other".to_owned();
        assert!(!profile_applies_when(&drifted, &profile));
        // The config gate only needs the two ids.
        let config = SchemaDiffConfigEndpointIdentity {
            source_connection_id: "pg".to_owned(),
            target_connection_id: "my".to_owned(),
            ..Default::default()
        };
        assert!(config_applies_when(&matched, &config));
        let mut wrong_target = matched.clone();
        wrong_target.target_id = "pg".to_owned();
        assert!(!config_applies_when(&wrong_target, &config));
    }

    fn save_input<'a>(name: &'a str, tables: &'a [String]) -> SaveProfileInput<'a> {
        SaveProfileInput {
            profile_name: name,
            profiles: &[],
            selected_profile_id: "",
            endpoints_valid: true,
            selected_tables: tables,
            table_picks: &[],
            selected_source_objects: &[],
            selected_target_objects: &[],
            allow_destructive: false,
            include_indexes: true,
            require_rollback: false,
            type_overrides: Vec::new(),
            now: "2026-02-02T00:00:00Z",
            now_ms: 1_700_000_000_000,
        }
    }

    #[test]
    fn save_refuses_a_blank_name_then_an_empty_setup() {
        let no_tables: Vec<String> = Vec::new();
        assert_eq!(
            build_profile(&save_input("   ", &no_tables)).unwrap_err(),
            SaveProfileRefusal::ProfileNameRequired
        );
        // A name but nothing selected and no objects.
        assert_eq!(
            build_profile(&save_input("My setup", &no_tables)).unwrap_err(),
            SaveProfileRefusal::ProfileSetupRequired
        );
        // Something selected: the profile builds, with the minted id.
        let tables = vec!["users".to_owned()];
        let built = build_profile(&save_input("My setup", &tables)).expect("builds");
        assert_eq!(built.id, "schema-diff-1700000000000");
        assert_eq!(built.name, "My setup");
        assert_eq!(built.version, 1);
        assert_eq!(built.updated_at, "2026-02-02T00:00:00Z");
    }

    #[test]
    fn save_keeps_the_existing_id_and_creation_time() {
        let existing = profile();
        let tables = vec!["users".to_owned()];
        let input = SaveProfileInput {
            profiles: std::slice::from_ref(&existing),
            selected_profile_id: "p1",
            ..save_input("Renamed", &tables)
        };
        let built = build_profile(&input).expect("builds");
        assert_eq!(built.id, "p1");
        assert_eq!(built.created_at, existing.created_at);
        assert_eq!(built.updated_at, "2026-02-02T00:00:00Z", "the update stamps now");
    }

    #[test]
    fn the_export_config_is_the_v2_shape() {
        let config = build_config_json(
            &["users".to_owned()],
            &[],
            &[],
            &[],
            true,
            false,
            true,
        )
        .expect("builds");
        assert_eq!(config["version"], json!(2));
        assert_eq!(config["tables"], json!(["users"]));
        assert_eq!(config["allowDestructive"], json!(true));
        assert_eq!(config["includeIndexes"], json!(false));
        assert_eq!(config["requireRollback"], json!(true));
    }
}
