//! Independent tester audit for T137 (`ConnectionShareDialog`).
//!
//! Precise probes for branches the port's own tests leave open, judged against
//! `ConnectionShareDialog.tsx` + `lib/connectionShareError.ts` and the T103
//! `ConnectionImportFields` contract the dialog is supposed to reuse:
//!
//! 1. the complete 13-entry exact error table and every format-parameterised
//!    branch, including branch-order precedence;
//! 2. stale-epoch inertness for ALL five host event kinds, not just `Exported`;
//! 3. import footer disable expressions under `submitting`/`detecting`;
//! 4. React-faithful visibility of the import password field;
//! 5. React-faithful placement of a backend export error (callback vs inline);
//! 6. rect-based geometry with adversarial long content at both sizes/themes.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::connection::connection_share_dialog::{
    translate_connection_share_error, ConnectionShareDialog, ImportResult, ImportSource,
    ShareAction, ShareEvent, ShareMode, ShareOutput, ShareState,
};
use crate::i18n::t;

const ROOT: &str = "connection-share-dialog";
const DATE: &str = "20260619";

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

#[derive(Clone)]
struct AuditScene {
    state: ShareState,
    dark: bool,
}

impl AuditScene {
    fn new(mode: ShareMode, source: ImportSource) -> Self {
        let mut state = ShareState::new(mode, source);
        state.open = true;
        Self { state, dark: true }
    }
}

fn view(s: &AuditScene, _cx: &mut Cx<AuditScene>) -> El {
    let mut layer = Vec::new();
    ConnectionShareDialog::<AuditScene>::new(
        &s.state,
        |st: &mut AuditScene| &mut st.state.password,
        |st: &mut AuditScene| &mut st.state.confirm_password,
        |st: &mut AuditScene| &mut st.state.import.password,
        |st: &mut AuditScene| &mut st.state.import.data_path,
    )
    .date(DATE)
    .on_action(|_st: &mut AuditScene, _a: ShareAction| {})
    .build(s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn runtime(scene: AuditScene, size: (f32, f32)) -> Runtime<AuditScene> {
    let mut rt = Runtime::new(scene, view, size, env());
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn assert_inside<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(outside.is_empty(), "{label}: {} node(s) outside {w}x{h}: {outside:?}", outside.len());
}

#[test]
fn error_mapper_covers_the_full_exact_table_and_every_format_pattern() {
    let fallback = t("common.exportFailed");
    // All 13 exact entries from connectionShareError.ts, in source order.
    let exact = [
        ("Password is required", "connShare.error.passwordRequired"),
        ("Password is required for encrypted connection import", "connShare.error.encryptedImportPasswordRequired"),
        ("Password is required for DBX encrypted import", "connShare.error.dbxEncryptedPasswordRequired"),
        ("Password is required for DataZen encrypted import", "connShare.error.datazenEncryptedPasswordRequired"),
        ("Password is required for DataZen import", "connShare.error.datazenPasswordRequired"),
        ("Import file is not valid UTF-8 text (for DataZen use .datazenconnection)", "connShare.error.invalidUtf8"),
        ("Unrecognized connection import format", "connShare.error.unrecognizedFormat"),
        ("No connection files to import", "connShare.error.noFiles"),
        ("Decryption failed: wrong password", "connShare.error.decryptionWrongPassword"),
        ("Invalid import file: missing 'connections' field", "connShare.error.missingConnectionsField"),
        ("missing connections", "connShare.error.missingConnections"),
        ("DBX decryption failed: wrong passphrase", "connShare.error.dbxWrongPassphrase"),
        ("DBX import: missing connections array", "connShare.error.dbxMissingConnections"),
    ];
    for (raw, key) in exact {
        assert_eq!(translate_connection_share_error(raw, fallback), t(key), "exact: {raw}");
        // Trim is applied before lookup, matching React's `const trimmed`.
        assert_eq!(translate_connection_share_error(&format!("  {raw}  "), fallback), t(key), "trimmed exact: {raw}");
    }

    // Every FORMAT_LABEL drives each parameterised branch; a non-default label
    // (DataGrip) falls through to the raw string, exactly as React does.
    for format in ["DataZen", "TablePlus", "Encrypted connection"] {
        let cases = [
            (format!("Password is required for {format} import"), "connShare.error.formatPasswordRequired"),
            (format!("Invalid {format} file: too short"), "connShare.error.invalidFileTooShort"),
            (format!("Invalid {format} file: expected password-based encryption"), "connShare.error.invalidFileEncryptionType"),
            (format!("Invalid {format} file: unsupported RNCryptor version 3"), "connShare.error.invalidFileRncryptorVersion"),
            (format!("{format} decryption failed: wrong password or corrupt file"), "connShare.error.decryptionFailed"),
            (format!("Invalid {format} JSON after decrypt: unexpected token"), "connShare.error.invalidJsonAfterDecrypt"),
            (format!("{format} payload is not UTF-8: ff fe"), "connShare.error.payloadNotUtf8"),
        ];
        for (raw, key) in cases {
            assert_eq!(translate_connection_share_error(&raw, fallback), t(key).replace("{format}", format), "{format}: {raw}");
        }
        // Legacy hint wins over the plain decryption-failed branch, but only
        // for DataZen: FORMAT_LABELS is ordered DataZen-first, so a TablePlus
        // message containing "(Legacy DataZen)" resolves format=DataZen and
        // falls through verbatim — exactly as React's `find` does.
        if format == "DataZen" {
            let legacy = format!("{format} decryption failed: wrong password or corrupt file (Legacy DataZen)");
            assert_eq!(translate_connection_share_error(&legacy, fallback), t("connShare.error.decryptionFailedLegacyHint").replace("{format}", format));
        } else {
            let legacy = format!("{format} decryption failed: wrong password or corrupt file (Legacy DataZen)");
            assert_eq!(translate_connection_share_error(&legacy, fallback), legacy);
        }
    }
    assert_eq!(translate_connection_share_error("Invalid DataGrip file: too short", fallback), "Invalid DataGrip file: too short");
    assert_eq!(translate_connection_share_error("Failed to read anything", fallback), t("connShare.error.readFailed"));
    assert_eq!(translate_connection_share_error("Unknown import source: x", fallback), t("connShare.error.unknownImportSource"));
    assert_eq!(translate_connection_share_error("   ", fallback), fallback);
    assert_eq!(translate_connection_share_error("brand new text", fallback), "brand new text");
    let _ = (fallback, crate::components::connection::connection_share_dialog::utc_date_ymd());
}

#[test]
fn stale_epochs_are_inert_for_every_host_event_kind() {
    let fallback = t("common.importFailed");
    let base = ShareState::new(ShareMode::Import, ImportSource::File);

    let events = [
        ShareEvent::Exported { epoch: base.epoch.wrapping_sub(1), result: Ok(99) },
        ShareEvent::FilePicked { epoch: base.epoch.wrapping_sub(1), result: Ok(Some("/x".into())) },
        ShareEvent::PathDetected { epoch: base.epoch.wrapping_sub(1), path: "/x".into(), found: true },
        ShareEvent::PathPicked { epoch: base.epoch.wrapping_sub(1), result: Ok("/x".into()) },
        ShareEvent::Imported { epoch: base.epoch.wrapping_sub(1), result: Ok(ImportResult::default()) },
    ];
    for event in events {
        let mut state = base.clone();
        let label = format!("{event:?}");
        assert!(state.apply(event, fallback).is_empty(), "stale {label} yields no output");
        assert_eq!(state.submitting, base.submitting, "stale {label} does not reset submitting");
        assert_eq!(state.import.selected_file, None, "stale {label} does not set a file");
        assert_eq!(state.import.local_error, String::new(), "stale {label} does not set an error");
    }
}

#[test]
fn import_footer_disable_tracks_busy_and_detecting() {
    let mut scene = AuditScene::new(ShareMode::Import, ImportSource::File);
    scene.state.import.detecting = true;
    let rt = runtime(scene, (960.0, 640.0));
    // React: cancel disabled = submitting || imp.submitting; primary = submitting || imp.busy.
    assert!(element(&rt, &format!("{ROOT}-cancel")).focusable, "detecting alone leaves cancel enabled");
    assert!(!element(&rt, &format!("{ROOT}-primary")).focusable, "detecting disables the primary (busy)");

    let mut scene = AuditScene::new(ShareMode::Import, ImportSource::Dbx);
    scene.state.import.submitting = true;
    let rt = runtime(scene, (960.0, 640.0));
    assert!(!element(&rt, &format!("{ROOT}-cancel")).focusable, "submitting disables cancel");
    assert!(!element(&rt, &format!("{ROOT}-primary")).focusable, "submitting disables primary");
}

/// REPRO D1 — React `ConnectionImportFields` mounts the password block only
/// when `appImport || selectedFile !== null`. With a bare file source and no
/// file chosen, the password field must be absent.
#[test]
fn import_file_mode_without_a_chosen_file_has_no_password_field() {
    let rt = runtime(AuditScene::new(ShareMode::Import, ImportSource::File), (960.0, 640.0));
    assert!(
        !has_key(&rt, "import-password-input"),
        "React hides the password field until an app import or a chosen file"
    );
}

/// REPRO D2 — React `handleExportSubmit`'s catch calls `onError(...)` only; it
/// never writes `localError`, so no inline banner appears after a backend
/// export failure. The host callback carries the mapped message.
#[test]
fn export_backend_failure_does_not_render_an_inline_error_banner() {
    let mut state = ShareState::new(ShareMode::Export, ImportSource::File);
    let outputs = state.apply(
        ShareEvent::Exported { epoch: state.epoch, result: Err("No connection files to import".into()) },
        t("common.exportFailed"),
    );
    assert_eq!(outputs, vec![ShareOutput::Error(t("connShare.error.noFiles").to_owned())], "host callback gets the mapped message");
    assert_eq!(state.local_error, String::new(), "React only sets localError for validation guards");

    let mut scene = AuditScene::new(ShareMode::Export, ImportSource::File);
    scene.state = state;
    scene.state.open = true;
    let rt = runtime(scene, (960.0, 640.0));
    assert!(!has_key(&rt, &format!("{ROOT}-password-error")), "no inline banner after a backend export error");
}

#[test]
fn dialog_widths_are_independently_pinned_to_react_tailwind_points() {
    // The coder's width test derives `expected_width` from WIDTH_IMPORT_APP /
    // WIDTH_EXPORT themselves, so it stays green when a constant is mutated.
    // These are the literal Tailwind values: sm=24rem=384, md=28rem=448,
    // lg=32rem=512 (1rem = 16pt).
    let cases = [
        (ShareMode::Export, ImportSource::File, 384.0f32),
        (ShareMode::Import, ImportSource::File, 448.0),
        (ShareMode::Import, ImportSource::Dbx, 512.0),
        (ShareMode::Import, ImportSource::Navicat, 512.0),
        (ShareMode::Import, ImportSource::DataGrip, 512.0),
        (ShareMode::Import, ImportSource::Dbeaver, 512.0),
        (ShareMode::Import, ImportSource::TablePlus, 512.0),
    ];
    for (mode, source, expected) in cases {
        let rt = runtime(AuditScene::new(mode, source), (1440.0, 816.0));
        let width = element(&rt, ROOT).rect[2];
        assert!((width - expected).abs() <= 1.5, "{mode:?}/{source:?}: width {width} != literal {expected}");
    }
}

#[test]
fn adversarial_geometry_holds_at_both_sizes_in_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut export = AuditScene::new(ShareMode::Export, ImportSource::File);
            export.dark = dark;
            export.state.local_error = t("connShare.passwordMismatch").to_owned();
            export.state.password = rune::TextEdit::new("x".repeat(240));
            let rt = runtime(export, (w, h));
            assert_inside(&rt, w, h, &format!("export dark={dark} {w}x{h}"));

            let mut import = AuditScene::new(ShareMode::Import, ImportSource::Navicat);
            import.dark = dark;
            import.state.import.path_found = true;
            import.state.import.data_path = rune::TextEdit::new(format!("/Applications/{}", "LongAppName ".repeat(30)));
            import.state.import.local_error = t("connShare.error.decryptionFailed").replace("{format}", "DataZen");
            let rt = runtime(import, (w, h));
            assert_inside(&rt, w, h, &format!("import dark={dark} {w}x{h}"));
        }
    }
}
