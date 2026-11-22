//! Port of `windows/connection/WorkspaceModeSidebar.tsx`.
//!
//! The workspace rail: five mode shortcuts (databases, workflow, dashboard,
//! workspace pages, extensions), a settings shortcut, a collapse/expand toggle
//! and an optional update reminder that sits just above settings. Expanded mode
//! shows labels; collapsed mode is icon-only. The active mode is highlighted.
//! Every click is an injected [`WorkspaceModeAction`] the host executes — no
//! fake IPC, timer or store write.
//!
//! ## Keys
//!
//! React `data-testid`s are kept verbatim: `workspace-nav-databases`,
//! `workspace-nav-workflow`, `workspace-nav-dashboard`,
//! `workspace-nav-workspace-pages`, `workspace-nav-extensions`,
//! `workspace-nav-update`, `workspace-nav-settings` and
//! `workspace-sidebar-toggle`.
//!
//! ## Local stand-ins (removal stage)
//!
//! The VM mirrors the props and the update-reminder store slice; no production
//! host mounts this rail yet. Every reusable item carries a per-item
//! `#[allow(dead_code)]` with removal stage "drop when the connection
//! workspace host wires this sidebar".

use std::rc::Rc;

use rune::{col, icon, row, text, El, Icon};
use rune_kit::Kit;

use crate::i18n::t;
use crate::icons::{
    ICON_CONNECTIONS, ICON_DASHBOARDS, ICON_DOWNLOAD, ICON_LAYOUT, ICON_PANEL_LEFT_CLOSE,
    ICON_PANEL_LEFT_OPEN, ICON_REFRESH, ICON_SETTINGS, ICON_WAPPS, ICON_WORKFLOWS,
};
use crate::ui::kit::kit;

pub(crate) const ROOT_KEY: &str = "workspace-mode-sidebar";
pub(crate) const NAV_DATABASES: &str = "workspace-nav-databases";
pub(crate) const NAV_WORKFLOW: &str = "workspace-nav-workflow";
pub(crate) const NAV_DASHBOARD: &str = "workspace-nav-dashboard";
pub(crate) const NAV_WORKSPACE_PAGES: &str = "workspace-nav-workspace-pages";
pub(crate) const NAV_EXTENSIONS: &str = "workspace-nav-extensions";
pub(crate) const NAV_UPDATE: &str = "workspace-nav-update";
pub(crate) const NAV_SETTINGS: &str = "workspace-nav-settings";
pub(crate) const SIDEBAR_TOGGLE: &str = "workspace-sidebar-toggle";

/// React `WorkspaceMode`.
#[allow(dead_code)] // VM mirror; remove when the host owns the mode store.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WorkspaceMode {
    #[default]
    Connections,
    Workflow,
    Dashboard,
    Workspace,
    Extension,
}

/// React's update progress phases (downloading / installing).
#[allow(dead_code)] // VM mirror; remove when the updater host lands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct UpdateProgressVm {
    pub(crate) phase_downloading: bool,
    pub(crate) phase_installing: bool,
    pub(crate) downloaded: u64,
    pub(crate) total: u64,
}

/// The update-reminder store fields the rail reads.
#[allow(dead_code)] // VM mirror; remove when the updater host lands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct UpdateReminderVm {
    pub(crate) available_version: Option<String>,
    pub(crate) pending_restart_version: Option<String>,
    pub(crate) is_installing: bool,
    pub(crate) progress: Option<UpdateProgressVm>,
    pub(crate) error: Option<String>,
}

/// Props plus the update store slice.
#[allow(dead_code)] // VM surface; remove when the host wires the rail.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkspaceModeSidebarVm {
    pub(crate) workspace_mode: WorkspaceMode,
    pub(crate) sidebar_expanded: bool,
    pub(crate) update: UpdateReminderVm,
}

#[allow(dead_code)] // pure helpers; remove with the host.
impl WorkspaceModeSidebarVm {
    /// React's update button presence: a truthy available or pending-restart
    /// version string (an empty string is falsy in JS; whitespace stays truthy).
    pub(crate) fn update_visible(&self) -> bool {
        self.update
            .available_version
            .as_deref()
            .is_some_and(|version| !version.is_empty())
            || self
                .update
                .pending_restart_version
                .as_deref()
                .is_some_and(|version| !version.is_empty())
    }

    /// React `progressLabel`: downloading / installing / generic installing.
    pub(crate) fn progress_label(&self) -> String {
        match self.update.progress.as_ref() {
            Some(progress) if progress.phase_downloading => {
                if progress.total > 0 {
                    // React Math.round((downloaded / total) * 100), then clamp.
                    let pct = ((progress.downloaded as f64 / progress.total as f64) * 100.0)
                        .round()
                        as i64;
                    let pct = pct.min(100);
                    t("settings.updater.downloading").replace("{pct}", &pct.to_string())
                } else {
                    t("settings.updater.downloadingIndeterminate").to_owned()
                }
            }
            Some(progress) if progress.phase_installing => t("settings.updater.installing").to_owned(),
            _ => t("settings.updater.sidebarInstalling").to_owned(),
        }
    }

    /// React `updateLabel` / `aria-label` of the update button.
    pub(crate) fn update_label(&self) -> String {
        if self.update.is_installing {
            self.progress_label()
        } else if let Some(version) = self.update.pending_restart_version.as_deref() {
            t("settings.updater.sidebarRestartReady").replace("{version}", version)
        } else {
            let version = self
                .update
                .available_version
                .as_deref()
                .or(self.update.pending_restart_version.as_deref())
                .unwrap_or("");
            t("settings.updater.sidebarAvailable").replace("{version}", version)
        }
    }

    /// React `title={error ?? updateLabel}`.
    pub(crate) fn update_title(&self) -> String {
        self.update.error.clone().unwrap_or_else(|| self.update_label())
    }
}

/// Every interaction the rail emits.
#[allow(dead_code)] // host port; remove when the host wires the rail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceModeAction {
    SetMode(WorkspaceMode),
    OpenWorkflow,
    OpenDashboard,
    OpenSettings,
    ToggleSidebar,
    InstallUpdate,
    RestartToApply,
}

#[allow(dead_code)] // host port; remove when the host wires the rail.
pub(crate) type WorkspaceModeActionFn<S> = Rc<dyn Fn(&mut S, WorkspaceModeAction) + 'static>;

fn emit<S: 'static>(
    on_action: &Option<WorkspaceModeActionFn<S>>,
    action: WorkspaceModeAction,
) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action);
        }
    }
}

/// A mode shortcut: `ThemedIcon` + label when expanded, icon only otherwise.
fn mode_button<S: 'static>(
    key: &str,
    label: &str,
    glyph: &'static Icon,
    active: bool,
    expanded: bool,
    on_action: &Option<WorkspaceModeActionFn<S>>,
    action: WorkspaceModeAction,
    dark: bool,
) -> El {
    let k: &Kit = kit(dark);
    let mut button = row()
        .key(key.to_owned())
        .role("button")
        .label(label.to_owned())
        .help(label.to_owned())
        .focusable(true)
        .w_full()
        .h(40.0)
        .gap(10.0)
        .center_y()
        .px(if expanded { 12.0 } else { 0.0 })
        .fill(if active {
            k.alpha(k.c.text_accent, 0.20)
        } else {
            rune::Color::TRANSPARENT
        })
        .child(
            icon(glyph, 16.0)
                .color(if active { k.c.text_accent } else { k.c.text_muted })
                .a11y_hidden(),
        );
    if expanded {
        button = button.child(
            text(label.to_owned())
                .key(format!("{key}-label"))
                .font(k.ty.text_small)
                .color(if active { k.c.text_accent } else { k.c.text_muted })
                .truncate(),
        );
    }
    let press = emit(on_action, action);
    button.on_press(move |s: &mut S| press(s))
}

/// Build the rail into `layer`.
#[allow(dead_code)] // view entry; remove when the host wires the rail.
pub(crate) fn build<S: 'static>(
    vm: &WorkspaceModeSidebarVm,
    on_action: &Option<WorkspaceModeActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    let expanded = vm.sidebar_expanded;
    let mut root = col()
        .key(ROOT_KEY)
        .h_full()
        .w(if expanded { 112.0 } else { 40.0 })
        .shrink()
        .fill(k.c.layer_02);

    let mut modes = col().key("workspace-mode-list").w_full().grow(1.0);
    modes = modes.child(mode_button::<S>(
        NAV_DATABASES,
        t("nav.databases"),
        &ICON_CONNECTIONS,
        vm.workspace_mode == WorkspaceMode::Connections,
        expanded,
        on_action,
        WorkspaceModeAction::SetMode(WorkspaceMode::Connections),
        dark,
    ));
    modes = modes.child(mode_button::<S>(
        NAV_WORKFLOW,
        t("nav.workflow"),
        &ICON_WORKFLOWS,
        vm.workspace_mode == WorkspaceMode::Workflow,
        expanded,
        on_action,
        WorkspaceModeAction::OpenWorkflow,
        dark,
    ));
    modes = modes.child(mode_button::<S>(
        NAV_DASHBOARD,
        t("nav.dashboard"),
        &ICON_DASHBOARDS,
        vm.workspace_mode == WorkspaceMode::Dashboard,
        expanded,
        on_action,
        WorkspaceModeAction::OpenDashboard,
        dark,
    ));
    modes = modes.child(mode_button::<S>(
        NAV_WORKSPACE_PAGES,
        t("nav.workspacePages"),
        &ICON_LAYOUT,
        vm.workspace_mode == WorkspaceMode::Workspace,
        expanded,
        on_action,
        WorkspaceModeAction::SetMode(WorkspaceMode::Workspace),
        dark,
    ));
    modes = modes.child(mode_button::<S>(
        NAV_EXTENSIONS,
        t("nav.extensions"),
        &ICON_WAPPS,
        vm.workspace_mode == WorkspaceMode::Extension,
        expanded,
        on_action,
        WorkspaceModeAction::SetMode(WorkspaceMode::Extension),
        dark,
    ));
    root = root.child(modes);

    let mut footer = col().key("workspace-mode-footer").w_full();
    if vm.update_visible() {
        let update_glyph: &'static Icon = if vm.update.is_installing {
            &ICON_REFRESH
        } else if vm.update.pending_restart_version.is_some() {
            &ICON_REFRESH
        } else {
            &ICON_DOWNLOAD
        };
        let label = vm.update_label();
        let mut update = row()
            .key(NAV_UPDATE)
            .role("button")
            .focusable(!vm.update.is_installing)
            .disabled(vm.update.is_installing)
            .label(label.clone())
            .help(vm.update_title())
            .w_full()
            .h(40.0)
            .gap(10.0)
            .center_y()
            .px(if expanded { 12.0 } else { 0.0 })
            .child(icon(update_glyph, 16.0).color(k.c.text_accent).a11y_hidden());
        if expanded {
            update = update.child(
                text(label)
                    .key("workspace-nav-update-label")
                    .font(k.ty.text_small)
                    .color(k.c.text_accent)
                    .truncate(),
            );
        }
        if !vm.update.is_installing {
            let action = if vm.update.pending_restart_version.is_some() {
                WorkspaceModeAction::RestartToApply
            } else {
                WorkspaceModeAction::InstallUpdate
            };
            let press = emit(on_action, action);
            update = update.on_press(move |s: &mut S| press(s));
        }
        footer = footer.child(update);
    }
    footer = footer.child(mode_button::<S>(
        NAV_SETTINGS,
        t("nav.settings"),
        &ICON_SETTINGS,
        false,
        expanded,
        on_action,
        WorkspaceModeAction::OpenSettings,
        dark,
    ));

    let toggle_label = if expanded {
        t("connWin.collapseSidebar")
    } else {
        t("connWin.expandSidebar")
    };
    let toggle_glyph: &'static Icon = if expanded {
        &ICON_PANEL_LEFT_CLOSE
    } else {
        &ICON_PANEL_LEFT_OPEN
    };
    let mut toggle = row()
        .key(SIDEBAR_TOGGLE)
        .role("button")
        .focusable(true)
        .label(toggle_label.to_owned())
        .help(toggle_label.to_owned())
        .w_full()
        .h(40.0)
        .gap(10.0)
        .center_y()
        .px(if expanded { 12.0 } else { 0.0 })
        .child(icon(toggle_glyph, 16.0).color(k.c.text_muted).a11y_hidden());
    if expanded {
        toggle = toggle.child(
            text(toggle_label)
                .key("workspace-sidebar-toggle-label")
                .font(k.ty.text_small)
                .color(k.c.text_muted)
                .truncate(),
        );
    }
    let press = emit(on_action, WorkspaceModeAction::ToggleSidebar);
    footer = footer.child(toggle.on_press(move |s: &mut S| press(s)));
    layer.push(root.child(footer));
}
