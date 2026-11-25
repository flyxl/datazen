//! `platform/contracts.ts` (T129) ported to Rune — the platform port
//! contracts (form-split plan §3.3).
//!
//! Shared code sees platforms only through these narrow ports plus
//! [`configure_platform`]; a concrete platform entity never leaks into
//! shared code (gate R7) — the implementations are host-owned and bound
//! here at bootstrap time by the assembly root.
//!
//! ## What this slice ports (the task is a split)
//!
//! The React file is 408 lines of interfaces plus a small binding
//! registry and one inert implementation. This first slice ports what
//! the Rust tree can consume today:
//!
//! * the **updater wire types** ([`UpdateProgress`],
//!   [`UpdateCheckResult`]) — shared by the host port and the updater
//!   shim, with the exact TS phase/status literals;
//! * the **window-routing contracts** ([`OpenWindowOptions`],
//!   [`WindowCloseEvent`]) — the host window router's call surface;
//! * the **tables exporter port** ([`TablesExporterPort`]) over the
//!   existing request mirror ([`TablesExportRequest`],
//!   `export_plan.rs`, pending T298's real port) and outcome shape
//!   ([`ExportTablesOutcome`], whose `Cancelled` arm has been waiting
//!   here since T043-S2);
//! * the **binding registry** ([`configure_platform`] /
//!   [`clear_platform`] / [`is_platform_configured`] /
//!   [`require_tables_exporter`] / [`require_capabilities`]) with the
//!   §7.4 fail-loud semantics: an unbound port that a user action needs
//!   is a wiring gap worth a locatable error, never a silent degrade.
//!
//! ## What this slice deliberately defers (with their owners)
//!
//! * `HostSettingsPort` / `DesktopHostServices` /
//!   `inertHostSettingsPort()` / `requireHostSettings()` — every method
//!   of that pair reads or writes `AppSettings` (`src/types/index.ts`),
//!   which has no Rust mirror yet; the trait lands with the settings
//!   track that owns the mirror (the settings surface's own task), and
//!   the registry then grows the `host_settings` slot with the same
//!   inert-default semantics React pins (deliberately NOT a throw:
//!   `settingsCommands` calls it unconditionally).
//! * `PlatformServices` (`@datazen/backend-client` §7.1), the
//!   `WindowRouter`'s async halves, and the `EventChannel` — the
//!   backend-client facade and the cross-window bus are their own
//!   migration tasks (T437 for `lib/crossWindowBus.ts`); Rune's native
//!   host today reaches the same abilities through the
//!   `DesktopAdapter` injection (`app_with_adapter`), which is the
//!   assembly root this registry's `services`/`windows`/`events` slots
//!   grow onto when those ports land.
//! * `PlatformCapabilities` is already ported (T091,
//!   [`super::capabilities`]); the registry carries it by reference.
//!
//! ## Deliberate differences from React
//!
//! * **The registry is thread-local, not module-global.** React keeps one
//!   module-level `bindings` variable; a native Rune app is
//!   single-threaded and immediate-mode, so the binding lives in a
//!   `thread_local` slot that [`configure_platform`] replaces (tests
//!   rebind) and [`clear_platform`] drops — the same lifecycle, scoped to
//!   the UI thread that owns it.
//! * **The exporter port is sync, not `Promise`-returning.** The async
//!   half of `exportTables` is the host's streaming run; the port's
//!   method signature stays the shared seam (the same request and the
//!   same result shape), and the host drives the duration.
//! * **`onExportProgress` takes a callback slot, not an unlisten
//!   function.** React's subscription returns an unlisten closure; the
//!   Rust port sets/clears the handler by `Option`, which is the
//!   immediate-mode equivalent of register/unlisten.
//! * **The optional port arms are default methods.** `cancelExportTables`
//!   defaults to "nothing to cancel" (`Ok(false)`) and the progress
//!   subscription defaults to no handler, exactly the forms React says a
//!   binding may simply not offer.

use std::cell::RefCell;
use std::rc::Rc;

use crate::platform::blob_tables_exporter::ExportTablesOutcome;
use crate::platform::capabilities::PlatformCapabilities;
use crate::platform::export_plan::TablesExportRequest;

// ---------------------------------------------------------------------------
// Updater wire types (form-split F6 prep — shared by the port and
// lib/updater).
// ---------------------------------------------------------------------------

/// React `UpdateProgress` (`contracts.ts:118-124`): the progress of a
/// running update check / download / install (the plugin-updater's own
/// shape).
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // the settings Update section (its consumer) lands with the settings track
pub enum UpdateProgress {
    /// `{ phase: 'checking' }`.
    Checking,
    /// `{ phase: 'downloading'; downloaded; total | null }`.
    Downloading {
        downloaded: u64,
        total: Option<u64>,
    },
    /// `{ phase: 'installing' }`.
    Installing,
    /// `{ phase: 'done'; version }`.
    Done { version: String },
    /// `{ phase: 'idle' }`.
    Idle,
}

impl UpdateProgress {
    /// The TS `phase` literal, verbatim.
    #[allow(dead_code)] // the settings Update section (its consumer) lands with the settings track
    pub fn phase(&self) -> &'static str {
        match self {
            Self::Checking => "checking",
            Self::Downloading { .. } => "downloading",
            Self::Installing => "installing",
            Self::Done { .. } => "done",
            Self::Idle => "idle",
        }
    }
}

/// React `UpdateCheckResult` (`contracts.ts:126-132`): the outcome of an
/// update check / download-and-install run.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // the settings Update section (its consumer) lands with the settings track
pub enum UpdateCheckResult {
    /// `{ status: 'upToDate' }`.
    UpToDate,
    /// `{ status: 'available'; version }`.
    Available { version: String },
    /// `{ status: 'installed'; version }`.
    Installed { version: String },
    /// `{ status: 'error'; message }`.
    Error { message: String },
}

impl UpdateCheckResult {
    /// The TS `status` literal, verbatim.
    #[allow(dead_code)] // the settings Update section (its consumer) lands with the settings track
    pub fn status(&self) -> &'static str {
        match self {
            Self::UpToDate => "upToDate",
            Self::Available { .. } => "available",
            Self::Installed { .. } => "installed",
            Self::Error { .. } => "error",
        }
    }
}

// ---------------------------------------------------------------------------
// Port 2: window routing (the types half; the router itself is the host's).
// ---------------------------------------------------------------------------

/// React `OpenWindowOptions` (`contracts.ts:29-37`): what the router
/// needs to open a singleton or multi-instance window.
#[derive(Clone, Debug, Default, PartialEq)]
#[allow(dead_code)] // the host window router binds these when its port lands
pub struct OpenWindowOptions {
    /// `params?: Record<string, string>` — kept as ordered pairs, the
    /// only shape a Rust host can hand to a native window's URL/router
    /// without inventing a map order React never promised.
    pub params: Vec<(String, String)>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub min_width: Option<f32>,
    pub min_height: Option<f32>,
    pub title: Option<String>,
    pub center: bool,
}

/// React `WindowCloseEvent` (`contracts.ts:41-43`): delivered to
/// `onCloseRequested` handlers so they can veto the close. The flag is
/// the whole event — React's object carries nothing else.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // the host window router binds these when its port lands
pub struct WindowCloseEvent {
    default_prevented: bool,
}

impl WindowCloseEvent {
    /// A fresh event nobody has vetoed.
    pub fn new() -> Self {
        Self::default()
    }

    /// React's `preventDefault()`: veto the close.
    pub fn prevent_default(&mut self) {
        self.default_prevented = true;
    }

    /// Whether any handler vetoed the close.
    pub fn default_prevented(&self) -> bool {
        self.default_prevented
    }
}

// ---------------------------------------------------------------------------
// Port 6: the tables exporter (form-split F4 dual-flow demonstration).
// ---------------------------------------------------------------------------

/// React `ExportProgressEvent` (`commands/file.ts:127-130`, re-exported
/// by contracts): writer progress while streaming a table to disk — one
/// row count per table. The real `commands/file.ts` port is T298
/// (segment 4, pending); this local mirror is the same labelled
/// placeholder the `export_plan` request mirror is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TablesExportProgressEvent {
    pub table: String,
    pub rows_written: u64,
}

/// React `TablesExporterPort` (`contracts.ts:312-332`): the shared export
/// signature — the desktop implementation streams through the
/// save-dialog token writer (the path never returns to JS); a web
/// implementation assembles the bytes and Blob-downloads them. Both
/// answer the same seam, so the export UI never learns which form factor
/// it is in.
///
/// The two optional members are default methods: a form without a Rust
/// run to cancel simply has no cancellation to offer (the SQL dialogs
/// hide the cancel button there), and a form without a streaming writer
/// has no progress to report.
#[allow(dead_code)] // the export dialogs (their consumer) wire this port
pub trait TablesExporterPort {
    /// `exportTables(request)` — `Promise<ExportTablesResult | null>` in
    /// React; the host drives the duration, the seam stays.
    fn export_tables(&mut self, request: &TablesExportRequest) -> Result<ExportTablesOutcome, String>;

    /// `cancelExportTables?(token)` — optional; `Ok(false)` ("nothing to
    /// cancel on this form") unless the binding offers cancellation.
    fn cancel_export_tables(&mut self, _token: &str) -> Result<bool, String> {
        Ok(false)
    }

    /// `onExportProgress?(handler)` — optional; the immediate-mode
    /// equivalent of the subscribe/unlisten pair: set the handler, or
    /// clear it with `None`. Returns whether the form offers streaming
    /// progress at all.
    fn set_on_export_progress(
        &mut self,
        _handler: Option<Rc<dyn Fn(&TablesExportProgressEvent)>>,
    ) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Injection (bootstrap, registry style — no constructor threading).
// ---------------------------------------------------------------------------

/// React `PlatformBindings` (`contracts.ts:344-361`) as this slice can
/// bind it: the exporter port and the capability table. The `services`
/// / `windows` / `events` slots grow onto the registry when those ports
/// land (see the module docs); the host-settings slot lands with the
/// settings track that owns the `AppSettings` mirror.
#[derive(Clone)]
pub struct PlatformBindings {
    /// The dual-flow export demonstration port (form-split F4, plan
    /// §3.3).
    pub tables_exporter: Option<Rc<RefCell<dyn TablesExporterPort>>>,
    /// The static capability table; `None` keeps
    /// `capabilitiesUnavailable()` semantics (F5 made it required in
    /// React; a binding without it fails loudly at
    /// [`require_capabilities`]).
    pub capabilities: Option<&'static PlatformCapabilities>,
}

thread_local! {
    /// React's module-level `bindings` variable, scoped to the UI thread
    /// that owns the platform (single-threaded immediate mode). A second
    /// [`configure_platform`] replaces the binding (tests rebind);
    /// [`clear_platform`] drops it.
    static BINDINGS: RefCell<Option<PlatformBindings>> = const { RefCell::new(None) };
}

/// React `configurePlatform` (`contracts.ts:364-366`): install the
/// platform ports — the app assembly root, before the first frame.
/// A second call replaces the binding (tests rebind).
pub fn configure_platform(next: PlatformBindings) {
    BINDINGS.with(|slot| *slot.borrow_mut() = Some(next));
}

/// React `clearPlatform` (`contracts.ts:369-370`): drop the binding, so
/// tests can exercise the unconfigured path.
pub fn clear_platform() {
    BINDINGS.with(|slot| *slot.borrow_mut() = None);
}

/// React `isPlatformConfigured` (`contracts.ts:372-374`).
pub fn is_platform_configured() -> bool {
    BINDINGS.with(|slot| slot.borrow().is_some())
}

/// React `requirePlatform` (`contracts.ts:379-391`): resolve the bindings
/// or fail loudly (§7.4) — an unconfigured call must produce a locatable
/// error, never silently degrade into a direct host connection.
pub fn require_platform() -> Result<PlatformBindings, String> {
    BINDINGS.with(|slot| {
        slot.borrow()
            .clone()
            .ok_or_else(|| PLATFORM_NOT_CONFIGURED.to_owned())
    })
}

/// The §7.4 error text, verbatim from React.
pub const PLATFORM_NOT_CONFIGURED: &str = "Platform ports have not been configured; check that the app assembly root ran configurePlatform().";

/// React `requireTablesExporter` (`contracts.ts:397-409`): the export
/// port, or an explicit failure when no form bound one. Export is a
/// user-triggered action, not an ambient read: an unbound exporter is a
/// wiring gap worth failing loudly rather than degrading into a silently
/// missing download.
pub fn require_tables_exporter() -> Result<Rc<RefCell<dyn TablesExporterPort>>, String> {
    let bindings = require_platform()?;
    bindings
        .tables_exporter
        .ok_or_else(|| NO_TABLES_EXPORTER.to_owned())
}

/// The unbound-exporter error text, verbatim from React.
pub const NO_TABLES_EXPORTER: &str = "No tables exporter has been bound; check that the app assembly root ran configurePlatform().";

/// React `requireCapabilities` (`contracts.ts:439-450`): the capability
/// table, or the §7.4 failure while it is missing. Callers that must
/// degrade before the table lands use `getPlatformOrNull` /
/// [`is_platform_configured`] instead.
pub fn require_capabilities() -> Result<&'static PlatformCapabilities, String> {
    require_platform()?
        .capabilities
        .ok_or_else(|| CAPABILITIES_NOT_PROVIDED.to_owned())
}

/// The missing-capability-table error text, verbatim from React.
pub const CAPABILITIES_NOT_PROVIDED: &str = "Platform capabilities have not been provided yet (form-split F5); the consuming call site must not be reachable in this build.";
