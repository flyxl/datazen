//! Bounded webdriver diagnostics; only fixed stages, never SQL or backend errors.
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
};
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Stage {
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
pub(crate) fn note(dashboard: &str, widget: &str, stage: Stage) {
    let mut stages = STAGES.lock().unwrap_or_else(|e| e.into_inner());
    let key = (dashboard.to_owned(), widget.to_owned());
    if stages.contains_key(&key) || stages.len() < 128 {
        stages.insert(key, stage);
    }
}
pub(crate) fn stage(dashboard: &str, widget: &str) -> Option<Stage> {
    STAGES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&(dashboard.to_owned(), widget.to_owned()))
        .copied()
}
