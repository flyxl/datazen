use crate::types::wapp::WappSummary;

pub const WAPPS_CHANGED_EVENT: &str = "wapps:changed";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WappError {
    Message(String),
    Raw(String),
}

pub fn to_error_message(error: &WappError) -> String {
    match error {
        WappError::Message(message) | WappError::Raw(message) => message.clone(),
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WappStoreState {
    pub wapps: Vec<WappSummary>,
    pub loaded: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FetchPatch {
    pub wapps: Option<Vec<WappSummary>>,
    pub loaded: bool,
    pub error: Option<String>,
}

pub fn fetch_patch(result: &Result<Vec<WappSummary>, WappError>) -> FetchPatch {
    match result {
        Ok(wapps) => FetchPatch {
            wapps: Some(wapps.clone()),
            loaded: true,
            error: None,
        },
        Err(error) => FetchPatch {
            wapps: None,
            loaded: true,
            error: Some(to_error_message(error)),
        },
    }
}

pub fn optimistic_toggle(wapps: &[WappSummary], id: &str, enabled: bool) -> Vec<WappSummary> {
    wapps
        .iter()
        .map(|wapp| {
            if wapp.id == id {
                WappSummary {
                    enabled,
                    ..wapp.clone()
                }
            } else {
                wapp.clone()
            }
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct MutationEffects {
    pub optimistic: Option<Vec<WappSummary>>,
    pub refetch: bool,
    pub error: Option<String>,
    pub rethrow: bool,
}

pub fn set_enabled_effects(
    wapps: &[WappSummary],
    id: &str,
    enabled: bool,
    ipc_ok: bool,
) -> MutationEffects {
    MutationEffects {
        optimistic: Some(optimistic_toggle(wapps, id, enabled)),
        refetch: true,
        error: if ipc_ok { None } else { None },
        rethrow: !ipc_ok,
    }
}

pub fn remove_effects(ipc_ok: bool, error: Option<&WappError>) -> MutationEffects {
    MutationEffects {
        optimistic: None,
        refetch: ipc_ok || error.is_some(),
        error: if ipc_ok {
            None
        } else {
            error.map(to_error_message)
        },
        rethrow: !ipc_ok,
    }
}

pub fn by_id<'a>(wapps: &'a [WappSummary], id: &str) -> Option<&'a WappSummary> {
    wapps.iter().find(|wapp| wapp.id == id)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListenerResult {
    NoPlatform,
    Pending,
    Rejected,
    Threw,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListenerDecision {
    Noop,
    Register,
    Reset,
}

pub fn listener_decision(started: bool, result: ListenerResult) -> ListenerDecision {
    if started {
        return ListenerDecision::Noop;
    }
    match result {
        ListenerResult::NoPlatform | ListenerResult::Rejected | ListenerResult::Threw => {
            ListenerDecision::Reset
        }
        ListenerResult::Pending => ListenerDecision::Register,
    }
}

pub fn should_refetch_on_event() -> bool {
    true
}

pub const WAPP_STORE_SURFACE: [&str; 9] = [
    "fetch",
    "setEnabled",
    "remove",
    "byId",
    "ensureWappsChangedListener",
    "fetchPatch",
    "optimisticToggle",
    "listenerDecision",
    "WAPP_STORE_GAP",
];

pub const WAPP_STORE_GAP: [&str; 6] = [
    "Zustand store creation and setState wiring",
    "wappCommands listWapps / setWappEnabled / removeWapp IPC",
    "getPlatformOrNull events.listenCrossWindow EventChannel port",
    "module-level subscriptionStarted guard and its retry on failure",
    "Promise rejection vs synchronous throw from the listener binding",
    "cross-window wapps:changed broadcast source",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn wapp(id: &str, enabled: bool) -> WappSummary {
        WappSummary {
            id: id.to_owned(),
            name: id.to_owned(),
            version: "1".to_owned(),
            api_version: 2,
            author: None,
            description: None,
            icon: None,
            enabled,
            permissions: Vec::new(),
            pages: Vec::new(),
            themes: Vec::new(),
            ai_tools: None,
        }
    }

    #[test]
    fn fetch_records_loaded_and_keeps_or_sets_the_error() {
        let ok = fetch_patch(&Ok(vec![wapp("a", true)]));
        assert!(ok.loaded);
        assert_eq!(ok.wapps.as_ref().map(Vec::len), Some(1));
        assert_eq!(ok.error, None);

        let failed = fetch_patch(&Err(WappError::Message("boom".to_owned())));
        assert!(failed.loaded);
        assert_eq!(failed.wapps, None);
        assert_eq!(failed.error.as_deref(), Some("boom"));
    }

    #[test]
    fn toggle_only_changes_the_matching_wapp() {
        let wapps = [wapp("a", false), wapp("b", true)];
        let updated = optimistic_toggle(&wapps, "a", true);
        assert!(updated[0].enabled);
        assert!(updated[1].enabled);
        assert!(!wapps[0].enabled);
    }

    #[test]
    fn set_enabled_refetches_on_both_paths_and_marks_failure() {
        let wapps = [wapp("a", false)];
        let ok = set_enabled_effects(&wapps, "a", true, true);
        assert!(ok.refetch);
        assert!(!ok.rethrow);
        assert_eq!(ok.error, None);
        assert!(ok.optimistic.as_ref().is_some_and(|list| list[0].enabled));

        let failed = set_enabled_effects(&wapps, "a", true, false);
        assert!(failed.refetch);
        assert!(failed.rethrow);
    }

    #[test]
    fn remove_only_refetches_and_sets_error_on_failure() {
        let ok = remove_effects(true, None);
        assert!(ok.refetch);
        assert!(!ok.rethrow);
        assert_eq!(ok.optimistic, None);

        let failed = remove_effects(false, Some(&WappError::Raw("boom".to_owned())));
        assert!(failed.refetch);
        assert_eq!(failed.error.as_deref(), Some("boom"));
        assert!(failed.rethrow);
    }

    #[test]
    fn by_id_returns_none_for_unknown_ids() {
        let wapps = [wapp("a", true)];
        assert_eq!(by_id(&wapps, "a").map(|w| w.id.as_str()), Some("a"));
        assert_eq!(by_id(&wapps, "missing"), None);
    }

    #[test]
    fn listener_guard_resets_on_every_failure_path() {
        assert_eq!(
            listener_decision(true, ListenerResult::NoPlatform),
            ListenerDecision::Noop
        );
        assert_eq!(
            listener_decision(false, ListenerResult::NoPlatform),
            ListenerDecision::Reset
        );
        assert_eq!(
            listener_decision(false, ListenerResult::Rejected),
            ListenerDecision::Reset
        );
        assert_eq!(
            listener_decision(false, ListenerResult::Threw),
            ListenerDecision::Reset
        );
        assert_eq!(
            listener_decision(false, ListenerResult::Pending),
            ListenerDecision::Register
        );
        assert!(should_refetch_on_event());
        assert_eq!(WAPP_STORE_SURFACE.len(), 9);
        assert_eq!(WAPP_STORE_GAP.len(), 6);
        assert_eq!(crate::WAPP_STORE_GAP.len(), 6);
    }
}
