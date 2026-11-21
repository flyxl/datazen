use std::collections::BTreeSet;

use super::{
    DataSyncTableMapping, MappingRowLike, ProfileEffect, ProfileEndpointState, ProfilesHost,
    ProfilesState, SyncProfile, PROFILE_MISSING_FIELDS_KEY, PROFILE_SAVED_KEY,
};
use crate::SyncOptions;

pub fn current_profile_tables(
    mapping_results: &[MappingRowLike],
    disabled_tables: &BTreeSet<String>,
    pending: Option<&[DataSyncTableMapping]>,
) -> Vec<DataSyncTableMapping> {
    if mapping_results.is_empty() {
        return pending.unwrap_or_default().to_vec();
    }
    mapping_results
        .iter()
        .filter(|row| !row.source_table.trim().is_empty() && !row.target_table.trim().is_empty())
        .map(|row| DataSyncTableMapping {
            source_table: row.source_table.clone(),
            target_table: row.target_table.clone(),
            enabled: !disabled_tables.contains(&row.source_table) && row.status != "DISABLED",
            source_filter: row.source_filter.clone(),
        })
        .collect()
}

pub fn save_current_profile(
    state: &mut ProfilesState,
    host: &dyn ProfilesHost,
    endpoints: &ProfileEndpointState,
    options: &SyncOptions,
    mapping_results: &[MappingRowLike],
    disabled_tables: &BTreeSet<String>,
    pending: Option<&[DataSyncTableMapping]>,
) -> Vec<ProfileEffect> {
    let name = state.profile_name.trim().to_owned();
    if name.is_empty() || endpoints.source_id.is_empty() || endpoints.target_id.is_empty() {
        return vec![
            ProfileEffect::SetError(host.translate(PROFILE_MISSING_FIELDS_KEY)),
            ProfileEffect::SetErrorOpen(true),
        ];
    }
    let existing = state
        .sync_profiles
        .iter()
        .find(|profile| profile.id == state.selected_profile_id)
        .cloned();
    let now = host.now_iso();
    let profile = SyncProfile {
        version: 1,
        id: existing
            .as_ref()
            .map(|profile| profile.id.clone())
            .unwrap_or_else(|| host.new_id()),
        name: name.clone(),
        source_connection_id: endpoints.source_id.clone(),
        target_connection_id: endpoints.target_id.clone(),
        source_database: optional(&endpoints.source_database),
        target_database: optional(&endpoints.target_database),
        source_schema: optional(&endpoints.source_schema),
        target_schema: optional(&endpoints.target_schema),
        tables: current_profile_tables(mapping_results, disabled_tables, pending),
        options: options.clone(),
        created_at: existing
            .as_ref()
            .map(|profile| profile.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    match host.save_profile(&profile) {
        Ok(()) => {
            state.selected_profile_id = profile.id.clone();
            state.profile_name = profile.name.clone();
            let refreshed = host.get_profiles().unwrap_or_default();
            state.sync_profiles = refreshed.clone();
            vec![
                ProfileEffect::SetProfiles(refreshed),
                ProfileEffect::SetSelectedProfileId(profile.id),
                ProfileEffect::SetProfileName(profile.name),
                ProfileEffect::SetStatus(host.translate(PROFILE_SAVED_KEY)),
            ]
        }
        Err(error) => vec![
            ProfileEffect::SetError(error),
            ProfileEffect::SetErrorOpen(true),
        ],
    }
}

fn optional(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}
