pub const USE_QUERY_BUILDER_CONTRIBUTION_GAP: [&str; 4] = [
    "useExtension(sqlEditorEnhancedEP) registry read",
    "useSyncExternalStore subscribe/getSnapshot/getServerSnapshot runtime",
    "the React re-render that follows a listener notification",
    "the contribution's subscribe/listener registration lifecycle",
];

pub const USE_QUERY_BUILDER_CONTRIBUTION_SURFACE: [&str; 4] = [
    "useQueryBuilderContribution",
    "contribution",
    "openPanelId",
    "getClosedSnapshot",
];

pub trait QueryBuilderContribution {
    fn get_open_panel_id(&self) -> Option<String>;
}

pub fn get_closed_snapshot() -> Option<String> {
    None
}

pub fn subscribe_falls_back_to_noop(
    contribution: Option<&dyn QueryBuilderContribution>,
) -> bool {
    contribution.is_none()
}

pub fn client_snapshot(contribution: Option<&dyn QueryBuilderContribution>) -> Option<String> {
    contribution.and_then(|contribution| contribution.get_open_panel_id())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryBuilderRunState {
    pub has_contribution: bool,
    pub open_panel_id: Option<String>,
}

impl QueryBuilderRunState {
    pub fn resolve(contribution: Option<&dyn QueryBuilderContribution>) -> Self {
        Self {
            has_contribution: contribution.is_some(),
            open_panel_id: client_snapshot(contribution),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Installed {
        panel: Option<String>,
    }

    impl QueryBuilderContribution for Installed {
        fn get_open_panel_id(&self) -> Option<String> {
            self.panel.clone()
        }
    }

    #[test]
    fn nothing_installed_reads_closed_on_both_snapshots() {
        assert_eq!(get_closed_snapshot(), None);
        assert!(subscribe_falls_back_to_noop(None));
        assert_eq!(client_snapshot(None), None);
        let state = QueryBuilderRunState::resolve(None);
        assert!(!state.has_contribution);
        assert_eq!(state.open_panel_id, None);
    }

    #[test]
    fn an_installed_contribution_supplies_the_client_snapshot() {
        let installed = Installed { panel: Some("panel-1".to_owned()) };
        assert!(!subscribe_falls_back_to_noop(Some(&installed)));
        assert_eq!(client_snapshot(Some(&installed)).as_deref(), Some("panel-1"));
        let state = QueryBuilderRunState::resolve(Some(&installed));
        assert!(state.has_contribution);
        assert_eq!(state.open_panel_id.as_deref(), Some("panel-1"));
    }

    #[test]
    fn an_installed_but_closed_builder_is_distinct_from_a_missing_one() {
        let closed = Installed { panel: None };
        let state = QueryBuilderRunState::resolve(Some(&closed));
        assert!(state.has_contribution, "the contribution is still installed");
        assert_eq!(state.open_panel_id, None, "its panel is simply not open");
        assert_eq!(state, QueryBuilderRunState {
            has_contribution: true,
            open_panel_id: None,
        });
        assert_eq!(get_closed_snapshot(), None, "the server snapshot stays closed regardless");
    }

    #[test]
    fn surface_and_gap_are_pinned() {
        assert_eq!(USE_QUERY_BUILDER_CONTRIBUTION_SURFACE.len(), 4);
        assert_eq!(
            USE_QUERY_BUILDER_CONTRIBUTION_SURFACE,
            ["useQueryBuilderContribution", "contribution", "openPanelId", "getClosedSnapshot"]
        );
        assert_eq!(USE_QUERY_BUILDER_CONTRIBUTION_GAP.len(), 4);
        assert_eq!(crate::USE_QUERY_BUILDER_CONTRIBUTION_GAP.len(), 4);
        assert_eq!(crate::USE_QUERY_BUILDER_CONTRIBUTION_SURFACE.len(), 4);
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::get_closed_snapshot;
        let _ = crate::client_snapshot;
        let _ = crate::subscribe_falls_back_to_noop;
        let _ = crate::QueryBuilderRunState::resolve;
        let _: Option<crate::QueryBuilderRunState> = None;
    }
}
