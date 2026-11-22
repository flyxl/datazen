pub const WORKSPACE_MODE_SIDEBAR_SURFACE: [&str; 8] = [
    "buildSidebarView",
    "buildUpdateView",
    "progressLabel",
    "updateLabel",
    "resolveRestartPrompt",
    "dispatchNavAction",
    "WorkspaceMode",
    "WORKSPACE_MODE_SIDEBAR_GAP",
];

pub const WORKSPACE_MODE_SIDEBAR_GAP: [&str; 6] = [
    "React component tree and themed icon rendering",
    "useUpdateReminderStore subscription and IPC actions",
    "useConfirmDialog modal and pending-restart effect",
    "crypto/random ids and async install/restart timing",
    "i18n t() for nav, updater and toggle labels",
    "NavButton DOM click wiring",
];

pub(crate) use crate::WorkspaceMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavButtonId {
    Databases,
    Workflow,
    Dashboard,
    WorkspacePages,
    Extensions,
    Update,
    Settings,
    SidebarToggle,
}

impl NavButtonId {
    pub fn as_str(self) -> &'static str {
        match self {
            NavButtonId::Databases => "workspace-nav-databases",
            NavButtonId::Workflow => "workspace-nav-workflow",
            NavButtonId::Dashboard => "workspace-nav-dashboard",
            NavButtonId::WorkspacePages => "workspace-nav-workspace-pages",
            NavButtonId::Extensions => "workspace-nav-extensions",
            NavButtonId::Update => "workspace-nav-update",
            NavButtonId::Settings => "workspace-nav-settings",
            NavButtonId::SidebarToggle => "workspace-sidebar-toggle",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavAction {
    SetWorkspaceMode(WorkspaceMode),
    OpenWorkflow,
    OpenDashboard,
    OpenSettings,
    ToggleSidebar,
    InstallUpdate,
    RestartToApply,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateProgressPhase {
    Downloading,
    Installing,
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UpdateProgress {
    pub phase: UpdateProgressPhase,
    pub downloaded: u64,
    pub total: Option<u64>,
}

impl Default for UpdateProgressPhase {
    fn default() -> Self {
        UpdateProgressPhase::Other
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateState {
    pub available_version: Option<String>,
    pub pending_restart_version: Option<String>,
    pub is_installing: bool,
    pub progress: Option<UpdateProgress>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProgressRing {
    pub size: f64,
    pub stroke: f64,
    pub radius: f64,
    pub circumference: f64,
    pub percent: Option<f64>,
    pub spin: bool,
    pub dash_offset: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UpdateButtonView {
    pub test_id: NavButtonId,
    pub class: String,
    pub aria_label: String,
    pub title: String,
    pub disabled: bool,
    pub action: UpdateAction,
    pub expanded: bool,
    pub show_badge: bool,
    pub ring: Option<ProgressRing>,
    pub icon_kind: UpdateIconKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateIconKind {
    Spinner,
    Restart,
    Download,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateAction {
    Install,
    Restart,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NavButton {
    pub id: NavButtonId,
    pub class: String,
    pub icon_id: String,
    pub label: String,
    pub active: bool,
    pub expanded: bool,
    pub action: NavAction,
    pub test_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SidebarView {
    pub class: String,
    pub expanded: bool,
    pub nav_buttons: Vec<NavButton>,
    pub update_button: Option<UpdateButtonView>,
    pub settings_button: NavButton,
    pub toggle_button: NavButton,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestartPrompt {
    pub message: String,
    pub title: String,
    pub confirm_label: String,
    pub cancel_label: String,
    pub kind: String,
}
