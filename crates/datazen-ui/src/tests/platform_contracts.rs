//! T129: the platform port contracts — the updater wire types, the
//! window-routing contracts, the exporter port, and the binding registry's
//! §7.4 semantics, probed as pure values and thread-local registry state.
//!
//! Every assertion observes the typed values and the resolver outcomes
//! (including the exact error texts React pins), never a value the module
//! reports about itself.

use std::cell::RefCell;
use std::rc::Rc;

use crate::platform::blob_tables_exporter::ExportTablesOutcome;
use crate::platform::capabilities::{PlatformCapabilities, DESKTOP_CAPABILITIES};
use crate::platform::contracts::{
    clear_platform, configure_platform, is_platform_configured, require_capabilities,
    require_platform, require_tables_exporter, PlatformBindings, TablesExportProgressEvent,
    TablesExporterPort, UpdateCheckResult, UpdateProgress, WindowCloseEvent, CAPABILITIES_NOT_PROVIDED,
    NO_TABLES_EXPORTER, PLATFORM_NOT_CONFIGURED,
};
use crate::platform::export_plan::TablesExportRequest;

/// A recording exporter: answers from a script and counts the calls.
struct ProbeExporter {
    outcome: Result<ExportTablesOutcome, String>,
    export_calls: usize,
    cancel_calls: usize,
    cancel_answer: bool,
    progress_offered: bool,
}

impl TablesExporterPort for ProbeExporter {
    fn export_tables(&mut self, request: &TablesExportRequest) -> Result<ExportTablesOutcome, String> {
        let _ = request;
        self.export_calls += 1;
        self.outcome.clone()
    }

    fn cancel_export_tables(&mut self, _token: &str) -> Result<bool, String> {
        self.cancel_calls += 1;
        Ok(self.cancel_answer)
    }

    fn set_on_export_progress(
        &mut self,
        _handler: Option<Rc<dyn Fn(&TablesExportProgressEvent)>>,
    ) -> bool {
        self.progress_offered
    }
}

fn exporter(outcome: Result<ExportTablesOutcome, String>) -> Rc<RefCell<dyn TablesExporterPort>> {
    Rc::new(RefCell::new(ProbeExporter {
        outcome,
        export_calls: 0,
        cancel_calls: 0,
        cancel_answer: false,
        progress_offered: false,
    }))
}

// -----------------------------------------------------------------------
// The updater wire types.
// -----------------------------------------------------------------------

#[test]
fn the_update_phases_carry_the_ts_literals() {
    assert_eq!(UpdateProgress::Checking.phase(), "checking");
    assert_eq!(
        UpdateProgress::Downloading {
            downloaded: 10,
            total: Some(100)
        }
        .phase(),
        "downloading"
    );
    assert!(matches!(
        UpdateProgress::Downloading {
            downloaded: 10,
            total: None
        },
        UpdateProgress::Downloading { total: None, .. }
    ));
    assert_eq!(UpdateProgress::Installing.phase(), "installing");
    assert_eq!(UpdateProgress::Done { version: "1.2.3".to_owned() }.phase(), "done");
    assert_eq!(UpdateProgress::Idle.phase(), "idle");
}

#[test]
fn the_update_results_carry_the_ts_literals() {
    assert_eq!(UpdateCheckResult::UpToDate.status(), "upToDate");
    assert_eq!(
        UpdateCheckResult::Available { version: "9.0.0".to_owned() }.status(),
        "available"
    );
    assert_eq!(
        UpdateCheckResult::Installed { version: "9.0.0".to_owned() }.status(),
        "installed"
    );
    assert_eq!(
        UpdateCheckResult::Error { message: "net".to_owned() }.status(),
        "error"
    );
    // The variants carry their payloads.
    assert_eq!(
        UpdateCheckResult::Installed { version: "9.0.0".to_owned() },
        UpdateCheckResult::Installed { version: "9.0.0".to_owned() }
    );
}

// -----------------------------------------------------------------------
// The window-routing contracts.
// -----------------------------------------------------------------------

#[test]
fn a_close_event_starts_unvetoed_and_one_veto_sticks() {
    let mut event = WindowCloseEvent::new();
    assert!(!event.default_prevented(), "nobody has vetoed yet");
    event.prevent_default();
    assert!(event.default_prevented(), "the handler vetoed the close");
    // Idempotent: a second call is the same veto.
    event.prevent_default();
    assert!(event.default_prevented());
}

#[test]
fn open_window_options_default_to_the_untouched_router_surface() {
    let options = crate::platform::contracts::OpenWindowOptions::default();
    assert!(options.params.is_empty());
    assert_eq!(options.width, None);
    assert_eq!(options.height, None);
    assert_eq!(options.min_width, None);
    assert_eq!(options.min_height, None);
    assert_eq!(options.title, None);
    assert!(!options.center, "React's center?: boolean defaults falsy");

    let mut options = crate::platform::contracts::OpenWindowOptions::default();
    options.params.push(("target".to_owned(), "sync".to_owned()));
    options.width = Some(960.0);
    options.height = Some(640.0);
    options.title = Some("Data Sync".to_owned());
    options.center = true;
    assert_eq!(options.params, vec![("target".to_owned(), "sync".to_owned())]);
    assert_eq!(options.width, Some(960.0));
}

// -----------------------------------------------------------------------
// The exporter port.
// ---------------------------------------------------------------------------

#[test]
fn the_exporter_port_answers_the_seam_and_the_optional_arms() {
    let port = exporter(Ok(ExportTablesOutcome::Saved(3)));
    let mut bound = port.borrow_mut();
    let request = TablesExportRequest::default();
    let outcome = bound.export_tables(&request);
    assert_eq!(outcome, Ok(ExportTablesOutcome::Saved(3)));

    // The optional arms default: no cancellation, no progress.
    assert_eq!(bound.cancel_export_tables("token-1"), Ok(false));
    assert!(!bound.set_on_export_progress(None));
}

#[test]
fn a_failing_exporter_passes_its_error_through_the_seam() {
    let port = exporter(Err("driver refused".to_owned()));
    let mut bound = port.borrow_mut();
    assert_eq!(
        bound.export_tables(&TablesExportRequest::default()),
        Err("driver refused".to_owned())
    );
}

// -----------------------------------------------------------------------
// The binding registry (§7.4 semantics).
// -----------------------------------------------------------------------

#[test]
fn an_unbound_platform_fails_loudly_with_the_react_texts() {
    clear_platform();
    assert!(!is_platform_configured());
    assert_eq!(
        require_platform().map(|_| ()).unwrap_err(),
        PLATFORM_NOT_CONFIGURED,
        "the unconfigured error is the React text, verbatim"
    );
    assert_eq!(require_tables_exporter().map(|_| ()).unwrap_err(), PLATFORM_NOT_CONFIGURED);
    assert_eq!(require_capabilities().map(|_| ()).unwrap_err(), PLATFORM_NOT_CONFIGURED);
}

#[test]
fn configure_binds_and_clear_rebinds() {
    clear_platform();
    configure_platform(PlatformBindings {
        tables_exporter: Some(exporter(Ok(ExportTablesOutcome::Saved(1)))),
        capabilities: None,
    });
    assert!(is_platform_configured());
    // A binding without a capability table fails loudly at the resolver,
    // not at the read.
    assert_eq!(require_capabilities().map(|_| ()).unwrap_err(), CAPABILITIES_NOT_PROVIDED);
    // A binding without an exporter fails at the exporter resolver with
    // the unbound-exporter text.
    configure_platform(PlatformBindings {
        tables_exporter: None,
        capabilities: Some(&DESKTOP_CAPABILITIES),
    });
    assert_eq!(require_tables_exporter().map(|_| ()).unwrap_err(), NO_TABLES_EXPORTER);
    // With both bound, the resolvers answer.
    configure_platform(PlatformBindings {
        tables_exporter: Some(exporter(Ok(ExportTablesOutcome::Cancelled))),
        capabilities: Some(&DESKTOP_CAPABILITIES),
    });
    let caps = require_capabilities().expect("the capability table is bound");
    assert_eq!(caps.updater, DESKTOP_CAPABILITIES.updater);
    let port = require_tables_exporter().expect("the exporter is bound");
    assert_eq!(
        port.borrow_mut().export_tables(&TablesExportRequest::default()),
        Ok(ExportTablesOutcome::Cancelled)
    );
    clear_platform();
    assert!(!is_platform_configured(), "clear drops the binding");
}

#[test]
fn a_second_configure_replaces_the_binding() {
    clear_platform();
    configure_platform(PlatformBindings {
        tables_exporter: Some(exporter(Ok(ExportTablesOutcome::Saved(1)))),
        capabilities: Some(&DESKTOP_CAPABILITIES),
    });
    let first = require_tables_exporter().expect("the first exporter");
    assert_eq!(
        first.borrow_mut().export_tables(&TablesExportRequest::default()),
        Ok(ExportTablesOutcome::Saved(1))
    );
    // Rebind: the registry now answers with the second exporter.
    configure_platform(PlatformBindings {
        tables_exporter: Some(exporter(Ok(ExportTablesOutcome::Saved(2)))),
        capabilities: Some(&DESKTOP_CAPABILITIES),
    });
    let second = require_tables_exporter().expect("the second exporter");
    assert_eq!(
        second.borrow_mut().export_tables(&TablesExportRequest::default()),
        Ok(ExportTablesOutcome::Saved(2)),
        "tests rebind"
    );
    clear_platform();
}

#[test]
fn the_capability_table_answers_through_the_registry() {
    clear_platform();
    configure_platform(PlatformBindings {
        tables_exporter: None,
        capabilities: Some(&DESKTOP_CAPABILITIES),
    });
    let caps: &PlatformCapabilities = require_capabilities().expect("bound");
    assert_eq!(caps.version_source, DESKTOP_CAPABILITIES.version_source);
    assert_eq!(caps.settings_sections.len(), DESKTOP_CAPABILITIES.settings_sections.len());
    clear_platform();
}
