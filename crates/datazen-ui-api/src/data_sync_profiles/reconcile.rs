use std::collections::BTreeSet;

use super::{
    MigrationRunRecordLike, ProfileEffect, ProfilesHost, ProfilesState, SyncProfile,
    RECONCILE_BUSY_KEY, RECONCILE_MISSING_CONNECTION_KEY, RECONCILE_NEEDS_SCOPE_KEY,
    RECONCILE_PROFILE_CHANGED_KEY, RECONCILE_PROFILE_READY_KEY, RECONCILE_SCOPE_UNAVAILABLE_KEY,
};

pub struct ReconcileInput {
    pub write_in_flight: bool,
}

pub fn parse_timestamp(value: &str) -> Option<i64> {
    let year = value.get(0..4)?.parse::<i64>().ok()?;
    let month = value.get(5..7)?.parse::<i64>().ok()?;
    let day = value.get(8..10)?.parse::<i64>().ok()?;
    let hour = value.get(11..13)?.parse::<i64>().ok()?;
    let minute = value.get(14..16)?.parse::<i64>().ok()?;
    let second = value.get(17..19)?.parse::<i64>().ok()?;
    if value.as_bytes().get(4) != Some(&b'-')
        || value.as_bytes().get(7) != Some(&b'-')
        || value.as_bytes().get(10) != Some(&b'T')
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
    {
        return None;
    }
    let millis = match value.as_bytes().get(19) {
        Some(b'.') => value.get(20..23)?.parse::<i64>().ok()?,
        _ => 0,
    };
    let days = days_from_civil(year, month, day);
    Some((((days * 24 + hour) * 60 + minute) * 60 + second) * 1000 + millis)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = if shifted_year >= 0 {
        shifted_year
    } else {
        shifted_year - 399
    } / 400;
    let year_of_era = shifted_year - era * 400;
    let month_prime = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn timestamps_equal(left: &str, right: &str) -> bool {
    match (parse_timestamp(left), parse_timestamp(right)) {
        (Some(a), Some(b)) => a == b,
        _ => left == right,
    }
}

fn matches_endpoint(run_id: Option<&String>, profile_id: &str) -> bool {
    run_id.map(|id| id == profile_id).unwrap_or(true)
}

pub fn reconcile_unknown_run(
    state: &mut ProfilesState,
    host: &dyn ProfilesHost,
    run: &MigrationRunRecordLike,
    input: ReconcileInput,
) -> (bool, Vec<ProfileEffect>) {
    if run.operation != "dataSync" || run.outcome != "unknown" {
        return (false, Vec::new());
    }
    if input.write_in_flight {
        return (
            false,
            vec![
                ProfileEffect::SetError(host.translate(RECONCILE_BUSY_KEY)),
                ProfileEffect::SetErrorOpen(true),
            ],
        );
    }

    let profiles = host.get_profiles().unwrap_or_default();
    let connections = host.get_connections().unwrap_or_default();
    let mut effects = vec![
        ProfileEffect::SetProfiles(profiles.clone()),
        ProfileEffect::SetConnections(connections.clone()),
    ];

    let saved = run
        .profile_id
        .as_ref()
        .and_then(|id| profiles.iter().find(|profile| &profile.id == id).cloned());
    let same_revision = saved
        .as_ref()
        .and_then(|profile| {
            run.profile_revision
                .as_ref()
                .map(|revision| timestamps_equal(&profile.updated_at, revision))
        })
        .unwrap_or(false);
    let reusable: Option<SyncProfile> = if same_revision {
        saved.as_ref().and_then(|profile| {
            if matches_endpoint(
                run.source_connection_id.as_ref(),
                &profile.source_connection_id,
            ) && matches_endpoint(
                run.target_connection_id.as_ref(),
                &profile.target_connection_id,
            ) {
                Some(profile.clone())
            } else {
                None
            }
        })
    } else {
        None
    };

    let source_id = run
        .source_connection_id
        .clone()
        .or_else(|| {
            reusable
                .as_ref()
                .map(|profile| profile.source_connection_id.clone())
        })
        .unwrap_or_default();
    let target_id = run
        .target_connection_id
        .clone()
        .or_else(|| {
            reusable
                .as_ref()
                .map(|profile| profile.target_connection_id.clone())
        })
        .unwrap_or_default();
    let ids = connections
        .iter()
        .map(|connection| connection.id.as_str())
        .collect::<BTreeSet<_>>();
    if (!source_id.is_empty() && !ids.contains(source_id.as_str()))
        || (!target_id.is_empty() && !ids.contains(target_id.as_str()))
    {
        effects.push(ProfileEffect::SetError(
            host.translate(RECONCILE_MISSING_CONNECTION_KEY),
        ));
        effects.push(ProfileEffect::SetErrorOpen(true));
        return (false, effects);
    }

    let mut profile_scope_can_be_restored = reusable.is_some();
    let mut source_databases = Vec::new();
    let mut target_databases = Vec::new();
    if let Some(profile) = reusable.as_ref() {
        match (
            host.list_databases(&profile.source_connection_id),
            host.list_databases(&profile.target_connection_id),
        ) {
            (Ok(source), Ok(target)) => {
                source_databases = source;
                target_databases = target;
                profile_scope_can_be_restored = profile
                    .source_database
                    .as_ref()
                    .map(|database| source_databases.contains(database))
                    .unwrap_or(false)
                    && profile
                        .target_database
                        .as_ref()
                        .map(|database| target_databases.contains(database))
                        .unwrap_or(false);
            }
            _ => profile_scope_can_be_restored = false,
        }
    }

    effects.push(ProfileEffect::BumpGeneration);
    effects.push(ProfileEffect::ClearJobRefs);
    let restored = if profile_scope_can_be_restored {
        reusable.clone()
    } else {
        None
    };
    effects.push(ProfileEffect::SetPendingMappings(
        restored.as_ref().map(|profile| profile.tables.clone()),
    ));

    state.selected_profile_id = restored
        .as_ref()
        .map(|profile| profile.id.clone())
        .unwrap_or_default();
    state.profile_name = restored
        .as_ref()
        .map(|profile| profile.name.clone())
        .unwrap_or_default();
    let source_database = restored
        .as_ref()
        .and_then(|profile| profile.source_database.clone())
        .unwrap_or_default();
    let target_database = restored
        .as_ref()
        .and_then(|profile| profile.target_database.clone())
        .unwrap_or_default();
    let source_schema = restored
        .as_ref()
        .and_then(|profile| profile.source_schema.clone())
        .unwrap_or_default();
    let target_schema = restored
        .as_ref()
        .and_then(|profile| profile.target_schema.clone())
        .unwrap_or_default();
    effects.push(ProfileEffect::SetSelectedProfileId(
        state.selected_profile_id.clone(),
    ));
    effects.push(ProfileEffect::SetProfileName(state.profile_name.clone()));
    effects.push(ProfileEffect::SetSourceId(source_id));
    effects.push(ProfileEffect::SetTargetId(target_id));
    effects.push(ProfileEffect::SetSourceDatabase(source_database));
    effects.push(ProfileEffect::SetTargetDatabase(target_database));
    effects.push(ProfileEffect::SetSourceSchema(source_schema));
    effects.push(ProfileEffect::SetTargetSchema(target_schema));
    effects.push(ProfileEffect::SetDatabases {
        source: source_databases,
        target: target_databases,
    });
    effects.push(ProfileEffect::ClearSchemas);
    effects.push(ProfileEffect::SetSyncOptions(
        restored
            .as_ref()
            .map(|profile| profile.options.clone())
            .unwrap_or_default(),
    ));
    effects.push(ProfileEffect::ResetCompareState);
    effects.push(ProfileEffect::ClearSelectedRows);
    effects.push(ProfileEffect::ClearTableSelections);
    effects.push(ProfileEffect::ClearPageIndex);
    effects.push(ProfileEffect::ClearPageCursors);
    effects.push(ProfileEffect::ClearLoadedPage);
    effects.push(ProfileEffect::SetLastExecutionResultNull);
    effects.push(ProfileEffect::SetWriteOutcomeUncertain(true));
    effects.push(ProfileEffect::SetScopeReconfirmationRequired(
        restored.is_none(),
    ));
    effects.push(ProfileEffect::SetSyncState("unknown".to_owned()));
    effects.push(ProfileEffect::SetStep("endpoints".to_owned()));

    let key = if state.selected_profile_id.is_empty() {
        if reusable.is_some() {
            RECONCILE_SCOPE_UNAVAILABLE_KEY
        } else if run.profile_id.is_some() {
            RECONCILE_PROFILE_CHANGED_KEY
        } else {
            RECONCILE_NEEDS_SCOPE_KEY
        }
    } else {
        RECONCILE_PROFILE_READY_KEY
    };
    effects.push(ProfileEffect::SetStatus(host.translate(key)));
    (true, effects)
}
