use super::*;
use crate::{MigrationRunRecordLike, ProfileEffect, ReconcileInput};

fn run(operation: &str, outcome: &str) -> MigrationRunRecordLike {
    MigrationRunRecordLike {
        operation: operation.to_owned(),
        outcome: outcome.to_owned(),
        ..MigrationRunRecordLike::default()
    }
}

fn matching_run(p: &SyncProfile) -> MigrationRunRecordLike {
    MigrationRunRecordLike {
        operation: "dataSync".to_owned(),
        outcome: "unknown".to_owned(),
        profile_id: Some(p.id.clone()),
        profile_revision: Some(p.updated_at.clone()),
        source_connection_id: None,
        target_connection_id: None,
    }
}

fn host_with_catalogs() -> Host {
    Host {
        connections: vec![
            ProfileConnection {
                id: "c1".to_owned(),
            },
            ProfileConnection {
                id: "c2".to_owned(),
            },
        ],
        databases: BTreeMap::from([
            ("c1".to_owned(), vec!["app".to_owned(), "other".to_owned()]),
            ("c2".to_owned(), vec!["warehouse".to_owned()]),
        ]),
        ..Host::default()
    }
}

#[test]
fn parsing_an_iso_timestamp_is_exact_to_the_millisecond() {
    assert_eq!(parse_timestamp("1970-01-01T00:00:00.000Z"), Some(0));
    assert_eq!(parse_timestamp("1970-01-01T00:00:01.500Z"), Some(1_500));
    assert_eq!(
        parse_timestamp("2026-01-01T00:00:00.000Z"),
        parse_timestamp("2026-01-01T00:00:00Z")
    );
    assert_eq!(parse_timestamp("not-a-date"), None);
    assert_eq!(parse_timestamp("2026-13-01T00:00:00.000Z"), None);
}

#[test]
fn only_unknown_data_sync_runs_are_reconciled() {
    let h = host_with_catalogs();
    let mut state = ProfilesState::default();
    assert_eq!(
        reconcile_unknown_run(
            &mut state,
            &h,
            &run("transfer", "unknown"),
            ReconcileInput {
                write_in_flight: false
            }
        )
        .0,
        false
    );
    assert_eq!(
        reconcile_unknown_run(
            &mut state,
            &h,
            &run("dataSync", "succeeded"),
            ReconcileInput {
                write_in_flight: false
            }
        )
        .0,
        false
    );
}

#[test]
fn a_running_write_refuses_reconciliation() {
    let h = host_with_catalogs();
    let mut state = ProfilesState::default();
    let (ok, effects) = reconcile_unknown_run(
        &mut state,
        &h,
        &run("dataSync", "unknown"),
        ReconcileInput {
            write_in_flight: true,
        },
    );
    assert!(!ok);
    assert_eq!(
        effects,
        vec![
            ProfileEffect::SetError("i18n:migrationHistory.syncReconcileBusy".to_owned()),
            ProfileEffect::SetErrorOpen(true),
        ]
    );
}

#[test]
fn a_missing_live_connection_refuses_reconciliation() {
    let p = profile("p", "c1", "c2");
    let h = Host {
        profiles: vec![p.clone()],
        databases: BTreeMap::new(),
        ..Host::default()
    };
    let mut state = ProfilesState::default();
    let (ok, effects) = reconcile_unknown_run(
        &mut state,
        &h,
        &matching_run(&p),
        ReconcileInput {
            write_in_flight: false,
        },
    );
    assert!(!ok);
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetError("i18n:migrationHistory.syncReconcileMissingConnection".to_owned())
    ));
}

#[test]
fn a_matching_profile_with_live_catalogs_restores_its_scope() {
    let p = profile("p", "c1", "c2");
    let mut tables = p.clone();
    tables.tables = vec![DataSyncTableMapping {
        source_table: "t".to_owned(),
        target_table: "t2".to_owned(),
        enabled: true,
        source_filter: None,
    }];
    let h = Host {
        profiles: vec![tables.clone()],
        ..host_with_catalogs()
    };
    let mut state = ProfilesState::default();
    let (ok, effects) = reconcile_unknown_run(
        &mut state,
        &h,
        &matching_run(&tables),
        ReconcileInput {
            write_in_flight: false,
        },
    );
    assert!(ok);
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetStatus("i18n:migrationHistory.syncReconcileProfileReady".to_owned())
    ));
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetWriteOutcomeUncertain(true)
    ));
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetScopeReconfirmationRequired(false)
    ));
    assert!(has_effect(&effects, &ProfileEffect::BumpGeneration));
    assert!(has_effect(&effects, &ProfileEffect::ClearJobRefs));
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetSyncState("unknown".to_owned())
    ));
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetPendingMappings(Some(tables.tables.clone()))
    ));
    assert_eq!(state.selected_profile_id, "p");
}

#[test]
fn a_failed_catalog_lookup_falls_back_to_scope_reconfirmation() {
    let p = profile("p", "c1", "c2");
    let h = Host {
        profiles: vec![p.clone()],
        connections: vec![
            ProfileConnection {
                id: "c1".to_owned(),
            },
            ProfileConnection {
                id: "c2".to_owned(),
            },
        ],
        databases: BTreeMap::new(),
        ..Host::default()
    };
    let mut state = ProfilesState::default();
    let (ok, effects) = reconcile_unknown_run(
        &mut state,
        &h,
        &matching_run(&p),
        ReconcileInput {
            write_in_flight: false,
        },
    );
    assert!(ok);
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetStatus("i18n:migrationHistory.syncReconcileScopeUnavailable".to_owned())
    ));
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetScopeReconfirmationRequired(true)
    ));
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetPendingMappings(None)
    ));
}

#[test]
fn a_changed_or_absent_profile_selects_the_right_message() {
    let p = profile("p", "c1", "c2");
    let h = Host {
        profiles: vec![p.clone()],
        ..host_with_catalogs()
    };
    let changed = MigrationRunRecordLike {
        profile_id: Some("missing".to_owned()),
        profile_revision: None,
        ..matching_run(&p)
    };
    let mut state = ProfilesState::default();
    let (_, effects) = reconcile_unknown_run(
        &mut state,
        &h,
        &changed,
        ReconcileInput {
            write_in_flight: false,
        },
    );
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetStatus("i18n:migrationHistory.syncReconcileProfileChanged".to_owned())
    ));

    let no_profile = MigrationRunRecordLike {
        profile_id: None,
        profile_revision: None,
        ..matching_run(&p)
    };
    let (_, effects) = reconcile_unknown_run(
        &mut state,
        &h,
        &no_profile,
        ReconcileInput {
            write_in_flight: false,
        },
    );
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetStatus("i18n:migrationHistory.syncReconcileNeedsScope".to_owned())
    ));
}

#[test]
fn run_level_connection_ids_override_the_profile_and_a_revision_mismatch_drops_reuse() {
    let p = profile("p", "c1", "c2");
    let h = Host {
        profiles: vec![p.clone()],
        ..host_with_catalogs()
    };
    let mut state = ProfilesState::default();
    let run = MigrationRunRecordLike {
        profile_revision: Some("2020-01-01T00:00:00.000Z".to_owned()),
        source_connection_id: Some("c1".to_owned()),
        target_connection_id: Some("c2".to_owned()),
        ..matching_run(&p)
    };
    let (_, effects) = reconcile_unknown_run(
        &mut state,
        &h,
        &run,
        ReconcileInput {
            write_in_flight: false,
        },
    );
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetSourceId("c1".to_owned())
    ));
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetPendingMappings(None)
    ));
}
