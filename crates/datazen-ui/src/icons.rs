//! Rail and chrome icons for the DataZen Rune UI.

use crate::types::Section;

// L4: lucide-semantic rail icons (24x24, stroke outline, currentColor).
pub(crate) static ICON_CONNECTIONS: rune::Icon = rune::Icon {
    name: "rail-connections",
    view: 24.0,
    body: r#"<ellipse cx="12" cy="5" rx="9" ry="3" stroke="currentColor" stroke-width="2" fill="none"/><path d="M3 5v14c0 1.66 4.03 3 9 3s9-1.34 9-3V5" stroke="currentColor" stroke-width="2" fill="none"/><path d="M3 12c0 1.66 4.03 3 9 3s9-1.34 9-3" stroke="currentColor" stroke-width="2" fill="none"/>"#,
};
pub(crate) static ICON_WORKSPACE: rune::Icon = rune::Icon {
    name: "rail-workspace",
    view: 24.0,
    body: r#"<rect x="3" y="3" width="18" height="18" rx="2" stroke="currentColor" stroke-width="2" fill="none"/><path d="M3 9h18M9 21V9" stroke="currentColor" stroke-width="2" fill="none"/>"#,
};
pub(crate) static ICON_SETTINGS: rune::Icon = rune::Icon {
    name: "rail-settings",
    view: 24.0,
    body: r#"<circle cx="12" cy="12" r="3" stroke="currentColor" stroke-width="2" fill="none"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1.08-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.6 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6c.32 0 .63-.12.87-.33.3-.27.49-.65.52-1.06V3a2 2 0 1 1 4 0v.09c.03.41.22.79.52 1.06.24.21.55.33.87.33.65 0 1.26-.38 1.54-.97l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06c-.43.46-.55 1.13-.33 1.71.19.5.6.88 1.12 1.03H21a2 2 0 1 1 0 4h-.09c-.41.03-.79.22-1.06.52-.21.24-.33.55-.33.87z" stroke="currentColor" stroke-width="2" fill="none"/>"#,
};
pub(crate) static ICON_TOOLS: rune::Icon = rune::Icon {
    name: "rail-tools",
    view: 24.0,
    body: r#"<path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z" stroke="currentColor" stroke-width="2" fill="none" stroke-linejoin="round"/>"#,
};
pub(crate) static ICON_DASHBOARDS: rune::Icon = rune::Icon {
    name: "rail-dashboards",
    view: 24.0,
    body: r#"<path d="M3 3v18h18" stroke="currentColor" stroke-width="2" fill="none" stroke-linecap="round" stroke-linejoin="round"/><rect x="7" y="12" width="3" height="6" rx="1" stroke="currentColor" stroke-width="2" fill="none"/><rect x="12" y="9" width="3" height="9" rx="1" stroke="currentColor" stroke-width="2" fill="none"/><rect x="17" y="6" width="3" height="12" rx="1" stroke="currentColor" stroke-width="2" fill="none"/>"#,
};
pub(crate) static ICON_WORKFLOWS: rune::Icon = rune::Icon {
    name: "rail-workflows",
    view: 24.0,
    body: r#"<rect x="3" y="3" width="6" height="6" rx="1" stroke="currentColor" stroke-width="2" fill="none"/><rect x="15" y="3" width="6" height="6" rx="1" stroke="currentColor" stroke-width="2" fill="none"/><rect x="9" y="15" width="6" height="6" rx="1" stroke="currentColor" stroke-width="2" fill="none"/><path d="M6 9v3a1 1 0 0 0 1 1h4M18 9v3a1 1 0 0 1-1 1h-4M12 13v2" stroke="currentColor" stroke-width="2" fill="none" stroke-linecap="round"/>"#,
};
pub(crate) static ICON_WAPPS: rune::Icon = rune::Icon {
    name: "rail-wapps",
    view: 24.0,
    body: r#"<rect x="3" y="3" width="7" height="7" rx="1" stroke="currentColor" stroke-width="2" fill="none"/><rect x="14" y="3" width="7" height="7" rx="1" stroke="currentColor" stroke-width="2" fill="none"/><rect x="3" y="14" width="7" height="7" rx="1" stroke="currentColor" stroke-width="2" fill="none"/><path d="M14 17.5h7M17.5 14v7" stroke="currentColor" stroke-width="2" fill="none" stroke-linecap="round"/>"#,
};
pub(crate) static ICON_CHEVRON_LEFT: rune::Icon = rune::Icon {
    name: "rail-collapse",
    view: 16.0,
    body: r#"<path d="M10 4L6 8l4 4" stroke="currentColor" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round"/>"#,
};
pub(crate) static ICON_CHEVRON_RIGHT: rune::Icon = rune::Icon {
    name: "rail-expand",
    view: 16.0,
    body: r#"<path d="M6 4l4 4-4 4" stroke="currentColor" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

pub(crate) fn section_icon(section: Section) -> &'static rune::Icon {
    match section {
        Section::Connections => &ICON_CONNECTIONS,
        Section::Workspace => &ICON_WORKSPACE,
        Section::Settings => &ICON_SETTINGS,
        Section::Tools => &ICON_TOOLS,
        Section::Dashboards => &ICON_DASHBOARDS,
        Section::Workflows => &ICON_WORKFLOWS,
        Section::Wapps => &ICON_WAPPS,
    }
}
