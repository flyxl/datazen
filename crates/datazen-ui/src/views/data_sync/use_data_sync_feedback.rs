//! The Data Sync window's user-visible feedback — T459.
//!
//! Ported from `packages/frontend/src/windows/data-sync/useDataSyncFeedback.ts`
//! (34 lines). The React module is one hook returning four `useState` pairs;
//! in immediate mode the hook IS the state, so the four cells become the
//! fields of [`DataSyncFeedback`] and the four `Dispatch<SetStateAction<T>>`
//! setters become plain-value methods. Every React call site passes a
//! plain value (verified across `useDataSyncProfiles` / `useDataSyncComparison`
//! / `useDataSyncJobLifecycle` / `useDataSyncSelection` and the window: only
//! `setErrorMsg(…)`, `setErrorOpen(true/false)`, `setStatusMsg(…)` and
//! `setExecuteProgress(…)` with direct values, plus the window's
//! `onCloseError={() => feedback.setErrorOpen(false)}`) — no site uses the
//! updater-function arm of `SetStateAction`, so a value-taking setter is the
//! full contract.
//!
//! ## What maps where
//!
//! | React | here |
//! |---|---|
//! | `useDataSyncFeedback()` | `DataSyncFeedback::default()` (the hook is state creation) |
//! | `feedback.errorMsg` / `errorOpen` / `statusMsg` / `executeProgress` | the four `pub` fields, read directly |
//! | `feedback.setErrorMsg(v)` etc. | [`DataSyncFeedback::set_error_msg`] etc. |
//! | `onCloseError={() => feedback.setErrorOpen(false)}` | `set_error_open(false)` — the message is deliberately retained (React clears nothing on close) |
//!
//! Consumers (the window track, T268, and the sibling hooks
//! useDataSyncProfiles / useDataSyncComparison / useDataSyncJobLifecycle /
//! useDataSyncSelection) hold this struct as a field; the error dialog
//! paints while `error_open` is set and reads `error_msg`, the status bar
//! reads `status_msg` (falling back to the window title when empty), and
//! the running execution's progress text is `execute_progress`.

/// User-visible feedback of the Data Sync window: the error dialog, the
/// status bar line and the progress text of a running execution.
///
/// React's `DataSyncFeedback` interface: four independent cells with their
/// setters. Default state — no error shown, empty status, no progress — is
/// `useDataSyncFeedback()`'s four `useState` seeds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // consumers: the Data Sync window track (T268, not landed yet)
pub(crate) struct DataSyncFeedback {
    /// The error dialog's message body.
    pub(crate) error_msg: String,
    /// Whether the error dialog is open. Closing it (`setErrorOpen(false)`,
    /// the window's `onCloseError`) clears only this flag — the message is
    /// kept until the next `setErrorMsg` overwrites it.
    pub(crate) error_open: bool,
    /// The status bar's left line.
    pub(crate) status_msg: String,
    /// The running execution's progress text.
    pub(crate) execute_progress: String,
}

#[allow(dead_code)] // consumers: the Data Sync window track (T268, not landed yet)
impl DataSyncFeedback {
    /// `setErrorMsg(value)` — plain value only (no React call site passes an
    /// updater function).
    pub(crate) fn set_error_msg(&mut self, value: impl Into<String>) {
        self.error_msg = value.into();
    }

    /// `setErrorOpen(value)` — opens/closes the error dialog without
    /// touching the message.
    pub(crate) fn set_error_open(&mut self, value: bool) {
        self.error_open = value;
    }

    /// `setStatusMsg(value)`.
    pub(crate) fn set_status_msg(&mut self, value: impl Into<String>) {
        self.status_msg = value.into();
    }

    /// `setExecuteProgress(value)`.
    pub(crate) fn set_execute_progress(&mut self, value: impl Into<String>) {
        self.execute_progress = value.into();
    }
}
