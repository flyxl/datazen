use crate::types::wapp::WappPermission;

pub const API_ROUTES: &[(&str, Option<WappPermission>)] = &[
    (
        "context.getConnections",
        Some(WappPermission::ContextConnections),
    ),
    (
        "context.getActiveConnection",
        Some(WappPermission::ContextConnections),
    ),
    ("command.invoke", Some(WappPermission::CommandInvoke)),
    ("session.open", Some(WappPermission::SessionStateful)),
    ("session.use", Some(WappPermission::SessionStateful)),
    ("session.close", Some(WappPermission::SessionStateful)),
    ("session.receipt", Some(WappPermission::SessionStateful)),
    ("storage.get", Some(WappPermission::StorageLocal)),
    ("storage.set", Some(WappPermission::StorageLocal)),
    ("storage.remove", Some(WappPermission::StorageLocal)),
    ("ui.notify", None),
    ("i18n.getString", None),
];

pub fn route_for(kind: &str) -> Option<Option<WappPermission>> {
    API_ROUTES
        .iter()
        .find(|(name, _)| *name == kind)
        .map(|(_, permission)| *permission)
}

pub const WAPP_COMMAND_DENYLIST: &[&str] = &[
    "execute",
    "create_database",
    "drop_database",
    "create_schema",
    "drop_schema",
    "create_user",
    "drop_user",
    "grant_privileges",
    "revoke_privileges",
];

pub fn is_wapp_command_allowed(command: &str) -> bool {
    !WAPP_COMMAND_DENYLIST.contains(&command)
}
