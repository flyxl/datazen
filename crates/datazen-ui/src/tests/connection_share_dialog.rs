//! T137: ConnectionShareDialog — export validation, import titles/actions and
//! modal geometry. Real probes against a mounted [`Dialog`] plus direct state
//! machine transitions; every rendered assertion reads the runtime element tree.

use rune::{col, row, Cx, El, Env, Runtime};

use crate::components::connection::connection_share_dialog::{
    export_filename, import_password_required, ipc_connection_share_error,
    translate_connection_share_error, utc_date_ymd, BrowseKind, ConnectionShareDialog,
    ExportRequest, ImportResult, ImportSource, ShareAction, ShareEvent, ShareMode, ShareOutput,
    ShareState, WIDTH_EXPORT, WIDTH_IMPORT_APP, WIDTH_IMPORT_FILE,
};
use crate::i18n::{has, t};

const ROOT: &str = "connection-share-dialog";
const DATE: &str = "20260619";

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

#[derive(Clone)]
struct Scene {
    state: ShareState,
    dark: bool,
    actions: Vec<ShareAction>,
    closes: usize,
    export_success: Vec<u64>,
    import_success: Vec<ImportResult>,
    errors: Vec<String>,
}

impl Scene {
    fn new(mode: ShareMode, source: ImportSource) -> Self {
        let mut state = ShareState::new(mode, source);
        state.open = true;
        Self {
            state,
            dark: true,
            actions: Vec::new(),
            closes: 0,
            export_success: Vec::new(),
            import_success: Vec::new(),
            errors: Vec::new(),
        }
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    ConnectionShareDialog::<Scene>::new(
        &s.state,
        |st: &mut Scene| &mut st.state.password,
        |st: &mut Scene| &mut st.state.confirm_password,
        |st: &mut Scene| &mut st.state.import.password,
        |st: &mut Scene| &mut st.state.import.data_path,
    )
    .date(DATE)
    .on_action(|st: &mut Scene, action: ShareAction| {
        st.actions.push(action.clone());
        if action == ShareAction::Close {
            st.closes += 1;
        }
    })
    .build(s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let mut rt = Runtime::new(scene, view, size, env());
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn key_count<S>(rt: &Runtime<S>, key: &str) -> usize {
    rt.elements()
        .iter()
        .filter(|e| e.key.as_deref() == Some(key))
        .count()
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements()
        .iter()
        .filter_map(|e| e.text.clone())
        .collect()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) outside {w}x{h}: {outside:?}",
        outside.len()
    );
}

#[test]
fn export_filename_uses_the_utc_date_and_the_datazenconnection_extension() {
    assert_eq!(
        export_filename(DATE),
        "datazen-connections-20260619.datazenconnection"
    );
    let today = utc_date_ymd();
    assert_eq!(today.len(), 8, "YYYYMMDD: {today}");
    assert!(today.bytes().all(|b| b.is_ascii_digit()), "digits only: {today}");
}

#[test]
fn export_validation_transitions_have_the_exact_keys_and_no_silent_success() {
    let mut state = ShareState::new(ShareMode::Export, ImportSource::File);
    // Empty password: React's `t('connShare.passwordRequired')`, no request.
    assert_eq!(state.begin_export(DATE), None);
    assert_eq!(state.local_error, t("connShare.passwordRequired"));
    assert!(!state.submitting, "a guard failure never marks submitting");

    // Mismatch: the other exact key, still no request.
    state.password = rune::TextEdit::new("secret".to_owned());
    state.confirm_password = rune::TextEdit::new("other".to_owned());
    assert_eq!(state.begin_export(DATE), None);
    assert_eq!(state.local_error, t("connShare.passwordMismatch"));

    // Matching, non-blank password: one request with the pinned filename.
    state.confirm_password = rune::TextEdit::new("secret".to_owned());
    let request = state
        .begin_export(DATE)
        .expect("a valid export stages exactly one request");
    assert_eq!(
        request,
        ExportRequest {
            password: "secret".into(),
            filename: "datazen-connections-20260619.datazenconnection".into(),
            epoch: 1,
        }
    );
    assert!(state.submitting, "submitting is set before the awaited call");
    assert!(state.local_error.is_empty(), "a valid attempt clears the error");

    // Success closes, then reports the count; submitting resets.
    let outputs = state.apply(
        ShareEvent::Exported {
            epoch: request.epoch,
            result: Ok(7),
        },
        t("common.exportFailed"),
    );
    assert_eq!(outputs, vec![ShareOutput::Close, ShareOutput::ExportSuccess(7)]);
    assert!(!state.submitting, "finally resets submitting on success");

    // A backend failure is mapped, never a generic success.
    let request2 = state
        .begin_export(DATE)
        .expect("a second export gets a fresh epoch");
    let outputs = state.apply(
        ShareEvent::Exported {
            epoch: request2.epoch,
            result: Err("DataZen decryption failed: wrong password or corrupt file".into()),
        },
        t("common.exportFailed"),
    );
    assert_eq!(
        outputs,
        vec![ShareOutput::Error(
            t("connShare.error.decryptionFailed")
                .replace("{format}", "DataZen")
        )],
        "the mapped IPC message replaces the generic fallback"
    );
    assert!(!state.submitting, "finally resets submitting on failure");

    // A stale answer cannot change anything (React's cancelled flag).
    let before = state.clone();
    assert!(state
        .apply(
            ShareEvent::Exported {
                epoch: request.epoch,
                result: Ok(99),
            },
            t("common.exportFailed"),
        )
        .is_empty());
    assert_eq!(state.submitting, before.submitting, "stale answer is inert");
}

#[test]
fn the_error_mapper_pins_exact_keys_format_patterns_prefixes_and_fallback() {
    let fallback = t("common.exportFailed");
    assert_eq!(ipc_connection_share_error("   ", fallback), fallback);
    assert_eq!(
        translate_connection_share_error("No connection files to import", fallback),
        t("connShare.error.noFiles")
    );
    assert_eq!(
        translate_connection_share_error("DBX decryption failed: wrong passphrase", fallback),
        t("connShare.error.dbxWrongPassphrase")
    );
    assert_eq!(
        translate_connection_share_error("Failed to read /some/file", fallback),
        t("connShare.error.readFailed")
    );
    assert_eq!(
        translate_connection_share_error("Unknown import source: mystery", fallback),
        t("connShare.error.unknownImportSource")
    );
    assert_eq!(
        translate_connection_share_error("Invalid DataGrip file: too short", fallback),
        "Invalid DataGrip file: too short",
        " React's FORMAT_LABELS list is fixed: a non-default label stays raw"
    );
    assert_eq!(
        translate_connection_share_error("Invalid TablePlus file: too short", fallback),
        t("connShare.error.invalidFileTooShort").replace("{format}", "TablePlus"),
        "a default format label matches the parameterised branch"
    );
    // The three FORMAT_LABELS drive the parameterised branches.
    for format in ["DataZen", "TablePlus", "Encrypted connection"] {
        let raw = format!("Password is required for {format} import");
        assert_eq!(
            translate_connection_share_error(&raw, fallback),
            t("connShare.error.formatPasswordRequired").replace("{format}", format),
            "{format}: formatPasswordRequired"
        );
    }
    let legacy = "DataZen decryption failed: wrong password or corrupt file (Legacy DataZen)";
    assert_eq!(
        translate_connection_share_error(legacy, fallback),
        t("connShare.error.decryptionFailedLegacyHint").replace("{format}", "DataZen")
    );
    assert_eq!(
        translate_connection_share_error("totally new backend text", fallback),
        "totally new backend text",
        "unknown messages are surfaced verbatim, not swallowed"
    );
    for key in [
        "connShare.error.passwordRequired",
        "connShare.error.decryptionFailed",
        "common.exportFailed",
    ] {
        assert!(has(key), "{key} must be in the locale table");
    }
}

#[test]
fn export_error_renders_and_submitting_disables_fields_and_footer() {
    let mut scene = Scene::new(ShareMode::Export, ImportSource::File);
    scene.state.local_error = t("connShare.passwordMismatch").to_owned();
    scene.state.submitting = true;
    let mut rt = runtime(scene, (960.0, 640.0));

    assert_eq!(
        element(&rt, &format!("{ROOT}-password-error")).text.as_deref(),
        Some(t("connShare.passwordMismatch")),
        "the validation message is the exact locale text"
    );
    // A text field's keyed element is the textbox shell; its focusable editor is
    // an inner node. Prove the disabled state behaviourally through input.
    click(&mut rt, &format!("{ROOT}-password"));
    rt.paste("ignored");
    rt.frame(0.0);
    assert_eq!(rt.state.state.password.text, "", "a disabled password field rejects typing");
    assert!(
        !element(&rt, &format!("{ROOT}-cancel")).focusable,
        "footer cancel is disabled while submitting"
    );
    let primary = element(&rt, &format!("{ROOT}-primary"));
    assert!(!primary.focusable, "footer primary is disabled while submitting");
    assert_eq!(primary.label.as_deref(), Some(t("connShare.exportAction")));

    // Ready state: the fields accept typing, the footer buttons are enabled,
    // and no error block is mounted.
    let mut ready = runtime(Scene::new(ShareMode::Export, ImportSource::File), (960.0, 640.0));
    click(&mut ready, &format!("{ROOT}-password"));
    ready.paste("typed");
    ready.frame(0.0);
    assert_eq!(ready.state.state.password.text, "typed", "a ready field accepts typing");
    assert!(element(&ready, &format!("{ROOT}-cancel")).focusable);
    assert!(element(&ready, &format!("{ROOT}-primary")).focusable);
    assert!(!has_key(&ready, &format!("{ROOT}-password-error")));
}

#[test]
fn footer_actions_reach_the_host_and_close_is_not_silent() {
    let mut rt = runtime(Scene::new(ShareMode::Export, ImportSource::File), (960.0, 640.0));
    click(&mut rt, &format!("{ROOT}-primary"));
    assert_eq!(
        rt.state.actions,
        vec![ShareAction::ExportSubmit { date: DATE.into() }],
        "primary press stages an export with the pinned date"
    );
    click(&mut rt, &format!("{ROOT}-cancel"));
    assert_eq!(rt.state.closes, 1, "footer cancel asks the host to close");
    click(&mut rt, &format!("{ROOT}-close"));
    assert_eq!(rt.state.closes, 2, "header close asks the host to close");
}

#[test]
fn import_titles_labels_and_widths_match_all_five_apps() {
    let title_for = |label: Option<&str>| match label {
        Some(label) => t("connShare.importFromAppTitle").replace("{app}", label),
        None => t("common.importConnections").to_owned(),
    };
    let cases = [
        (ImportSource::File, None, WIDTH_IMPORT_FILE),
        (ImportSource::Dbx, Some("DBX"), WIDTH_IMPORT_APP),
        (ImportSource::Navicat, Some("Navicat"), WIDTH_IMPORT_APP),
        (ImportSource::DataGrip, Some("DataGrip"), WIDTH_IMPORT_APP),
        (ImportSource::Dbeaver, Some("DBeaver"), WIDTH_IMPORT_APP),
        (ImportSource::TablePlus, Some("TablePlus"), WIDTH_IMPORT_APP),
    ];
    for (source, label, expected_width) in cases {
        let expected_title = title_for(label);
        let mut scene = Scene::new(ShareMode::Import, source);
        scene.state.open = true;
        let rt = runtime(scene, (960.0, 640.0));
        assert_eq!(
            element(&rt, &format!("{ROOT}-title")).text.as_deref(),
            Some(expected_title.as_str()),
            "{source:?}: dialog title"
        );
        let panel_width = element(&rt, ROOT).rect[2];
        assert!(
            (panel_width - expected_width).abs() <= 1.5,
            "{source:?}: width {panel_width} != {expected_width}"
        );
        assert_eq!(source.app_label(), label, "{source:?}: brand label");
        assert!(!expected_title.contains("{app}"), "{source:?}: placeholder substituted");
    }

    // Export has the narrowest width and its own title.
    let rt = runtime(Scene::new(ShareMode::Export, ImportSource::File), (960.0, 640.0));
    assert_eq!(
        element(&rt, &format!("{ROOT}-title")).text.as_deref(),
        Some(t("common.exportConnections"))
    );
    assert!((element(&rt, ROOT).rect[2] - WIDTH_EXPORT).abs() <= 1.5);
}

#[test]
fn import_primary_action_follows_the_two_step_file_flow_and_app_path_guard() {
    // File flow: first press picks a file, label is "choose file".
    let mut rt = runtime(
        Scene::new(ShareMode::Import, ImportSource::File),
        (960.0, 640.0),
    );
    assert_eq!(
        element(&rt, &format!("{ROOT}-primary")).label.as_deref(),
        Some(t("connShare.chooseImportFile"))
    );
    assert!(has_key(&rt, "import-formats-hint"));
    click(&mut rt, &format!("{ROOT}-primary"));
    assert_eq!(
        rt.state.actions,
        vec![ShareAction::ImportSubmit],
        "the primary press asks the host to run the import transition"
    );
    assert_eq!(
        rt.state.state.begin_import(),
        Some(ShareAction::PickImportFile { epoch: 1 }),
        "the first file-flow transition only picks a file"
    );
    let outputs = rt.state.state.apply(
        ShareEvent::FilePicked {
            epoch: 1,
            result: Ok(Some("/tmp/datazen-connections.datazenconnection".into())),
        },
        t("common.importFailed"),
    );
    assert!(outputs.is_empty(), "a pick has no success callback");
    assert_eq!(
        rt.state.state.import.selected_file.as_deref(),
        Some("/tmp/datazen-connections.datazenconnection")
    );
    assert_eq!(rt.state.state.primary_label(), t("connShare.importAction"), "second label");
    assert!(
        import_password_required("/tmp/datazen-connections.datazenconnection"),
        "encrypted file needs a password"
    );
    let guard = rt.state.state.begin_import();
    assert_eq!(guard, None, "an encrypted file with no password is refused");
    assert_eq!(
        rt.state.state.import.local_error,
        t("connShare.encryptedImportPasswordRequired")
    );

    // App flow with no detected path: pathRequired, no IPC staged.
    let mut state = ShareState::new(ShareMode::Import, ImportSource::Navicat);
    state.open = true;
    assert_eq!(state.begin_import(), None);
    assert!(state.local_error.is_empty(), "app errors live on the import slice");
    assert_eq!(state.import.local_error, t("connShare.pathRequired"));

    // With a detected path the app import is staged with source + path.
    state.import.path_found = true;
    state.import.data_path = rune::TextEdit::new("/Applications/Navicat Premium.app".to_owned());
    let action = state.begin_import().expect("a path-bearing app import is staged");
    assert_eq!(
        action,
        ShareAction::ImportFromApp {
            epoch: 1,
            password: String::new(),
            path: "/Applications/Navicat Premium.app".into(),
            source: ImportSource::Navicat,
        }
    );
    let outputs = state.apply(
        ShareEvent::Imported {
            epoch: 1,
            result: Ok(ImportResult {
                imported: 2,
                source_format: Some("DBX".into()),
                ..Default::default()
            }),
        },
        t("common.importFailed"),
    );
    assert_eq!(
        outputs,
        vec![
            ShareOutput::Close,
            ShareOutput::ImportSuccess(ImportResult {
                imported: 2,
                source_format: Some("DBX".into()),
                ..Default::default()
            })
        ]
    );
}

#[test]
fn import_browse_and_path_hints_follow_the_app_source() {
    let mut rt = runtime(
        Scene::new(ShareMode::Import, ImportSource::Dbx),
        (960.0, 640.0),
    );
    assert!(has_key(&rt, "import-data-path"));
    assert!(has_key(&rt, "import-browse-folder"));
    assert!(has_key(&rt, "import-browse-file"));
    assert!(!has_key(&rt, "import-selected-file"));
    assert_eq!(
        element(&rt, "import-data-path-hint").text.as_deref(),
        Some(t("connShare.dataPathMissingHint"))
    );

    click(&mut rt, "import-browse-folder");
    assert!(
        rt.state.actions.contains(&ShareAction::BrowsePath {
            source: ImportSource::Dbx,
            kind: BrowseKind::Folder,
        }),
        "folder browse reports the source"
    );
    let outputs = rt.state.state.apply(
        ShareEvent::PathPicked {
            epoch: rt.state.state.epoch,
            result: Ok("/custom/dbx-data".into()),
        },
        t("common.importFailed"),
    );
    assert!(outputs.is_empty());
    assert_eq!(
        rt.state.state.import.data_path.text,
        "/custom/dbx-data",
        "the picked path lands in the import state"
    );

    let mut next = Scene::new(ShareMode::Import, ImportSource::Dbx);
    next.state.import.path_found = true;
    next.state.import.data_path = rune::TextEdit::new("/custom/dbx-data".to_owned());
    let rt = runtime(next, (960.0, 640.0));
    assert_eq!(
        element(&rt, "import-data-path-hint").text.as_deref(),
        Some(t("connShare.dataPathFoundHint"))
    );
}

#[test]
fn dialog_keys_are_stable_and_geometry_holds_at_both_sizes_in_both_themes() {
    let owned = [
        ROOT.to_owned(),
        format!("{ROOT}-title"),
        format!("{ROOT}-body"),
        format!("{ROOT}-footer"),
        format!("{ROOT}-close"),
        format!("{ROOT}-password"),
        format!("{ROOT}-confirm-password"),
        format!("{ROOT}-cancel"),
        format!("{ROOT}-primary"),
    ];
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut scene = Scene::new(ShareMode::Export, ImportSource::File);
            scene.dark = dark;
            scene.state.local_error = t("connShare.passwordRequired").to_owned();
            let rt = runtime(scene, (w, h));
            assert_in_window(&rt, w, h, &format!("export {w}x{h} dark={dark}"));
            for key in &owned {
                assert_eq!(key_count(&rt, key), 1, "{key} appears once");
            }
            for e in rt.elements() {
                if let Some(key) = e.key.as_deref() {
                    assert!(!key.contains("#dup"), "a key was duplicated: {key}");
                }
            }

            let mut scene = Scene::new(ShareMode::Import, ImportSource::TablePlus);
            scene.dark = dark;
            scene.state.import.path_found = true;
            scene.state.import.data_path = rune::TextEdit::new("/Applications/TablePlus.app".to_owned());
            let rt = runtime(scene, (w, h));
            assert_in_window(&rt, w, h, &format!("import {w}x{h} dark={dark}"));
            let import_keys = [
                ROOT.to_owned(),
                "connection-import-fields".to_owned(),
                "import-data-path".to_owned(),
                "import-browse-folder".to_owned(),
                "import-browse-file".to_owned(),
                "import-password-input".to_owned(),
                format!("{ROOT}-cancel"),
                format!("{ROOT}-primary"),
            ];
            for key in &import_keys {
                assert!(has_key(&rt, key), "import key: {key}");
            }
        }
    }
}

// Reverse verification (mutation -> observed failure text -> restored):
// 1. Returning `"wrong"` from `export_filename` made
//    `export_filename_uses_the_utc_date_and_the_datazenconnection_extension`
//    fail with: `left: "wrong" right: "datazen-connections-20260619.datazenconnection"`.
// 2. Mapping `connShare.passwordMismatch` to `connShare.passwordRequired` in
//    `begin_export` made `export_validation_transitions_have_the_exact_keys_and_no_silent_success`
//    fail with: `left: "请输入密码" right: "两次输入的密码不一致"`.
