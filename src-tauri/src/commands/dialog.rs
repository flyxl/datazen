//! Central native-dialog gateway.
//!
//! Single mechanism for every `tauri-plugin-dialog` invocation in the command
//! layer: each dialog kind has exactly ONE helper in this module, and every
//! helper first consults the E2E dialog-injection queue before touching the OS:
//!
//! ```text
//! gateway(app, …) → queue non-empty? consume one answer FIFO (webdriver only)
//!                 └─ otherwise → real native dialog on a blocking worker
//! ```
//!
//! Webdriver/E2E builds pre-inject dialog results through
//! [`test_inject_dialog_result`] and clear them between cases with
//! [`test_reset_dialog_queue`]. Both IPCs are compiled out of production
//! builds (cfg gates here + registration gates in `bootstrap.rs`); there the queue
//! can never receive an answer and every request reaches the native UI.
//!
//! There must be no second judgement path: command code never calls
//! `tauri_plugin_dialog` directly and never inspects the queue itself.

use super::error::CommandError;
use std::collections::VecDeque;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, FilePath, MessageDialogButtons, MessageDialogKind};

// ---------------------------------------------------------------------------
// Types (shared by all builds; inert plumbing in production)
// ---------------------------------------------------------------------------

/// Normalised answer of one dialog interaction, shared by every dialog kind.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(feature = "webdriver"), allow(dead_code))]
pub(crate) enum DialogAnswer {
    /// User dismissed / cancelled: file pickers yield `None`, message dialogs
    /// yield `false`.
    Cancelled,
    /// User picked an absolute filesystem path (file or folder). Message
    /// dialogs treat any non-cancelled answer as confirmed (`true`).
    Path(PathBuf),
}

impl DialogAnswer {
    /// Map to the `Option<PathBuf>` shape file/folder pickers return.
    fn into_option_path(self) -> Option<PathBuf> {
        match self {
            Self::Cancelled => None,
            Self::Path(path) => Some(path),
        }
    }

    /// Map to the boolean shape OkCancel message dialogs return.
    #[allow(dead_code)] // used only by confirm_message + tests
    fn into_confirmed(self) -> bool {
        matches!(self, Self::Path(_))
    }
}

/// Wire payload accepted by the webdriver-only `test_inject_dialog_result`
/// IPC. Exactly one of two shapes: `{"canceled": true}` (dismiss) or
/// `{"path": "/abs/file.ext"}` (pick). Anything else is rejected at inject
/// time so a typo'd fixture fails loudly instead of silently falling through
/// to a real native dialog.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "webdriver"), allow(dead_code))]
pub struct InjectedDialogResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    canceled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
}

impl InjectedDialogResult {
    /// Normalise both accepted wire shapes into one [`DialogAnswer`].
    #[cfg_attr(not(feature = "webdriver"), allow(dead_code))]
    fn into_answer(self) -> Result<DialogAnswer, CommandError> {
        if self.canceled == Some(true) {
            return Ok(DialogAnswer::Cancelled);
        }
        self.path
            .map(|p| DialogAnswer::Path(PathBuf::from(p)))
            .ok_or_else(|| {
                CommandError::Validation(
                    "dialog result must be {\"canceled\":true} or {\"path\":\"…\"}".into(),
                )
            })
    }
}

/// Managed FIFO of injected dialog answers, consumed by every gateway helper.
///
/// The type exists in all builds so the gateway keeps one shape; production
/// builds never manage it nor construct an answer, so it stays inert plumbing
/// (`next_answer` short-circuits without touching any state).
#[derive(Default)]
pub struct DialogInjectionQueue(std::sync::Mutex<VecDeque<DialogAnswer>>);

#[cfg_attr(not(feature = "webdriver"), allow(dead_code))]
impl DialogInjectionQueue {
    fn push(&self, answer: DialogAnswer) {
        self.0
            .lock()
            .expect("dialog injection queue poisoned")
            .push_back(answer);
    }

    fn pop(&self) -> Option<DialogAnswer> {
        self.0
            .lock()
            .expect("dialog injection queue poisoned")
            .pop_front()
    }

    fn clear(&self) {
        self.0
            .lock()
            .expect("dialog injection queue poisoned")
            .clear();
    }
}

/// The single judgement point shared by every gateway helper: consume one
/// injected answer if the webdriver injection queue is non-empty (FIFO),
/// otherwise fall through to the real native dialog.
fn next_answer(_app: &AppHandle) -> Option<DialogAnswer> {
    #[cfg(feature = "webdriver")]
    {
        use tauri::Manager;
        let answer = _app.state::<DialogInjectionQueue>().pop();
        if let Some(a) = &answer {
            tracing::debug!(
                injected = true,
                canceled = matches!(a, DialogAnswer::Cancelled),
                "native dialog replaced by injected result"
            );
        }
        answer
    }
    #[cfg(not(feature = "webdriver"))]
    {
        // Production: the injection surface is compiled out — nothing can be
        // queued, so always open the real native dialog.
        None
    }
}

// ---------------------------------------------------------------------------
// Native-dialog execution shim
// ---------------------------------------------------------------------------

/// Run one blocking native-dialog operation on a worker thread.
///
/// Moved here (from config.rs) as the single execution shim for ALL dialogs:
/// sync IPC + `blocking_pick_*` freezes macOS (main thread waits for Finder,
/// Finder waits for the main thread). Callback `pick_file` + `oneshot` await
/// also freezes: the plugin `block_on`s the dialog on the same runtime the
/// command is waiting on. Async command + `spawn_blocking` + `blocking_*`
/// is the pattern tauri-plugin-dialog documents for async commands.
async fn run_blocking_dialog<T, F>(f: F) -> Result<T, CommandError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| CommandError::Internal(format!("native dialog task: {e}")))
}

/// Convert an optional picked [`FilePath`]; `None` means cancelled.
fn picked_opt_to_path(picked: Option<FilePath>) -> Result<Option<PathBuf>, CommandError> {
    picked
        .map(|fp| {
            fp.into_path()
                .map_err(|e| CommandError::Validation(format!("Invalid dialog path: {e}")))
        })
        .transpose()
}

// ---------------------------------------------------------------------------
// Gateway helpers — the ONLY tauri-plugin-dialog call sites in the commands
// ---------------------------------------------------------------------------

/// Native **save** dialog behind the injection queue. Returns the picked
/// destination path, or `None` when cancelled.
pub(crate) async fn save_file(
    app: &AppHandle,
    filter: (String, Vec<String>),
    default_file_name: String,
) -> Result<Option<PathBuf>, CommandError> {
    if let Some(answer) = next_answer(app) {
        return Ok(answer.into_option_path());
    }
    let app = app.clone();
    let picked = run_blocking_dialog(move || {
        let extensions: Vec<&str> = filter.1.iter().map(String::as_str).collect();
        app.dialog()
            .file()
            .add_filter(&filter.0, &extensions)
            .set_file_name(&default_file_name)
            .blocking_save_file()
    })
    .await?;
    picked_opt_to_path(picked)
}

/// Native **open** dialog behind the injection queue. Filters are applied in
/// order. Returns the picked file path, or `None` when cancelled.
pub(crate) async fn open_file(
    app: &AppHandle,
    filters: Vec<(String, Vec<String>)>,
) -> Result<Option<PathBuf>, CommandError> {
    if let Some(answer) = next_answer(app) {
        return Ok(answer.into_option_path());
    }
    let app = app.clone();
    let picked = run_blocking_dialog(move || {
        let mut builder = app.dialog().file();
        for (name, extensions) in &filters {
            let exts: Vec<&str> = extensions.iter().map(String::as_str).collect();
            builder = builder.add_filter(name, &exts);
        }
        builder.blocking_pick_file()
    })
    .await?;
    picked_opt_to_path(picked)
}

/// Native **folder** picker behind the injection queue. Returns the picked
/// folder path, or `None` when cancelled.
pub(crate) async fn pick_folder(app: &AppHandle) -> Result<Option<PathBuf>, CommandError> {
    if let Some(answer) = next_answer(app) {
        return Ok(answer.into_option_path());
    }
    let app = app.clone();
    let picked = run_blocking_dialog(move || app.dialog().file().blocking_pick_folder()).await?;
    picked_opt_to_path(picked)
}

/// Native OkCancel **message** dialog behind the injection queue (warning
/// kind, matching its single caller). Returns `true` when confirmed, `false`
/// when dismissed.
#[allow(dead_code)] // reserved for future IPC surface
pub(crate) async fn confirm_message(
    app: &AppHandle,
    title: &str,
    message: &str,
) -> Result<bool, CommandError> {
    if let Some(answer) = next_answer(app) {
        return Ok(answer.into_confirmed());
    }
    let app = app.clone();
    let title = title.to_string();
    let message = message.to_string();
    Ok(run_blocking_dialog(move || {
        app.dialog()
            .message(&message)
            .title(&title)
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancel)
            .blocking_show()
    })
    .await?)
}

// ---------------------------------------------------------------------------
// Injection IPC (webdriver builds only — compiled out of production)
// ---------------------------------------------------------------------------

/// **Webdriver builds only.** Queue one injected dialog result (FIFO): the
/// NEXT native-dialog request in the app consumes it instead of opening the
/// OS UI. Wire shapes: `{"canceled": true}` or `{"path": "/abs/file.ext"}`.
#[cfg(feature = "webdriver")]
#[tauri::command]
pub fn test_inject_dialog_result(
    app: AppHandle,
    result: InjectedDialogResult,
) -> Result<(), CommandError> {
    use tauri::Manager;

    let answer = result.into_answer()?;
    app.state::<DialogInjectionQueue>().push(answer);
    tracing::info!("dialog result injected");
    Ok(())
}

/// **Webdriver builds only.** Clear any pending injected results (use-case
/// isolation between tests).
#[cfg(feature = "webdriver")]
#[tauri::command]
pub fn test_reset_dialog_queue(app: AppHandle) -> Result<(), CommandError> {
    use tauri::Manager;

    app.state::<DialogInjectionQueue>().clear();
    tracing::info!("dialog injection queue reset");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = include_str!("dialog.rs");
    const BOOTSTRAP_RS: &str = include_str!("../bootstrap.rs");

    /// `(verb, tail)` pairs assembling `test_<verb>_dialog_<tail>` needles at
    /// runtime so this test's own source never contains them verbatim
    /// (precedent: backup.rs ipc_contract_guards).
    fn injection_commands() -> [(&'static str, &'static str); 2] {
        [("inject", "result"), ("reset", "queue")]
    }

    // -- wire shapes --------------------------------------------------------

    #[test]
    fn canceled_shape_parses_to_cancelled_answer() {
        let result: InjectedDialogResult =
            serde_json::from_value(serde_json::json!({"canceled": true})).unwrap();
        assert_eq!(result.into_answer().unwrap(), DialogAnswer::Cancelled);
    }

    #[test]
    fn path_shape_parses_to_path_answer() {
        let result: InjectedDialogResult =
            serde_json::from_value(serde_json::json!({"path": "/tmp/e2e/out.sql"})).unwrap();
        assert_eq!(
            result.into_answer().unwrap(),
            DialogAnswer::Path(PathBuf::from("/tmp/e2e/out.sql"))
        );
    }

    #[test]
    fn neither_shape_is_rejected_at_inject_time() {
        for payload in [
            serde_json::json!({}),
            serde_json::json!({"canceled": false}),
            serde_json::json!({"canceled": null}),
        ] {
            let result: InjectedDialogResult = serde_json::from_value(payload).unwrap();
            let err = result.into_answer().unwrap_err();
            assert!(matches!(err, CommandError::Validation(ref m) if m.contains("canceled")));
        }
    }

    #[test]
    fn wire_shapes_serialize_for_e2e_fixtures() {
        let cancel = InjectedDialogResult {
            canceled: Some(true),
            path: None,
        };
        assert_eq!(
            serde_json::to_value(&cancel).unwrap(),
            serde_json::json!({"canceled": true})
        );
        let pick = InjectedDialogResult {
            canceled: None,
            path: Some("/x/y.zip".into()),
        };
        assert_eq!(
            serde_json::to_value(&pick).unwrap(),
            serde_json::json!({"path": "/x/y.zip"})
        );
    }

    // -- FIFO queue ---------------------------------------------------------

    #[test]
    fn queue_consumes_fifo_across_both_shapes() {
        let queue = DialogInjectionQueue::default();
        assert_eq!(queue.pop(), None, "empty queue yields nothing");

        queue.push(DialogAnswer::Path(PathBuf::from("/first.sql")));
        queue.push(DialogAnswer::Cancelled);
        queue.push(DialogAnswer::Path(PathBuf::from("/second.zip")));

        assert_eq!(
            queue.pop(),
            Some(DialogAnswer::Path(PathBuf::from("/first.sql")))
        );
        assert_eq!(queue.pop(), Some(DialogAnswer::Cancelled));
        assert_eq!(
            queue.pop(),
            Some(DialogAnswer::Path(PathBuf::from("/second.zip")))
        );
        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn reset_clears_pending_answers() {
        let queue = DialogInjectionQueue::default();
        queue.push(DialogAnswer::Cancelled);
        queue.push(DialogAnswer::Path(PathBuf::from("/stale")));
        queue.clear();
        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn answer_maps_to_picker_and_message_semantics() {
        assert_eq!(DialogAnswer::Cancelled.clone().into_option_path(), None);
        assert!(!DialogAnswer::Cancelled.into_confirmed());
        assert_eq!(
            DialogAnswer::Path(PathBuf::from("/p"))
                .clone()
                .into_option_path(),
            Some(PathBuf::from("/p"))
        );
        assert!(DialogAnswer::Path(PathBuf::from("/p")).into_confirmed());
    }

    // -- execution shim ------------------------------------------------------

    #[tokio::test]
    async fn run_blocking_dialog_runs_off_caller() {
        let value = run_blocking_dialog(|| 7u8).await.unwrap();
        assert_eq!(value, 7);
    }

    #[test]
    fn picked_none_is_cancel_not_error() {
        assert_eq!(picked_opt_to_path(None).unwrap(), None);
    }

    // -- feature-combination guards ------------------------------------------

    #[test]
    fn injection_ipc_definitions_carry_their_own_webdriver_gate() {
        // error.rs gating-test style, source-level: each IPC definition must
        // sit directly behind `#[cfg(feature = "webdriver")]`. include_str!
        // sees raw text in every build, so the pinned gate placement plus a
        // green default-feature build together are the compile-time proof
        // that production never emits these commands.
        for (verb, tail) in injection_commands() {
            let def_needle = format!("pub fn test_{verb}_dialog_{tail}(");
            let count = SOURCE.match_indices(&def_needle).count();
            assert_eq!(count, 1, "`{def_needle}` must be defined exactly once");
            let pos = SOURCE.find(&def_needle).unwrap();
            let gate = SOURCE[..pos]
                .rfind("#[cfg(feature")
                .unwrap_or_else(|| panic!("`{def_needle}` must carry a cfg gate"));
            assert!(
                SOURCE[gate..pos].contains("feature = \"webdriver\""),
                "`{def_needle}` must sit directly behind the webdriver cfg gate"
            );
        }
    }

    #[test]
    fn injection_registration_is_cfg_gated_in_bootstrap_rs() {
        for (verb, tail) in injection_commands() {
            let reg_needle = format!("commands::test_{verb}_dialog_{tail},");
            let count = BOOTSTRAP_RS.match_indices(&reg_needle).count();
            assert_eq!(count, 1, "`{reg_needle}` must register exactly once");
            let pos = BOOTSTRAP_RS.find(&reg_needle).unwrap();
            let gate = BOOTSTRAP_RS[..pos]
                .rfind("#[cfg(feature")
                .expect("registration must carry its own cfg gate");
            assert!(
                BOOTSTRAP_RS[gate..pos].contains("feature = \"webdriver\""),
                "`{reg_needle}` must sit directly behind the webdriver cfg gate"
            );
        }
    }

    #[test]
    fn injection_queue_state_is_managed_only_behind_webdriver_gate() {
        let needle = "commands::DialogInjectionQueue";
        let count = BOOTSTRAP_RS.match_indices(needle).count();
        assert_eq!(count, 1, "the managed queue line must appear exactly once");
        let pos = BOOTSTRAP_RS.find(needle).unwrap();
        let gate = BOOTSTRAP_RS[..pos]
            .rfind("#[cfg(feature")
            .expect("managed queue must carry its own cfg gate");
        assert!(
            BOOTSTRAP_RS[gate..pos].contains("feature = \"webdriver\""),
            "the managed queue must sit behind the webdriver cfg gate"
        );
    }
}
