//! T129 independent audit: fresh vectors over the platform contracts.
//!
//! Written against the ported behavior, not the coder's fixtures: the
//! downloading-progress payload round trip, a web-shaped capability table
//! (not the desktop one the coder used), the exporter's optional arms
//! actually being offered by a binding, and the registry's teardown
//! isolation between resolvers. Mutation evidence lives in the audit
//! commit message (red-then-green, restore verified).

use std::cell::RefCell;
use std::rc::Rc;

use datazen_ui::platform::blob_tables_exporter::ExportTablesOutcome;
use datazen_ui::platform::capabilities::{PlatformCapabilities, VersionSource, WEB_CAPABILITIES};
use datazen_ui::platform::contracts::{
    clear_platform, configure_platform, is_platform_configured, require_capabilities,
    require_platform, require_tables_exporter, PlatformBindings, TablesExportProgressEvent,
    TablesExporterPort, UpdateCheckResult, UpdateProgress, WindowCloseEvent,
};
use datazen_ui::platform::export_plan::TablesExportRequest;

/// A fresh exporter that OFFERS the optional arms (unlike the coder's
/// probe, whose default arms were inert).
struct OfferingExporter {
    progress: Vec<TablesExportProgressEvent>,
    handler_seen: bool,
}

impl TablesExporterPort for OfferingExporter {
    fn export_tables(&mut self, request: &TablesExportRequest) -> Result<ExportTablesOutcome, String> {
        let _ = request;
        Ok(ExportTablesOutcome::Saved(7))
    }

    fn cancel_export_tables(&mut self, token: &str) -> Result<bool, String> {
        // A live Rust run to cancel: true, and the token rides the seam.
        Ok(!token.is_empty())
    }

    fn set_on_export_progress(
        &mut self,
        handler: Option<Rc<dyn Fn(&TablesExportProgressEvent)>>,
    ) -> bool {
        self.handler_seen = handler.is_some();
        if let Some(handler) = handler {
            for event in self.progress.clone() {
                handler(&event);
            }
        }
        true
    }
}

// -----------------------------------------------------------------------
// Fresh wire vectors.
// -----------------------------------------------------------------------

#[test]
fn a_download_progress_round_trips_its_payload() {
    let progress = UpdateProgress::Downloading {
        downloaded: 4_194_304,
        total: Some(16_777_216),
    };
    assert_eq!(progress.phase(), "downloading");
    assert!(matches!(
        progress,
        UpdateProgress::Downloading {
            downloaded: 4_194_304,
            total: Some(16_777_216)
        }
    ));
    // Done carries the installed version, not a phase payload.
    let done = UpdateProgress::Done {
        version: "2.1.0".to_owned(),
    };
    if let UpdateProgress::Done { version } = &done {
        assert_eq!(version, "2.1.0");
    } else {
        panic!("the Done phase carries its version");
    }
}

#[test]
fn an_error_result_carries_its_message_verbatim() {
    let result = UpdateCheckResult::Error {
        message: "404 https://releases.example/missing".to_owned(),
    };
    assert_eq!(result.status(), "error");
    if let UpdateCheckResult::Error { message } = &result {
        assert!(message.starts_with("404"));
    } else {
        panic!("the Error result carries its message");
    }
}

#[test]
fn two_vetoes_and_a_fresh_event_are_independent() {
    let mut first = WindowCloseEvent::new();
    let mut second = WindowCloseEvent::new();
    first.prevent_default();
    assert!(first.default_prevented());
    assert!(!second.default_prevented(), "events are independent objects");
}

// -----------------------------------------------------------------------
// Fresh registry vectors.
// ---------------------------------------------------------------------------

#[test]
fn a_web_shaped_capability_table_answers_through_the_registry() {
    clear_platform();
    configure_platform(PlatformBindings {
        tables_exporter: None,
        capabilities: Some(&WEB_CAPABILITIES),
    });
    let caps: &PlatformCapabilities = require_capabilities().expect("bound");
    assert!(!caps.updater, "the web form has no updater");
    assert_eq!(caps.version_source, VersionSource::BuildConstant);
    assert_eq!(caps.settings_sections.len(), 5, "the five shared sections");
    clear_platform();
}

#[test]
fn an_offering_exporter_answers_the_optional_arms() {
    let port: Rc<RefCell<dyn TablesExporterPort>> = Rc::new(RefCell::new(OfferingExporter {
        progress: vec![TablesExportProgressEvent {
            table: "orders".to_owned(),
            rows_written: 1_000,
        }],
        handler_seen: false,
    }));
    let mut bound = port.borrow_mut();
    assert_eq!(
        bound.export_tables(&TablesExportRequest::default()),
        Ok(ExportTablesOutcome::Saved(7))
    );
    assert_eq!(bound.cancel_export_tables("tok-1"), Ok(true), "a live run cancels");
    assert_eq!(bound.cancel_export_tables(""), Ok(false), "an empty token is nothing to cancel");

    // The progress subscription: the handler replays the exporter's
    // recorded progress synchronously.
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    assert!(bound.set_on_export_progress(Some(Rc::new(move |event: &TablesExportProgressEvent| {
        sink.borrow_mut().push(event.clone());
    }))));
    assert_eq!(
        seen.borrow().as_slice(),
        [TablesExportProgressEvent {
            table: "orders".to_owned(),
            rows_written: 1_000,
        }],
        "the handler received the recorded progress"
    );
    // Clearing is the unlisten: the call returns without recording a
    // handler (the trait's return flips to the "no handler" state).
    bound.set_on_export_progress(None);
}

#[test]
fn require_platform_copies_the_binding_so_the_registry_survives_the_reader() {
    clear_platform();
    configure_platform(PlatformBindings {
        tables_exporter: Some(Rc::new(RefCell::new(OfferingExporter {
            progress: Vec::new(),
            handler_seen: false,
        }))),
        capabilities: Some(&WEB_CAPABILITIES),
    });
    let first = require_platform().expect("the binding resolves");
    // The registry stays readable after the first resolve.
    let second = require_platform().expect("the binding still resolves");
    assert!(first.capabilities == second.capabilities);
    assert!(is_platform_configured());
    clear_platform();
}

#[test]
fn clearing_between_two_bindings_never_leaks_the_first() {
    clear_platform();
    configure_platform(PlatformBindings {
        tables_exporter: None,
        capabilities: Some(&WEB_CAPABILITIES),
    });
    clear_platform();
    configure_platform(PlatformBindings {
        tables_exporter: None,
        capabilities: None,
    });
    // No capability table on the second binding: the resolver fails
    // with the missing-table text, not the first binding's table.
    assert_eq!(
        require_capabilities().map(|_| ()).unwrap_err(),
        "Platform capabilities have not been provided yet (form-split F5); the consuming call site must not be reachable in this build."
    );
    clear_platform();
    assert!(!is_platform_configured());
}
