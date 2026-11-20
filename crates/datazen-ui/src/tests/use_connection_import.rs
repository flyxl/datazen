//! T346: `useConnectionImport` hook port — real branch tests.
//!
//! Every transition is driven through `ImportState::step` with pinned effects
//! and callback events; no runtime, no fake IPC, no threads.

use crate::components::connection::connection_import_fields::PasswordPolicy;
use crate::components::connection::connection_share_dialog::{
    BrowseKind, ImportResult, ImportSource,
};
use crate::components::connection::use_connection_import::{
    ImportEffect, ImportEvent, ImportInput, ImportState, ImportStrings,
};

fn strings() -> ImportStrings {
    ImportStrings {
        path_required: "请填写数据目录、安装路径或导出文件".to_owned(),
        password_required: "加密连接文件需要输入密码".to_owned(),
        import_failed: "导入失败".to_owned(),
        choose_file: "选择文件…".to_owned(),
        import_action: "导入".to_owned(),
    }
}

fn result() -> ImportResult {
    ImportResult {
        imported: 3,
        overwritten: 1,
        groups_added: 2,
        skipped: vec!["dup".to_owned()],
        source_format: Some("datazen".to_owned()),
    }
}

fn app_state() -> ImportState {
    ImportState::new(ImportSource::TablePlus, true)
}

fn file_state() -> ImportState {
    ImportState::new(ImportSource::File, true)
}

#[test]
fn disabled_state_is_empty_and_skips_detection() {
    let mut state = ImportState::new(ImportSource::Dbx, false);
    assert_eq!(state.password(), "");
    assert_eq!(state.data_path(), "");
    assert!(!state.path_found());
    assert!(!state.detecting());
    assert!(!state.submitting());
    assert!(!state.busy());
    assert_eq!(state.local_error(), "");
    assert_eq!(state.selected_file(), None);
    assert!(state.app_import(), "a non-file source is an app import");

    let step = state.step(ImportInput::Enabled(false), &strings());
    assert!(step.effects.is_empty(), "disabled state never detects");
    assert!(step.events.is_empty());
}

#[test]
fn enabling_an_app_source_detects_and_discards_stale_answers() {
    let mut state = ImportState::new(ImportSource::TablePlus, false);
    let step = state.step(ImportInput::Enabled(true), &strings());
    assert_eq!(
        step.effects,
        vec![ImportEffect::DetectPath {
            source: ImportSource::TablePlus,
            generation: 1,
        }]
    );
    assert!(state.detecting());
    assert!(state.busy(), "detection makes the hook busy");

    let stale = state.step(
        ImportInput::Detected {
            path: "old".to_owned(),
            found: true,
            generation: 0,
        },
        &strings(),
    );
    assert!(stale.effects.is_empty());
    assert_eq!(state.data_path(), "", "a stale answer is ignored");
    assert!(state.detecting());

    let fresh = state.step(
        ImportInput::Detected {
            path: "/Applications/TablePlus".to_owned(),
            found: true,
            generation: 1,
        },
        &strings(),
    );
    assert!(fresh.effects.is_empty());
    assert_eq!(state.data_path(), "/Applications/TablePlus");
    assert!(state.path_found());
    assert!(!state.detecting());
}

#[test]
fn detect_failure_clears_the_path_and_the_detecting_flag() {
    let mut state = app_state();
    state.set_data_path("/stale");
    state.set_path_found(true);
    let _ = state.step(ImportInput::DetectFailed { generation: 0 }, &strings());
    assert_eq!(state.data_path(), "");
    assert!(!state.path_found());
    assert!(!state.detecting());
    assert_eq!(state.import_password_policy(), None);
}

#[test]
fn file_source_has_no_detection_and_chooses_the_primary_label() {
    let mut state = file_state();
    let enabled = state.step(ImportInput::Enabled(true), &strings());
    assert!(enabled.effects.is_empty(), "the file flow has no path detection");
    assert!(!state.app_import());
    assert_eq!(state.primary_action_label(&strings()), "选择文件…");

    let picked = state.step(ImportInput::FilePicked(Some("conns.json".to_owned())), &strings());
    assert!(picked.effects.is_empty());
    assert_eq!(state.primary_action_label(&strings()), "导入");
    assert_eq!(state.import_password_policy(), Some(PasswordPolicy::Optional));
    assert_eq!(
        state.to_vm(false).selected_file.as_deref(),
        Some("conns.json"),
        "the widget VM mirrors the slice"
    );
}

#[test]
fn browse_path_is_app_only_and_reports_picked_or_failed() {
    let mut file = file_state();
    let none = file.step(ImportInput::BrowsePath(BrowseKind::Folder), &strings());
    assert!(none.effects.is_empty(), "file flow has no browse buttons");

    let mut state = app_state();
    let step = state.step(ImportInput::BrowsePath(BrowseKind::Folder), &strings());
    assert_eq!(
        step.effects,
        vec![ImportEffect::BrowsePath {
            kind: BrowseKind::Folder,
            source: ImportSource::TablePlus,
            generation: 0,
        }]
    );
    state.set_local_error("old error");
    let picked = state.step(
        ImportInput::BrowsePicked {
            path: "/chosen".to_owned(),
            generation: 0,
        },
        &strings(),
    );
    assert!(picked.effects.is_empty());
    assert_eq!(state.data_path(), "/chosen");
    assert!(state.path_found());
    assert_eq!(state.local_error(), "", "a successful pick clears the error");

    let failed = state.step(
        ImportInput::BrowseFailed {
            raw: "Unknown import source: x".to_owned(),
            generation: 0,
        },
        &strings(),
    );
    assert_eq!(
        failed.events,
        vec![ImportEvent::Error(
            crate::components::connection::connection_share_dialog::ipc_connection_share_error(
                "Unknown import source: x",
                "导入失败",
            )
        )]
    );
    assert_eq!(state.local_error(), failed.events[0].clone().into_error());
}

trait IntoErrorString {
    fn into_error(self) -> String;
}
impl IntoErrorString for ImportEvent {
    fn into_error(self) -> String {
        match self {
            ImportEvent::Error(message) => message,
            other => panic!("expected Error, got {other:?}"),
        }
    }
}

#[test]
fn submit_validates_path_and_the_two_step_file_flow() {
    let mut app = app_state();
    let step = app.step(ImportInput::Submit, &strings());
    assert!(step.effects.is_empty());
    assert_eq!(
        step.events,
        vec![ImportEvent::Error(strings().path_required)],
        "an empty app path is rejected before submitting"
    );

    let mut file = file_state();
    let first = file.step(ImportInput::Submit, &strings());
    assert_eq!(first.effects, vec![ImportEffect::PickImportFile]);
    assert!(file.submitting(), "the first file call is submitting while picking");
    let second = file.step(
        ImportInput::FilePicked(Some("backup.datazenconnection".to_owned())),
        &strings(),
    );
    assert!(second.effects.is_empty(), "picking a file returns without importing");
    assert!(!file.submitting());
    assert_eq!(file.import_password_policy(), Some(PasswordPolicy::Required));

    let blocked = file.step(ImportInput::Submit, &strings());
    assert!(blocked.effects.is_empty());
    assert_eq!(
        blocked.events,
        vec![ImportEvent::Error(strings().password_required)],
        "an encrypted import needs a password"
    );

    file.set_password("secret");
    let ready = file.step(ImportInput::Submit, &strings());
    assert_eq!(
        ready.effects,
        vec![ImportEffect::ImportAtPath {
            password: "secret".to_owned(),
            file: "backup.datazenconnection".to_owned(),
        }]
    );
    assert!(file.submitting());
}

#[test]
fn app_submit_sends_password_and_path_then_reports_success() {
    let mut state = app_state();
    state.set_password("pw");
    state.set_data_path("/data");
    state.set_path_found(true);
    let step = state.step(ImportInput::Submit, &strings());
    assert_eq!(
        step.effects,
        vec![ImportEffect::ImportFromApp {
            source: ImportSource::TablePlus,
            password: "pw".to_owned(),
            data_path: "/data".to_owned(),
        }]
    );
    let done = state.step(ImportInput::ImportOk(result()), &strings());
    assert!(!state.submitting());
    assert_eq!(done.events, vec![ImportEvent::Success(result())]);
}

#[test]
fn import_failure_maps_the_raw_error_and_notifies_the_caller() {
    let mut state = app_state();
    state.set_data_path("/data");
    let _ = state.step(ImportInput::Submit, &strings());
    let step = state.step(ImportInput::ImportFailed("boom".to_owned()), &strings());
    assert!(!state.submitting());
    assert_eq!(
        step.events,
        vec![ImportEvent::Error("boom".to_owned())],
        "a non-API message is returned verbatim"
    );
    assert_eq!(state.local_error(), "boom");
}

#[test]
fn pick_file_failure_reports_without_changing_the_selected_file() {
    let mut state = file_state();
    let step = state.step(ImportInput::PickImportFile, &strings());
    assert_eq!(step.effects, vec![ImportEffect::PickImportFile]);
    let failed = state.step(ImportInput::FilePickFailed("denied".to_owned()), &strings());
    assert!(!state.submitting());
    assert_eq!(state.selected_file(), None);
    assert_eq!(failed.events, vec![ImportEvent::Error("denied".to_owned())]);
}
