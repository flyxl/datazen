//! Core UI types for the DataZen Rune UI.

use crate::i18n::t;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TableView {
    #[default]
    Data,
    Ddl,
    Columns,
    Indexes,
    ForeignKeys,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Section {
    #[default]
    Connections,
    Workspace,
    Settings,
    Tools,
    Dashboards,
    Workflows,
    Wapps,
}

impl Section {
    pub const ALL: [Self; 7] = [
        Self::Connections,
        Self::Workspace,
        Self::Settings,
        Self::Tools,
        Self::Dashboards,
        Self::Workflows,
        Self::Wapps,
    ];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Connections => "connections",
            Self::Workspace => "workspace",
            Self::Settings => "settings",
            Self::Tools => "tools",
            Self::Dashboards => "dashboards",
            Self::Workflows => "workflows",
            Self::Wapps => "wapps",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            // Align rail/main navigation labels with the React sidebar
            // (packages/frontend/src/windows/connection/WorkspaceModeSidebar.tsx).
            // Tools has no React sidebar entry, so it keeps its literal.
            Self::Connections => t("nav.databases"),
            Self::Workspace => t("nav.workspacePages"),
            Self::Settings => t("nav.settings"),
            Self::Tools => "Tools",
            Self::Dashboards => t("nav.dashboard"),
            Self::Workflows => t("nav.workflow"),
            Self::Wapps => t("nav.extensions"),
        }
    }
}

/// Which data tool the tools page shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    Sync,
    Compare,
    Transfer,
    ValueSearch,
}

/// A3: the panel shown in the query workspace's sidebar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuerySidebarPanel {
    #[default]
    Favorites,
    History,
    Snippets,
}

impl QuerySidebarPanel {
    pub const ALL: [QuerySidebarPanel; 3] = [
        QuerySidebarPanel::Favorites,
        QuerySidebarPanel::History,
        QuerySidebarPanel::Snippets,
    ];

    pub fn label(self) -> &'static str {
        match self {
            QuerySidebarPanel::Favorites => "Favorites",
            QuerySidebarPanel::History => "History",
            QuerySidebarPanel::Snippets => "Snippets",
        }
    }
}

/// A5: how the result workspace presents query results.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResultView {
    #[default]
    Table,
    Chart,
}
