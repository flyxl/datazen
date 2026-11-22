use super::types::{ContentViewPanel, PanelKind, SubTab, SubTabId};

fn translate(t: Option<&dyn Fn(&str) -> String>, key: &str, fallback: &str) -> String {
    match t {
        Some(translate) => translate(key),
        None => fallback.to_owned(),
    }
}

pub fn get_panel_short_label(
    panel: &ContentViewPanel,
    t: Option<&dyn Fn(&str) -> String>,
) -> String {
    match panel.kind {
        PanelKind::Table => panel.table_name.clone().unwrap_or_default(),
        PanelKind::View => panel.view_name.clone().unwrap_or_default(),
        PanelKind::Query => panel.title.clone().unwrap_or_default(),
        PanelKind::CreateTable => translate(t, "common.newTable", "New Table"),
        PanelKind::ErDiagram => translate(t, "common.erDiagram", "ER Diagram"),
        PanelKind::Objects => translate(t, "objects.title", "Objects"),
        PanelKind::Privileges => translate(t, "privileges.title", "Privileges"),
        PanelKind::ServerStatus => translate(t, "serverStatus.dashboardTitle", "Server Dashboard"),
        PanelKind::Processes => translate(t, "common.processList", "Process List"),
        PanelKind::DbObject => panel.object_name.clone().unwrap_or_default(),
        PanelKind::RedisDb => panel.db_name.clone().unwrap_or_default(),
        PanelKind::Unknown => String::new(),
    }
}

pub fn resolve_panel_tab_database(
    panel: &ContentViewPanel,
    session_database: Option<&str>,
) -> Option<String> {
    match panel.kind {
        PanelKind::Table | PanelKind::View | PanelKind::CreateTable => {
            panel.database.clone().or_else(|| session_database.map(str::to_owned))
        }
        PanelKind::RedisDb => panel.db_name.clone(),
        _ => session_database.map(str::to_owned),
    }
}

pub fn get_panel_tab_label(
    panel: &ContentViewPanel,
    session_database: Option<&str>,
    t: Option<&dyn Fn(&str) -> String>,
) -> String {
    let short = get_panel_short_label(panel, t);
    let database = resolve_panel_tab_database(panel, session_database);
    let mut parts = vec![panel.connection_name.clone()];
    if let Some(database) = database {
        let database = database.trim();
        if !database.is_empty() {
            parts.push(database.to_owned());
        }
    }
    if !short.is_empty() {
        parts.push(short);
    }
    parts.join(" · ")
}

pub fn get_panel_label(panel: &ContentViewPanel, t: Option<&dyn Fn(&str) -> String>) -> String {
    get_panel_short_label(panel, t)
}

pub fn get_sub_tabs(t: &dyn Fn(&str) -> String, read_only: bool) -> Vec<SubTab> {
    let mut tabs = vec![
        SubTab { id: SubTabId::Data, label: t("connWin.data") },
        SubTab { id: SubTabId::Structure, label: t("connWin.structure") },
    ];
    if !read_only {
        tabs.push(SubTab { id: SubTabId::Indexes, label: t("connWin.indexes") });
        tabs.push(SubTab { id: SubTabId::ForeignKeys, label: t("connWin.foreignKeys") });
        tabs.push(SubTab { id: SubTabId::Ddl, label: "DDL".to_owned() });
    }
    tabs
}

pub fn get_view_sub_tabs(t: &dyn Fn(&str) -> String) -> Vec<SubTab> {
    vec![
        SubTab { id: SubTabId::Data, label: t("connWin.data") },
        SubTab { id: SubTabId::Structure, label: t("connWin.structure") },
        SubTab { id: SubTabId::Ddl, label: "DDL".to_owned() },
    ]
}
