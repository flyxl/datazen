//! Core UI types for the DataZen Rune UI.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TableView {
    #[default]
    Data,
    Ddl,
    Columns,
    Indexes,
    ForeignKeys,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Section {
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
    pub(crate) const ALL: [Self; 7] = [
        Self::Connections,
        Self::Workspace,
        Self::Settings,
        Self::Tools,
        Self::Dashboards,
        Self::Workflows,
        Self::Wapps,
    ];

    pub(crate) const fn key(self) -> &'static str {
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

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Connections => "Connections",
            Self::Workspace => "Workspace",
            Self::Settings => "Settings",
            Self::Tools => "Tools",
            Self::Dashboards => "Dashboards",
            Self::Workflows => "Workflows",
            Self::Wapps => "WAPPS",
        }
    }
}

/// Which data tool the tools page shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Tool {
    #[default]
    Sync,
    Compare,
    Transfer,
    ValueSearch,
}

/// A3: the panel shown in the query workspace's sidebar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum QuerySidebarPanel {
    #[default]
    Favorites,
    History,
    Snippets,
}

impl QuerySidebarPanel {
    pub(crate) const ALL: [QuerySidebarPanel; 3] = [
        QuerySidebarPanel::Favorites,
        QuerySidebarPanel::History,
        QuerySidebarPanel::Snippets,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            QuerySidebarPanel::Favorites => "Favorites",
            QuerySidebarPanel::History => "History",
            QuerySidebarPanel::Snippets => "Snippets",
        }
    }
}
