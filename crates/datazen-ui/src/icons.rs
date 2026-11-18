//! Rail and chrome icons for the DataZen Rune UI.
//!
//! Chrome draws lucide icons: a 24 viewBox with stroke outlines, so the native shell matches the
//! React UI's visual language. The kit's own 16-view `rune_kit::glyphs` stay inside kit
//! components; they are not the chrome set. This file is the only place a chrome icon's SVG body
//! may live — add new icons here, reference the constant at the call site, and keep every body
//! unique (the registry guard in `tests/icon_registry.rs` fails on an inline definition).

use crate::types::Section;

// Lucide-semantic icons (24x24, stroke outline, currentColor).
pub(crate) static ICON_CONNECTIONS: rune::Icon = rune::Icon {
    name: "database",
    view: 24.0,
    body: "<ellipse stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" cx=\"12\" cy=\"5\" rx=\"9\" ry=\"3\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M3 5V19A9 3 0 0 0 21 19V5\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M3 12A9 3 0 0 0 21 12\"/>",
};
pub(crate) static ICON_WORKFLOWS: rune::Icon = rune::Icon {
    name: "workflow",
    view: 24.0,
    body: "<rect stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" width=\"8\" height=\"8\" x=\"3\" y=\"3\" rx=\"2\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M7 11v4a2 2 0 0 0 2 2h4\"/><rect stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" width=\"8\" height=\"8\" x=\"13\" y=\"13\" rx=\"2\"/>",
};
pub(crate) static ICON_DASHBOARDS: rune::Icon = rune::Icon {
    name: "gauge",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"m12 14 4-4\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M3.34 19a10 10 0 1 1 17.32 0\"/>",
};
pub(crate) static ICON_WORKSPACE: rune::Icon = rune::Icon {
    name: "layout-grid",
    view: 24.0,
    body: "<rect stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" width=\"7\" height=\"7\" x=\"3\" y=\"3\" rx=\"1\"/><rect stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" width=\"7\" height=\"7\" x=\"14\" y=\"3\" rx=\"1\"/><rect stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" width=\"7\" height=\"7\" x=\"14\" y=\"14\" rx=\"1\"/><rect stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" width=\"7\" height=\"7\" x=\"3\" y=\"14\" rx=\"1\"/>",
};
pub(crate) static ICON_WAPPS: rune::Icon = rune::Icon {
    name: "puzzle",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M15.39 4.39a1 1 0 0 0 1.68-.474 2.5 2.5 0 1 1 3.014 3.015 1 1 0 0 0-.474 1.68l1.683 1.682a2.414 2.414 0 0 1 0 3.414L19.61 15.39a1 1 0 0 1-1.68-.474 2.5 2.5 0 1 0-3.014 3.015 1 1 0 0 1 .474 1.68l-1.683 1.682a2.414 2.414 0 0 1-3.414 0L8.61 19.61a1 1 0 0 0-1.68.474 2.5 2.5 0 1 1-3.014-3.015 1 1 0 0 0 .474-1.68l-1.683-1.682a2.414 2.414 0 0 1 0-3.414L4.39 8.61a1 1 0 0 1 1.68.474 2.5 2.5 0 1 0 3.014-3.015 1 1 0 0 1-.474-1.68l1.683-1.682a2.414 2.414 0 0 1 3.414 0z\"/>",
};
pub(crate) static ICON_SETTINGS: rune::Icon = rune::Icon {
    name: "settings",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z\"/><circle stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" cx=\"12\" cy=\"12\" r=\"3\"/>",
};
pub(crate) static ICON_TOOLS: rune::Icon = rune::Icon {
    name: "wrench",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z\"/>",
};
pub(crate) static ICON_PANEL_LEFT_CLOSE: rune::Icon = rune::Icon {
    name: "panel-left-close",
    view: 24.0,
    body: "<rect stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" width=\"18\" height=\"18\" x=\"3\" y=\"3\" rx=\"2\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M9 3v18\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"m16 15-3-3 3-3\"/>",
};
pub(crate) static ICON_PANEL_LEFT_OPEN: rune::Icon = rune::Icon {
    name: "panel-left-open",
    view: 24.0,
    body: "<rect stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" width=\"18\" height=\"18\" x=\"3\" y=\"3\" rx=\"2\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M9 3v18\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"m14 9 3 3-3 3\"/>",
};
pub(crate) static ICON_X: rune::Icon = rune::Icon {
    name: "x",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M18 6 6 18\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"m6 6 12 12\"/>",
};
pub(crate) static ICON_SEARCH: rune::Icon = rune::Icon {
    name: "search",
    view: 24.0,
    body: "<circle stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" cx=\"11\" cy=\"11\" r=\"8\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"m21 21-4.3-4.3\"/>",
};
pub(crate) static ICON_DOWNLOAD: rune::Icon = rune::Icon {
    name: "download",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4\"/><polyline stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" points=\"7 10 12 15 17 10\"/><line stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" x1=\"12\" x2=\"12\" y1=\"15\" y2=\"3\"/>",
};
pub(crate) static ICON_PLUS: rune::Icon = rune::Icon {
    name: "plus",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M5 12h14\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M12 5v14\"/>",
};
pub(crate) static ICON_CLOCK: rune::Icon = rune::Icon {
    name: "clock",
    view: 24.0,
    body: "<circle stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" cx=\"12\" cy=\"12\" r=\"10\"/><polyline stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" points=\"12 6 12 12 16 14\"/>",
};
pub(crate) static ICON_CHEVRON_RIGHT: rune::Icon = rune::Icon {
    name: "chevron-right",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"m9 18 6-6-6-6\"/>",
};
pub(crate) static ICON_CHEVRON_DOWN: rune::Icon = rune::Icon {
    name: "chevron-down",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"m6 9 6 6 6-6\"/>",
};
pub(crate) static ICON_FOLDER: rune::Icon = rune::Icon {
    name: "folder",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z\"/>",
};
pub(crate) static ICON_FOLDER_OPEN: rune::Icon = rune::Icon {
    name: "folder-open",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2\"/>",
};
pub(crate) static ICON_TABLE: rune::Icon = rune::Icon {
    name: "table",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M12 3v18\"/><rect stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" width=\"18\" height=\"18\" x=\"3\" y=\"3\" rx=\"2\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M3 9h18\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M3 15h18\"/>",
};
pub(crate) static ICON_REFRESH: rune::Icon = rune::Icon {
    name: "refresh-cw",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M21 3v5h-5\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M8 16H3v5\"/>",
};
pub(crate) static ICON_LAYOUT: rune::Icon = rune::Icon {
    name: "layout",
    view: 24.0,
    body: "<rect stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" width=\"18\" height=\"18\" x=\"3\" y=\"3\" rx=\"2\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M3 9h18\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M9 21V9\"/>",
};
pub(crate) static ICON_PLAY: rune::Icon = rune::Icon {
    name: "play",
    view: 24.0,
    body: "<polygon stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" points=\"6 3 20 12 6 21 6 3\"/>",
};
pub(crate) static ICON_COPY: rune::Icon = rune::Icon {
    name: "copy",
    view: 24.0,
    body: "<rect stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" width=\"14\" height=\"14\" x=\"8\" y=\"8\" rx=\"2\" ry=\"2\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2\"/>",
};
pub(crate) static ICON_TERMINAL: rune::Icon = rune::Icon {
    name: "terminal",
    view: 24.0,
    body: "<polyline stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" points=\"4 17 10 11 4 5\"/><line stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" x1=\"12\" x2=\"20\" y1=\"19\" y2=\"19\"/>",
};
pub(crate) static ICON_SPARKLES: rune::Icon = rune::Icon {
    name: "sparkles",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"m12 3-1.912 5.813a2 2 0 0 1-1.275 1.275L3 12l5.813 1.912a2 2 0 0 1 1.275 1.275L12 21l1.912-5.813a2 2 0 0 1 1.275-1.275L21 12l-5.813-1.912a2 2 0 0 1-1.275-1.275Z\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M5 3v4\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M19 17v4\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M3 5h4\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M17 19h4\"/>",
};

pub(crate) static ICON_CHEVRON_LEFT: rune::Icon = rune::Icon {
    name: "chevron-left",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"m15 18-6-6 6-6\"/>",
};
pub(crate) static ICON_ARROW_UP: rune::Icon = rune::Icon {
    name: "arrow-up",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"m5 12 7-7 7 7\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M12 19V5\"/>",
};
pub(crate) static ICON_ARROW_DOWN: rune::Icon = rune::Icon {
    name: "arrow-down",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M12 5v14\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"m19 12-7 7-7-7\"/>",
};
pub(crate) static ICON_FILTER: rune::Icon = rune::Icon {
    name: "filter",
    view: 24.0,
    body: "<polygon stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" points=\"22 3 2 3 10 12.46 10 19 14 21 14 12.46 22 3\"/>",
};
pub(crate) static ICON_CHECK: rune::Icon = rune::Icon {
    name: "check",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M20 6 9 17l-5-5\"/>",
};
pub(crate) static ICON_STAR: rune::Icon = rune::Icon {
    name: "star",
    view: 24.0,
    body: "<polygon stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" points=\"12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2\"/>",
};
pub(crate) static ICON_EXTERNAL_LINK: rune::Icon = rune::Icon {
    name: "external-link",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M15 3h6v6\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M10 14 21 3\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6\"/>",
};
pub(crate) static ICON_TRASH: rune::Icon = rune::Icon {
    name: "trash-2",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M3 6h18\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2\"/><line stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" x1=\"10\" x2=\"10\" y1=\"11\" y2=\"17\"/><line stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" x1=\"14\" x2=\"14\" y1=\"11\" y2=\"17\"/>",
};
// Lucide "pencil" 0.487.0 (T015's edit affordance, the registry addition of
// that port; body data from the pinned lucide-static package).
pub(crate) static ICON_PENCIL: rune::Icon = rune::Icon {
    name: "pencil",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"m15 5 4 4\"/>",
};

pub(crate) static ICON_LOADER2: rune::Icon = rune::Icon {
    name: "loader-2",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M21 12a9 9 0 1 1-6.219-8.56\"/>",
};
pub(crate) static ICON_RING_ARC: rune::Icon = rune::Icon {
    name: "spinner-ring-arc",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2.5\" fill=\"none\" stroke-linecap=\"round\" d=\"M12 3a9 9 0 1 1-9 9\"/>",
};

// Lucide "triangle-alert" 0.487.0 (the confirm dialog's warning glyph, the
// registry addition of the T038 port; React imports it under the deprecated
// `AlertTriangle` alias — same body; body data from the pinned
// lucide-static package).
pub(crate) static ICON_ALERT_TRIANGLE: rune::Icon = rune::Icon {
    name: "triangle-alert",
    view: 24.0,
    body: "<path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M12 9v4\"/><path stroke=\"currentColor\" stroke-width=\"2\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M12 17h.01\"/>",
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
