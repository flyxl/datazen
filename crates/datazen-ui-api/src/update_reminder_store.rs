pub const UPDATE_CHECK_INTERVAL_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateProgress {
    Checking,
    Downloading { downloaded: u64, total: Option<u64> },
    Installing,
    Done { version: String },
    Idle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateCheckResult {
    UpToDate,
    Available { version: String },
    Installed { version: String },
    Error { message: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateOutcome {
    Check(UpdateCheckResult),
    Thrown(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UpdateReminderState {
    pub last_check_attempt_at: Option<i64>,
    pub available_version: Option<String>,
    pub pending_restart_version: Option<String>,
    pub is_checking: bool,
    pub is_installing: bool,
    pub progress: Option<UpdateProgress>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StatePatch {
    pub last_check_attempt_at: Option<Option<i64>>,
    pub available_version: Option<Option<String>>,
    pub pending_restart_version: Option<Option<String>>,
    pub is_checking: Option<bool>,
    pub is_installing: Option<bool>,
    pub progress: Option<Option<UpdateProgress>>,
    pub error: Option<Option<String>>,
    pub warnings: Vec<String>,
}

impl StatePatch {
    pub fn is_empty(&self) -> bool {
        self.last_check_attempt_at.is_none()
            && self.available_version.is_none()
            && self.pending_restart_version.is_none()
            && self.is_checking.is_none()
            && self.is_installing.is_none()
            && self.progress.is_none()
            && self.error.is_none()
            && self.warnings.is_empty()
    }

    pub fn apply(&self, state: &mut UpdateReminderState) {
        if let Some(value) = self.last_check_attempt_at {
            state.last_check_attempt_at = value;
        }
        if let Some(value) = &self.available_version {
            state.available_version = value.clone();
        }
        if let Some(value) = &self.pending_restart_version {
            state.pending_restart_version = value.clone();
        }
        if let Some(value) = self.is_checking {
            state.is_checking = value;
        }
        if let Some(value) = self.is_installing {
            state.is_installing = value;
        }
        if let Some(value) = &self.progress {
            state.progress = value.clone();
        }
        if let Some(value) = &self.error {
            state.error = value.clone();
        }
    }
}

pub fn is_check_due(state: &UpdateReminderState, now_ms: i64) -> bool {
    match state.last_check_attempt_at {
        None => true,
        Some(last) if now_ms < last => true,
        Some(last) => now_ms - last >= UPDATE_CHECK_INTERVAL_MS,
    }
}

pub fn begin_check(state: &UpdateReminderState, now_ms: i64) -> Option<StatePatch> {
    if state.is_checking || state.is_installing {
        return None;
    }
    let mut patch = StatePatch {
        is_checking: Some(true),
        ..Default::default()
    };
    patch.last_check_attempt_at = Some(Some(now_ms));
    Some(patch)
}

pub fn finish_check(outcome: &UpdateOutcome) -> StatePatch {
    let mut patch = StatePatch {
        is_checking: Some(false),
        ..Default::default()
    };
    match outcome {
        UpdateOutcome::Check(UpdateCheckResult::Available { version }) => {
            patch.available_version = Some(Some(version.clone()));
            patch.error = Some(None);
        }
        UpdateOutcome::Check(UpdateCheckResult::UpToDate) => {
            patch.available_version = Some(None);
            patch.error = Some(None);
        }
        UpdateOutcome::Check(UpdateCheckResult::Error { message }) => {
            patch
                .warnings
                .push(format!("[updater] scheduled check failed: {message}"));
        }
        UpdateOutcome::Check(UpdateCheckResult::Installed { .. }) => {}
        UpdateOutcome::Thrown(detail) => {
            patch
                .warnings
                .push(format!("[updater] scheduled check failed: {detail}"));
        }
    }
    patch
}

pub fn begin_install(state: &UpdateReminderState) -> bool {
    state.available_version.is_some() && !state.is_installing
}

pub fn install_start_patch() -> StatePatch {
    StatePatch {
        is_installing: Some(true),
        error: Some(None),
        progress: Some(Some(UpdateProgress::Checking)),
        ..Default::default()
    }
}

pub fn install_result_patch(outcome: &UpdateOutcome) -> StatePatch {
    let mut patch = StatePatch {
        is_installing: Some(false),
        ..Default::default()
    };
    match outcome {
        UpdateOutcome::Check(UpdateCheckResult::UpToDate) => {
            patch.available_version = Some(None);
            patch.progress = Some(None);
        }
        UpdateOutcome::Check(UpdateCheckResult::Error { message }) => {
            patch.error = Some(Some(message.clone()));
            patch.progress = Some(None);
        }
        UpdateOutcome::Check(UpdateCheckResult::Installed { version }) => {
            patch.available_version = Some(None);
            patch.pending_restart_version = Some(Some(version.clone()));
            patch.progress = Some(None);
        }
        UpdateOutcome::Check(UpdateCheckResult::Available { .. }) => {}
        UpdateOutcome::Thrown(detail) => {
            patch.error = Some(Some(detail.clone()));
            patch.progress = Some(None);
        }
    }
    patch
}

pub fn progress_patch(progress: UpdateProgress) -> StatePatch {
    StatePatch {
        progress: Some(Some(progress)),
        ..Default::default()
    }
}

pub fn restart_allowed(state: &UpdateReminderState) -> bool {
    state.pending_restart_version.is_some()
}

pub fn restart_error_patch(detail: &str) -> StatePatch {
    StatePatch {
        error: Some(Some(detail.to_owned())),
        ..Default::default()
    }
}

pub const UPDATE_REMINDER_SURFACE: [&str; 8] = [
    "checkForUpdatesIfDue",
    "checkForUpdates",
    "installUpdate",
    "restartToApplyUpdate",
    "isCheckDue",
    "beginCheck",
    "finishCheck",
    "UPDATE_REMINDER_GAP",
];

pub const UPDATE_REMINDER_GAP: [&str; 5] = [
    "Zustand store creation and setState wiring",
    "updater host flows for check, install and relaunch IPC",
    "Date.now deadline and backward-clock handling",
    "UpdateProgress callback from the install stream",
    "console.warn diagnostics for scheduled check failures",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadline_handles_missing_recent_and_backward_clocks() {
        let mut state = UpdateReminderState::default();
        assert!(is_check_due(&state, 100));
        state.last_check_attempt_at = Some(100);
        assert!(!is_check_due(&state, 100 + UPDATE_CHECK_INTERVAL_MS - 1));
        assert!(is_check_due(&state, 100 + UPDATE_CHECK_INTERVAL_MS));
        assert!(is_check_due(&state, 50));
    }

    #[test]
    fn busy_states_block_checks_but_record_the_attempt() {
        let mut state = UpdateReminderState {
            is_checking: true,
            ..Default::default()
        };
        assert_eq!(begin_check(&state, 10), None);
        state.is_checking = false;
        state.is_installing = true;
        assert_eq!(begin_check(&state, 10), None);
        state.is_installing = false;
        let patch = begin_check(&state, 10).unwrap();
        assert_eq!(patch.last_check_attempt_at, Some(Some(10)));
        assert_eq!(patch.is_checking, Some(true));
        patch.apply(&mut state);
        assert_eq!(state.last_check_attempt_at, Some(10));
        assert!(state.is_checking);
    }

    #[test]
    fn check_outcomes_update_availability_or_warn() {
        let available = finish_check(&UpdateOutcome::Check(UpdateCheckResult::Available {
            version: "2.0".to_owned(),
        }));
        assert_eq!(available.available_version, Some(Some("2.0".to_owned())));
        assert_eq!(available.error, Some(None));
        assert_eq!(available.is_checking, Some(false));

        let up_to_date = finish_check(&UpdateOutcome::Check(UpdateCheckResult::UpToDate));
        assert_eq!(up_to_date.available_version, Some(None));
        assert!(up_to_date.warnings.is_empty());

        let failed = finish_check(&UpdateOutcome::Check(UpdateCheckResult::Error {
            message: "boom".to_owned(),
        }));
        assert_eq!(failed.warnings, ["[updater] scheduled check failed: boom"]);
        assert_eq!(failed.available_version, None);

        let thrown = finish_check(&UpdateOutcome::Thrown("oops".to_owned()));
        assert_eq!(thrown.warnings.len(), 1);
    }

    #[test]
    fn install_requires_an_available_version_and_not_busy() {
        let mut state = UpdateReminderState::default();
        assert!(!begin_install(&state));
        state.available_version = Some("2.0".to_owned());
        assert!(begin_install(&state));
        state.is_installing = true;
        assert!(!begin_install(&state));
        let patch = install_start_patch();
        assert_eq!(patch.progress, Some(Some(UpdateProgress::Checking)));
        assert_eq!(patch.error, Some(None));
        assert!(patch.is_installing.unwrap_or(false));
    }

    #[test]
    fn install_outcomes_map_to_pending_restart_or_error() {
        let installed = install_result_patch(&UpdateOutcome::Check(UpdateCheckResult::Installed {
            version: "2.0".to_owned(),
        }));
        assert_eq!(
            installed.pending_restart_version,
            Some(Some("2.0".to_owned()))
        );
        assert_eq!(installed.available_version, Some(None));
        assert_eq!(installed.progress, Some(None));

        let error = install_result_patch(&UpdateOutcome::Thrown("boom".to_owned()));
        assert_eq!(error.error, Some(Some("boom".to_owned())));
        assert_eq!(error.progress, Some(None));

        let up_to_date = install_result_patch(&UpdateOutcome::Check(UpdateCheckResult::UpToDate));
        assert_eq!(up_to_date.available_version, Some(None));
        assert_eq!(up_to_date.error, None);
    }

    #[test]
    fn restart_and_progress_follow_the_store_guards() {
        let mut state = UpdateReminderState::default();
        assert!(!restart_allowed(&state));
        state.pending_restart_version = Some("2.0".to_owned());
        assert!(restart_allowed(&state));
        assert_eq!(
            restart_error_patch("boom").error,
            Some(Some("boom".to_owned()))
        );
        let patch = progress_patch(UpdateProgress::Idle);
        patch.apply(&mut state);
        assert_eq!(state.progress, Some(UpdateProgress::Idle));
        assert_eq!(UPDATE_REMINDER_SURFACE.len(), 8);
        assert_eq!(UPDATE_REMINDER_GAP.len(), 5);
        assert_eq!(crate::UPDATE_REMINDER_GAP.len(), 5);
    }
}
