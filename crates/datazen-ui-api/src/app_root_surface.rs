#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppWindowRoute {
    pub kind: &'static str,
    pub component: &'static str,
    pub hides_splash: bool,
}

pub const APP_WINDOW_ROUTES: [AppWindowRoute; 6] = [
    AppWindowRoute {
        kind: "onboarding",
        component: "OnboardingWindow",
        hides_splash: true,
    },
    AppWindowRoute {
        kind: "data-sync",
        component: "DataSyncWindow",
        hides_splash: true,
    },
    AppWindowRoute {
        kind: "data-transfer",
        component: "DataTransferWindow",
        hides_splash: true,
    },
    AppWindowRoute {
        kind: "schema-diff",
        component: "SchemaDiffWindow",
        hides_splash: true,
    },
    AppWindowRoute {
        kind: "backup",
        component: "BackupWindow",
        hides_splash: true,
    },
    AppWindowRoute {
        kind: "main",
        component: "MainPage",
        hides_splash: false,
    },
];

pub const APP_DEFAULT_WINDOW_KIND: &str = "main";
pub const APP_DEFAULT_COMPONENT: &str = "MainPage";

pub fn resolve_window_component(kind: &str) -> &'static str {
    for route in APP_WINDOW_ROUTES.iter() {
        if route.kind == kind {
            return route.component;
        }
    }
    APP_DEFAULT_COMPONENT
}

pub fn should_hide_splash(kind: &str) -> bool {
    for route in APP_WINDOW_ROUTES.iter() {
        if route.kind == kind {
            return route.hides_splash;
        }
    }
    false
}

pub fn app_module_mark(kind: &str) -> String {
    format!("windowKind resolved: \"{kind}\"")
}

pub const APP_MOUNT_MARK: &str = "window component mounted";

pub const APP_CHUNK_LOAD_MARKS: [&str; 6] = [
    "MainPage chunk loaded",
    "DataSyncWindow chunk loaded",
    "DataTransferWindow chunk loaded",
    "SchemaDiffWindow chunk loaded",
    "BackupWindow chunk loaded",
    "OnboardingWindow chunk loaded",
];

pub const APP_SHELL_WRAPPERS: [&str; 3] = ["ErrorBoundary", "Suspense", "WindowChromeFallback"];

pub const APP_GLOBAL_HOSTS: [&str; 3] = [
    "WebContextMenuHost",
    "McpApprovalHost",
    "AiWriteProposalHost",
];

pub const APP_SELECTION_INSTALLERS: [&str; 4] = [
    "installGlobalTextSelectionPolicy",
    "installRightDragSelectionSuppressor",
    "installDragSelectionGuard",
    "installNativeContextMenuSuppressor",
];

pub const APP_DOM_GAP: [&str; 8] = [
    "React lazy / Suspense chunk loading",
    "document.getElementById('splash') + hideSplash",
    "getWindowKind() URL ?window= resolution",
    "ErrorBoundary render-crash wrapper",
    "WebContextMenuHost / McpApprovalHost / AiWriteProposalHost portals",
    "globalTextSelection document listeners",
    "performance.now() startup marks",
    "runnable root composition lives in datazen-ui::views::app_root",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_route_table_is_exactly_the_switch() {
        assert_eq!(APP_WINDOW_ROUTES.len(), 6);
        assert_eq!(APP_WINDOW_ROUTES[0].kind, "onboarding");
        assert_eq!(APP_WINDOW_ROUTES[0].component, "OnboardingWindow");
        assert_eq!(APP_WINDOW_ROUTES[5].kind, "main");
        assert_eq!(APP_WINDOW_ROUTES[5].component, "MainPage");
        assert_eq!(APP_DEFAULT_WINDOW_KIND, "main");
        assert_eq!(APP_DEFAULT_COMPONENT, "MainPage");
        assert_eq!(APP_MOUNT_MARK, "window component mounted");
    }

    #[test]
    fn router_maps_every_arm_and_falls_back_to_main() {
        for route in APP_WINDOW_ROUTES.iter() {
            assert_eq!(resolve_window_component(route.kind), route.component);
        }
        assert_eq!(resolve_window_component("unknown"), "MainPage");
        assert_eq!(resolve_window_component(""), "MainPage");
    }

    #[test]
    fn only_non_main_windows_hide_the_splash() {
        for route in APP_WINDOW_ROUTES.iter() {
            assert_eq!(should_hide_splash(route.kind), route.hides_splash);
        }
        assert!(should_hide_splash("data-sync"));
        assert!(!should_hide_splash("main"));
        assert!(!should_hide_splash("unknown"));
    }

    #[test]
    fn module_scope_mark_embeds_the_resolved_kind() {
        assert_eq!(
            app_module_mark("data-sync"),
            "windowKind resolved: \"data-sync\""
        );
    }

    #[test]
    fn chunk_marks_follow_the_lazy_declaration_order() {
        assert_eq!(
            APP_CHUNK_LOAD_MARKS,
            [
                "MainPage chunk loaded",
                "DataSyncWindow chunk loaded",
                "DataTransferWindow chunk loaded",
                "SchemaDiffWindow chunk loaded",
                "BackupWindow chunk loaded",
                "OnboardingWindow chunk loaded",
            ]
        );
    }

    #[test]
    fn wrapper_host_and_installer_surfaces_are_pinned() {
        assert_eq!(APP_SHELL_WRAPPERS.len(), 3);
        assert_eq!(APP_SHELL_WRAPPERS[0], "ErrorBoundary");
        assert_eq!(APP_SHELL_WRAPPERS[2], "WindowChromeFallback");
        assert_eq!(APP_GLOBAL_HOSTS.len(), 3);
        assert_eq!(APP_GLOBAL_HOSTS[0], "WebContextMenuHost");
        assert_eq!(APP_GLOBAL_HOSTS[2], "AiWriteProposalHost");
        assert_eq!(APP_SELECTION_INSTALLERS.len(), 4);
        assert_eq!(
            APP_SELECTION_INSTALLERS[0],
            "installGlobalTextSelectionPolicy"
        );
        assert_eq!(
            APP_SELECTION_INSTALLERS[3],
            "installNativeContextMenuSuppressor"
        );
    }

    #[test]
    fn dom_gap_is_pinned_and_reachable_from_the_crate_root() {
        assert_eq!(APP_DOM_GAP.len(), 8);
        assert!(APP_DOM_GAP.contains(&"React lazy / Suspense chunk loading"));
        assert!(APP_DOM_GAP.contains(&"performance.now() startup marks"));
        assert_eq!(crate::APP_DOM_GAP.len(), 8);
    }
}
