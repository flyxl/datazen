use super::types::{ContentViewPanel, PanelIcon, PanelKind};

pub fn get_panel_icon(panel: &ContentViewPanel) -> Option<PanelIcon> {
    match panel.kind {
        PanelKind::Table => Some(PanelIcon::Table),
        PanelKind::View => Some(PanelIcon::View),
        PanelKind::Query => Some(PanelIcon::Query),
        PanelKind::CreateTable => Some(PanelIcon::CreateTable),
        PanelKind::ErDiagram => Some(PanelIcon::ErDiagram),
        PanelKind::Objects => Some(PanelIcon::Objects),
        PanelKind::Privileges => Some(PanelIcon::Privileges),
        PanelKind::ServerStatus => Some(PanelIcon::ServerStatus),
        PanelKind::Processes => Some(PanelIcon::Processes),
        PanelKind::RedisDb => Some(PanelIcon::RedisDb),
        PanelKind::DbObject => Some(match panel.object_kind.as_deref() {
            Some("trigger") => PanelIcon::Trigger,
            Some("procedure") => PanelIcon::Procedure,
            Some("sequence") => PanelIcon::Sequence,
            Some("type") => PanelIcon::Type,
            _ => PanelIcon::Function,
        }),
        PanelKind::Unknown => None,
    }
}
