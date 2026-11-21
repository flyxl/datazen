mod reconcile_tests;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::SyncOptions;

#[derive(Default)]
struct Host {
    profiles: Vec<SyncProfile>,
    connections: Vec<ProfileConnection>,
    databases: BTreeMap<String, Vec<String>>,
    save_error: Option<String>,
    delete_error: Option<String>,
    saved: RefCell<Vec<SyncProfile>>,
    deleted: RefCell<Vec<String>>,
    id_seq: RefCell<u64>,
    translations: RefCell<Vec<String>>,
}

impl ProfilesHost for Host {
    fn translate(&self, key: &str) -> String {
        self.translations.borrow_mut().push(key.to_owned());
        format!("i18n:{key}")
    }

    fn get_profiles(&self) -> Result<Vec<SyncProfile>, String> {
        Ok(self.profiles.clone())
    }

    fn save_profile(&self, profile: &SyncProfile) -> Result<(), String> {
        self.saved.borrow_mut().push(profile.clone());
        match &self.save_error {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }

    fn delete_profile(&self, id: &str) -> Result<(), String> {
        self.deleted.borrow_mut().push(id.to_owned());
        match &self.delete_error {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }

    fn get_connections(&self) -> Result<Vec<ProfileConnection>, String> {
        Ok(self.connections.clone())
    }

    fn list_databases(&self, connection_id: &str) -> Result<Vec<String>, String> {
        self.databases
            .get(connection_id)
            .cloned()
            .ok_or_else(|| "catalog unavailable".to_owned())
    }

    fn new_id(&self) -> String {
        let mut value = self.id_seq.borrow_mut();
        *value += 1;
        format!("new-{value}")
    }

    fn now_iso(&self) -> String {
        "2026-01-02T03:04:05.000Z".to_owned()
    }
}

fn profile(id: &str, source: &str, target: &str) -> SyncProfile {
    SyncProfile {
        version: 1,
        id: id.to_owned(),
        name: "Saved".to_owned(),
        source_connection_id: source.to_owned(),
        target_connection_id: target.to_owned(),
        source_database: Some("app".to_owned()),
        target_database: Some("warehouse".to_owned()),
        source_schema: None,
        target_schema: Some("public".to_owned()),
        tables: Vec::new(),
        options: SyncOptions::default(),
        created_at: "2026-01-01T00:00:00.000Z".to_owned(),
        updated_at: "2026-01-01T00:00:00.000Z".to_owned(),
    }
}

fn endpoints() -> ProfileEndpointState {
    ProfileEndpointState {
        source_id: "c1".to_owned(),
        target_id: "c2".to_owned(),
        source_database: "app".to_owned(),
        target_database: "warehouse".to_owned(),
        source_schema: String::new(),
        target_schema: "public".to_owned(),
        connections: vec![
            ProfileConnection {
                id: "c1".to_owned(),
            },
            ProfileConnection {
                id: "c2".to_owned(),
            },
        ],
    }
}

fn has_effect(effects: &[ProfileEffect], expected: &ProfileEffect) -> bool {
    effects.iter().any(|effect| effect == expected)
}

#[test]
fn current_tables_fall_back_to_pending_and_filter_invalid_rows() {
    let pending = vec![DataSyncTableMapping {
        source_table: "old".to_owned(),
        target_table: "old".to_owned(),
        enabled: true,
        source_filter: None,
    }];
    assert_eq!(
        current_profile_tables(&[], &BTreeSet::new(), Some(&pending)),
        pending
    );

    let rows = vec![
        MappingRowLike {
            source_table: " a ".to_owned(),
            target_table: "b".to_owned(),
            source_filter: Some("x=1".to_owned()),
            status: "OK".to_owned(),
        },
        MappingRowLike {
            source_table: "".to_owned(),
            target_table: "b".to_owned(),
            source_filter: None,
            status: "OK".to_owned(),
        },
        MappingRowLike {
            source_table: "c".to_owned(),
            target_table: "d".to_owned(),
            source_filter: None,
            status: "DISABLED".to_owned(),
        },
    ];
    let disabled = BTreeSet::from([" a ".to_owned()]);
    let mapped = current_profile_tables(&rows, &disabled, Some(&pending));
    assert_eq!(mapped.len(), 2);
    assert!(!mapped[0].enabled);
    assert_eq!(mapped[0].source_filter.as_deref(), Some("x=1"));
    assert!(!mapped[1].enabled);
}

#[test]
fn saving_requires_name_and_both_endpoint_ids() {
    let h = Host::default();
    let mut state = ProfilesState::default();
    for name in ["", "  "] {
        state.profile_name = name.to_owned();
        let effects = save_current_profile(
            &mut state,
            &h,
            &endpoints(),
            &SyncOptions::default(),
            &[],
            &BTreeSet::new(),
            None,
        );
        assert_eq!(
            effects,
            vec![
                ProfileEffect::SetError("i18n:sync.profile.missingFields".to_owned()),
                ProfileEffect::SetErrorOpen(true),
            ]
        );
    }
    state.profile_name = "N".to_owned();
    let mut missing = endpoints();
    missing.source_id.clear();
    let effects = save_current_profile(
        &mut state,
        &h,
        &missing,
        &SyncOptions::default(),
        &[],
        &BTreeSet::new(),
        None,
    );
    assert_eq!(effects.len(), 2);
    assert!(h.saved.borrow().is_empty());
}

#[test]
fn saving_a_named_profile_preserves_identity_and_reloads_the_list() {
    let existing = profile("old-id", "c1", "c2");
    let h = Host {
        profiles: vec![existing.clone()],
        ..Host::default()
    };
    let mut state = ProfilesState {
        profile_name: "  My profile  ".to_owned(),
        selected_profile_id: "old-id".to_owned(),
        sync_profiles: vec![existing.clone()],
    };
    let effects = save_current_profile(
        &mut state,
        &h,
        &endpoints(),
        &SyncOptions::default(),
        &[],
        &BTreeSet::new(),
        None,
    );
    let saved = &h.saved.borrow()[0];
    assert_eq!(saved.id, "old-id");
    assert_eq!(saved.name, "My profile");
    assert_eq!(saved.created_at, existing.created_at);
    assert_eq!(saved.updated_at, "2026-01-02T03:04:05.000Z");
    assert_eq!(saved.source_database.as_deref(), Some("app"));
    assert_eq!(saved.target_schema.as_deref(), Some("public"));
    assert_eq!(saved.version, 1);
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetStatus("i18n:sync.profile.saved".to_owned())
    ));
    assert_eq!(state.profile_name, "My profile");
}

#[test]
fn a_failed_save_reports_its_message() {
    let h = Host {
        save_error: Some("denied".to_owned()),
        ..Host::default()
    };
    let mut state = ProfilesState {
        profile_name: "N".to_owned(),
        ..ProfilesState::default()
    };
    let effects = save_current_profile(
        &mut state,
        &h,
        &endpoints(),
        &SyncOptions::default(),
        &[],
        &BTreeSet::new(),
        None,
    );
    assert_eq!(
        effects,
        vec![
            ProfileEffect::SetError("denied".to_owned()),
            ProfileEffect::SetErrorOpen(true)
        ]
    );
    assert_eq!(h.saved.borrow()[0].id, "new-1");
}

#[test]
fn loading_a_profile_checks_that_both_connections_still_exist() {
    let h = Host::default();
    let mut state = ProfilesState {
        sync_profiles: vec![profile("p", "c1", "c2")],
        selected_profile_id: "p".to_owned(),
        ..ProfilesState::default()
    };
    let mut missing = endpoints();
    missing.connections.pop();
    let effects = load_selected_profile(&mut state, &h, &missing);
    assert_eq!(
        effects,
        vec![
            ProfileEffect::SetError("i18n:sync.profile.missingConnection".to_owned()),
            ProfileEffect::SetErrorOpen(true),
        ]
    );
}

#[test]
fn loading_a_profile_emits_the_restore_effects() {
    let h = Host::default();
    let mut state = ProfilesState {
        sync_profiles: vec![profile("p", "c1", "c2")],
        selected_profile_id: "p".to_owned(),
        ..ProfilesState::default()
    };
    let effects = load_selected_profile(&mut state, &h, &endpoints());
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetProfileName("Saved".to_owned())
    ));
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetSourceId("c1".to_owned())
    ));
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetTargetId("c2".to_owned())
    ));
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetSourceDatabase("app".to_owned())
    ));
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetTargetSchema("public".to_owned())
    ));
    assert!(has_effect(&effects, &ProfileEffect::ResetCompareState));
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetStep("endpoints".to_owned())
    ));
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetStatus("i18n:sync.profile.loaded".to_owned())
    ));
    assert!(matches!(
        effects[0],
        ProfileEffect::SetPendingMappings(Some(_))
    ));
}

#[test]
fn refresh_and_delete_keep_the_state_observable() {
    let h = Host {
        profiles: vec![profile("p", "c1", "c2")],
        ..Host::default()
    };
    let mut state = ProfilesState {
        selected_profile_id: "p".to_owned(),
        ..ProfilesState::default()
    };
    assert_eq!(load_sync_profiles(&mut state, &h).len(), 1);
    assert_eq!(state.sync_profiles.len(), 1);
    let effects = delete_selected_profile(&mut state, &h);
    assert_eq!(h.deleted.borrow().as_slice(), &["p".to_owned()]);
    assert!(state.selected_profile_id.is_empty());
    assert!(has_effect(
        &effects,
        &ProfileEffect::SetStatus("i18n:sync.profile.deleted".to_owned())
    ));
    assert!(delete_selected_profile(&mut state, &h).is_empty());
}

#[test]
fn gap_and_surface_are_pinned_and_root_reexports_match() {
    assert_eq!(DATA_SYNC_PROFILES_SURFACE.len(), 10);
    assert!(DATA_SYNC_PROFILES_SURFACE.contains(&"reconcileUnknownRun"));
    assert_eq!(DATA_SYNC_PROFILES_GAP.len(), 7);
    assert_eq!(crate::DATA_SYNC_PROFILES_SURFACE.len(), 10);
    assert_eq!(crate::DATA_SYNC_PROFILES_GAP.len(), 7);
    let _: Option<crate::SyncProfile> = Some(profile("p", "c1", "c2"));
}
