//! Bounded, form-neutral monitor diagnostics.

use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    GrantChecking,
    GrantValidated,
    GrantRejected,
    OccurrencePreparing,
    JobAdmission,
    AdmissionRejected,
    Admitted,
    Dispatching,
    RunReady,
    DispatchFailed,
}

static STAGES: LazyLock<Mutex<HashMap<(String, String), Stage>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Keep only a bounded projection. Never record SQL, contexts or backend errors.
pub fn note(dashboard: &str, widget: &str, stage: Stage) {
    let mut stages = STAGES.lock().unwrap_or_else(|error| error.into_inner());
    let key = (dashboard.to_owned(), widget.to_owned());
    if stages.contains_key(&key) || stages.len() < 128 {
        stages.insert(key, stage);
    }
}

pub fn stage(dashboard: &str, widget: &str) -> Option<Stage> {
    STAGES
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(&(dashboard.to_owned(), widget.to_owned()))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_is_a_bounded_stage_projection() {
        note("dashboard", "widget", Stage::GrantChecking);
        assert_eq!(stage("dashboard", "widget"), Some(Stage::GrantChecking));
    }
}
