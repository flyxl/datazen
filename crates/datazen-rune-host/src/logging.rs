//! Native GUI process-start console diagnostics. The persisted setting takes
//! effect at the next launch; developers can override it with RUST_LOG.
//! Only explicitly reviewed lifecycle messages reach stderr. This is not a
//! file log, telemetry, or a switch for raw SQL/driver traces.

use std::path::Path;

use datazen_ui_api::types::LogLevel;
use tracing::Subscriber;
use tracing_subscriber::{filter, fmt, fmt::MakeWriter, prelude::*, EnvFilter};

const TARGET: &str = "datazen_native_host";

fn subscriber<W>(level: LogLevel, rust_log: Option<&str>, writer: W) -> impl Subscriber + Send + Sync
where
    W: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    let severity = rust_log
        .filter(|directives| !directives.trim().is_empty())
        .and_then(|directives| EnvFilter::try_new(directives).ok())
        .unwrap_or_else(|| EnvFilter::new(level.as_str()));
    tracing_subscriber::registry()
        .with(severity)
        .with(fmt::layer().with_ansi(false).with_writer(writer)
            .with_filter(filter::filter_fn(|metadata| metadata.target() == TARGET)))
}

/// Initialize once, before opening the live adapter. Nothing is written to
/// the profile during startup, even if the incoming settings are malformed.
pub fn init(data_dir: &Path) -> bool {
    let document = crate::settings::read_log_document(data_dir);
    let level = document.as_ref().map(crate::settings::log_level).unwrap_or(LogLevel::Info);
    let rust_log = std::env::var("RUST_LOG").ok();
    if tracing::subscriber::set_global_default(
        subscriber(level, rust_log.as_deref(), std::io::stderr),
    ).is_err() {
        return false;
    }
    if document.is_err() {
        unreadable_configuration();
    }
    true
}

fn unreadable_configuration() {
    tracing::warn!(target: TARGET, "Native settings could not be read; startup diagnostic defaults were applied");
}

/// Does not print the profile path, connection metadata or user content.
pub fn opening_profile() {
    tracing::debug!(target: TARGET, "Opening the native profile");
}

/// The adapter exists and tray initialization has returned; the event loop
/// starts just after this. The tray itself is optional on some desktops.
pub fn start_gui(tray_available: bool) {
    tracing::trace!(target: TARGET, tray_available, "Native tray initialization completed");
    tracing::info!(target: TARGET, "Native GUI is starting");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::{self, Write}, sync::{Arc, Mutex}};

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> { Ok(()) }
    }

    // A new scoped subscriber is the startup boundary; don't mutate globals.
    fn run_startup(data_dir: &Path, rust_log: Option<&str>) -> String {
        let document = crate::settings::read_log_document(data_dir);
        let level = document.as_ref().map(crate::settings::log_level).unwrap_or(LogLevel::Info);
        let capture = Capture::default();
        let writer = capture.clone();
        tracing::subscriber::with_default(subscriber(level, rust_log, move || writer.clone()), || {
            if document.is_err() {
                unreadable_configuration();
            }
            opening_profile();
            start_gui(true);
            tracing::error!(target: "unreviewed_driver", "UNREVIEWED PRIVATE DRIVER EVENT");
        });
        let bytes = capture.0.lock().unwrap().clone();
        let output = String::from_utf8(bytes).unwrap();
        assert!(!output.contains("UNREVIEWED PRIVATE DRIVER EVENT"));
        output
    }

    #[test]
    fn startup_filters_real_native_messages_and_keeps_other_targets_out() {
        let dir = tempfile::tempdir().unwrap();
        let initial = run_startup(dir.path(), None);
        assert!(initial.contains("Native GUI is starting"));
        assert!(!initial.contains("Opening the native profile"));
        assert!(!initial.contains("Native tray initialization completed"));

        let path = crate::settings::settings_path(dir.path());
        std::fs::write(&path, r#"{"logLevel":"warn"}"#).unwrap();
        let warn = run_startup(dir.path(), None);
        assert!(warn.is_empty(), "Warn hides the real Info/Debug/Trace messages: {warn}");
        assert!(run_startup(dir.path(), Some("trace")).contains("Native tray initialization completed"));
        assert!(run_startup(dir.path(), Some("[[")).is_empty(), "invalid override falls back to Warn");

        std::fs::write(&path, r#"{"logLevel":"debug"}"#).unwrap();
        let debug = run_startup(dir.path(), None);
        assert!(debug.contains("Opening the native profile"));
        assert!(debug.contains("Native GUI is starting"));
        assert!(!debug.contains("Native tray initialization completed"));
    }

    #[test]
    fn startup_falls_back_without_rewriting_the_broken_profile() {
        let dir = tempfile::tempdir().unwrap();
        let path = crate::settings::settings_path(dir.path());
        for malformed in ["{abcdef", "  ", "[]"] {
            std::fs::write(&path, malformed).unwrap();
            let output = run_startup(dir.path(), None);
            assert!(output.contains("Native settings could not be read"));
            assert!(output.contains("Native GUI is starting"));
            assert!(!output.contains("Opening the native profile"));
            assert_eq!(std::fs::read_to_string(&path).unwrap(), malformed);
        }
    }
}
