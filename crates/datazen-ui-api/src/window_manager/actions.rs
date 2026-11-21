use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::build_docs_url;

use super::labels::{is_main_shell_menu_id, PENDING_CONNECTION_KEY};

pub const WINDOW_MANAGER_GAP: [&str; 6] = [
    "requirePlatform().windows.*",
    "emitCrossWindow",
    "localStorage.setItem",
    "settingsCommands.openPath",
    "window.open",
    "IS_DESKTOP_BUILD",
];

#[derive(Clone, Debug, PartialEq)]
pub struct MenuActionPlan {
    pub focus_main: bool,
    pub event: String,
    pub payload: Option<Value>,
}

pub fn emit_menu_action_plan(id: &str, payload: Option<Value>) -> MenuActionPlan {
    MenuActionPlan {
        focus_main: is_main_shell_menu_id(id),
        event: format!("menu:{id}"),
        payload,
    }
}

pub fn settings_window_plan(section: Option<&str>) -> MenuActionPlan {
    emit_menu_action_plan(
        "open-settings",
        section.map(|value| json!({ "section": value })),
    )
}

pub fn workflow_window_plan() -> MenuActionPlan {
    emit_menu_action_plan("workflow", None)
}

#[derive(Clone, Debug, PartialEq)]
pub struct DashboardWindowPlan {
    pub focus_main: bool,
    pub event: &'static str,
    pub payload: Option<Value>,
}

pub fn dashboard_window_plan(id: Option<&str>, name: Option<&str>) -> DashboardWindowPlan {
    match id.filter(|value| !value.is_empty()) {
        Some(id) => {
            let mut payload = Map::new();
            payload.insert("dashboardId".to_owned(), json!(id));
            if let Some(name) = name {
                payload.insert("dashboardName".to_owned(), json!(name));
            }
            DashboardWindowPlan {
                focus_main: true,
                event: "menu:open-dashboard",
                payload: Some(Value::Object(payload)),
            }
        }
        None => DashboardWindowPlan {
            focus_main: true,
            event: "menu:dashboard",
            payload: None,
        },
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConnectionWindowOpts {
    pub db_session_id: Option<String>,
    pub connection_id: Option<String>,
}

pub fn connection_payload(
    opts: &ConnectionWindowOpts,
    connection_name: &str,
    database: Option<&str>,
    database_type: Option<&str>,
    action: Option<&str>,
) -> BTreeMap<String, String> {
    let mut payload = BTreeMap::new();
    payload.insert("connectionName".to_owned(), connection_name.to_owned());
    if let Some(value) = opts
        .db_session_id
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        payload.insert("dbSessionId".to_owned(), value.to_owned());
    }
    if let Some(value) = opts
        .connection_id
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        payload.insert("connectionId".to_owned(), value.to_owned());
    }
    if let Some(value) = database.filter(|value| !value.is_empty()) {
        payload.insert("database".to_owned(), value.to_owned());
    }
    if let Some(value) = database_type.filter(|value| !value.is_empty()) {
        payload.insert("databaseType".to_owned(), value.to_owned());
    }
    if let Some(value) = action.filter(|value| !value.is_empty()) {
        payload.insert("action".to_owned(), value.to_owned());
    }
    payload
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConnectionWindowPlan {
    pub payload: BTreeMap<String, String>,
    pub stored_json: Option<String>,
    pub event: &'static str,
    pub focus_main: bool,
}

pub fn connection_window_plan<S>(
    opts: &ConnectionWindowOpts,
    connection_name: &str,
    database: Option<&str>,
    database_type: Option<&str>,
    action: Option<&str>,
    store: S,
) -> ConnectionWindowPlan
where
    S: Fn(&str, &str) -> Result<(), ()>,
{
    let payload = connection_payload(opts, connection_name, database, database_type, action);
    let encoded = serde_json::to_string(&payload).ok();
    let stored_json = encoded.and_then(|encoded| match store(PENDING_CONNECTION_KEY, &encoded) {
        Ok(()) => Some(encoded),
        Err(()) => None,
    });
    ConnectionWindowPlan {
        payload,
        stored_json,
        event: "datazen:open-connection",
        focus_main: true,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocsWindowPlan {
    pub url: String,
    pub open_browser: bool,
    pub native_attempt: bool,
}

pub fn docs_window_plan(
    language: &str,
    section: Option<&str>,
    is_desktop: bool,
    native_open_fails: bool,
) -> DocsWindowPlan {
    DocsWindowPlan {
        url: build_docs_url(language, section),
        open_browser: !is_desktop || native_open_fails,
        native_attempt: is_desktop,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[test]
    fn gap_surface_is_pinned() {
        assert_eq!(WINDOW_MANAGER_GAP.len(), 6);
        assert!(WINDOW_MANAGER_GAP.contains(&"emitCrossWindow"));
        assert_eq!(crate::WINDOW_MANAGER_GAP.len(), 6);
    }

    #[test]
    fn menu_actions_focus_main_only_for_shell_ids() {
        let settings = settings_window_plan(Some("appearance"));
        assert!(settings.focus_main);
        assert_eq!(settings.event, "menu:open-settings");
        assert_eq!(settings.payload, Some(json!({"section": "appearance"})));
        assert_eq!(settings_window_plan(None).payload, None);

        let workflow = workflow_window_plan();
        assert!(workflow.focus_main);
        assert_eq!(workflow.event, "menu:workflow");
        assert_eq!(workflow.payload, None);

        let custom = emit_menu_action_plan("custom-action", Some(json!(7)));
        assert!(!custom.focus_main);
        assert_eq!(custom.event, "menu:custom-action");
    }

    #[test]
    fn dashboard_plan_uses_different_events_for_id_and_generic() {
        let named = dashboard_window_plan(Some("d1"), Some("Sales"));
        assert!(named.focus_main);
        assert_eq!(named.event, "menu:open-dashboard");
        assert_eq!(
            named.payload,
            Some(json!({"dashboardId": "d1", "dashboardName": "Sales"}))
        );
        let anonymous = dashboard_window_plan(Some("d1"), None);
        assert_eq!(anonymous.payload, Some(json!({"dashboardId": "d1"})));
        let generic = dashboard_window_plan(None, None);
        assert_eq!(generic.event, "menu:dashboard");
        assert_eq!(generic.payload, None);
    }

    #[test]
    fn connection_payload_includes_only_truthy_optional_fields() {
        let opts = ConnectionWindowOpts {
            db_session_id: Some("s1".to_owned()),
            connection_id: Some("c1".to_owned()),
        };
        let payload = connection_payload(
            &opts,
            "Local",
            Some("shop"),
            Some("postgresql"),
            Some("connect"),
        );
        assert_eq!(payload["connectionName"], "Local");
        assert_eq!(payload["dbSessionId"], "s1");
        assert_eq!(payload["connectionId"], "c1");
        assert_eq!(payload["database"], "shop");
        assert_eq!(payload["databaseType"], "postgresql");
        assert_eq!(payload["action"], "connect");

        let minimal = connection_payload(&ConnectionWindowOpts::default(), "", None, None, None);
        assert_eq!(minimal.len(), 1);
        assert_eq!(minimal["connectionName"], "");
    }

    #[test]
    fn connection_plan_stores_json_and_falls_back_to_event_only() {
        let writes = RefCell::new(Vec::new());
        let plan = connection_window_plan(
            &ConnectionWindowOpts {
                db_session_id: Some("s1".to_owned()),
                connection_id: None,
            },
            "Local",
            None,
            None,
            None,
            |key, value| {
                writes.borrow_mut().push((key.to_owned(), value.to_owned()));
                Ok(())
            },
        );
        assert_eq!(plan.event, "datazen:open-connection");
        assert!(plan.focus_main);
        assert!(plan.stored_json.is_some());
        assert_eq!(writes.borrow()[0].0, PENDING_CONNECTION_KEY);

        let plan = connection_window_plan(
            &ConnectionWindowOpts::default(),
            "Local",
            None,
            None,
            None,
            |_, _| Err(()),
        );
        assert_eq!(plan.stored_json, None);
        assert_eq!(plan.payload["connectionName"], "Local");
    }

    #[test]
    fn docs_plan_uses_native_open_on_desktop_with_browser_fallback() {
        let desktop = docs_window_plan("en", Some("dataSync"), true, false);
        assert_eq!(desktop.url, crate::build_docs_url("en", Some("dataSync")));
        assert!(desktop.native_attempt);
        assert!(!desktop.open_browser);

        let failed = docs_window_plan("en", None, true, true);
        assert!(failed.native_attempt);
        assert!(failed.open_browser);

        let web = docs_window_plan("zh-CN", None, false, false);
        assert!(!web.native_attempt);
        assert!(web.open_browser);
    }
}
