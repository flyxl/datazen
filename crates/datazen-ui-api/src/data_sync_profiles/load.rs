use super::{
    ProfileEffect, ProfileEndpointState, ProfilesHost, ProfilesState, PROFILE_DELETED_KEY,
    PROFILE_LOADED_KEY, PROFILE_MISSING_CONNECTION_KEY,
};

pub fn load_sync_profiles(
    state: &mut ProfilesState,
    host: &dyn ProfilesHost,
) -> Vec<ProfileEffect> {
    let profiles = host.get_profiles().unwrap_or_default();
    state.sync_profiles = profiles.clone();
    vec![ProfileEffect::SetProfiles(profiles)]
}

pub fn load_selected_profile(
    state: &mut ProfilesState,
    host: &dyn ProfilesHost,
    endpoints: &ProfileEndpointState,
) -> Vec<ProfileEffect> {
    let profile = match state
        .sync_profiles
        .iter()
        .find(|candidate| candidate.id == state.selected_profile_id)
        .cloned()
    {
        Some(profile) => profile,
        None => return Vec::new(),
    };
    let source_exists = endpoints
        .connections
        .iter()
        .any(|connection| connection.id == profile.source_connection_id);
    let target_exists = endpoints
        .connections
        .iter()
        .any(|connection| connection.id == profile.target_connection_id);
    if !source_exists || !target_exists {
        return vec![
            ProfileEffect::SetError(host.translate(PROFILE_MISSING_CONNECTION_KEY)),
            ProfileEffect::SetErrorOpen(true),
        ];
    }
    state.profile_name = profile.name.clone();
    vec![
        ProfileEffect::SetPendingMappings(Some(profile.tables.clone())),
        ProfileEffect::SetProfileName(profile.name),
        ProfileEffect::SetSourceId(profile.source_connection_id),
        ProfileEffect::SetTargetId(profile.target_connection_id),
        ProfileEffect::SetSourceDatabase(profile.source_database.unwrap_or_default()),
        ProfileEffect::SetTargetDatabase(profile.target_database.unwrap_or_default()),
        ProfileEffect::SetSourceSchema(profile.source_schema.unwrap_or_default()),
        ProfileEffect::SetTargetSchema(profile.target_schema.unwrap_or_default()),
        ProfileEffect::SetSyncOptions(profile.options),
        ProfileEffect::ResetCompareState,
        ProfileEffect::ClearSelectedRows,
        ProfileEffect::ClearTableSelections,
        ProfileEffect::SetStep("endpoints".to_owned()),
        ProfileEffect::SetStatus(host.translate(PROFILE_LOADED_KEY)),
    ]
}

pub fn delete_selected_profile(
    state: &mut ProfilesState,
    host: &dyn ProfilesHost,
) -> Vec<ProfileEffect> {
    if state.selected_profile_id.is_empty() {
        return Vec::new();
    }
    match host.delete_profile(&state.selected_profile_id) {
        Ok(()) => {
            state.selected_profile_id.clear();
            state.profile_name.clear();
            let profiles = host.get_profiles().unwrap_or_default();
            state.sync_profiles = profiles.clone();
            vec![
                ProfileEffect::SetProfiles(profiles),
                ProfileEffect::SetSelectedProfileId(String::new()),
                ProfileEffect::SetProfileName(String::new()),
                ProfileEffect::SetStatus(host.translate(PROFILE_DELETED_KEY)),
            ]
        }
        Err(error) => vec![
            ProfileEffect::SetError(error),
            ProfileEffect::SetErrorOpen(true),
        ],
    }
}
