//! Port of `windows/connection/useKvSlotActions.ts` — the KV slot action
//! dispatcher — as pure decision logic (T616).
//!
//! React's hook is the single entry point a driver context bar can ask for host
//! capabilities. The three rules that matter are kept here as pure decisions:
//! one dispatcher (one function answers "what can a slot ask for?"), unwired is
//! a no-op plus a developer warning (never a throw), and the dangerous `flushDb`
//! branch passes the shared confirm gate first — Safe Mode hard-blocks, else
//! consent only reaches the warning because running `flush_db` stays the
//! driver's command. The host owns the sinks, the confirm dialog and the
//! settings window; this module returns a [`KvSlotDecision`] for it to carry
//! out. No IPC, promise, store or timer lives here.
//!
//! ## Consumer / removal stage
//!
//! No production host wires this dispatcher yet; the KV panel host will map
//! each decision to its refresh sink / window manager / confirm dialog. Every
//! reusable item carries a per-item `#[allow(dead_code)]` with this stage.

/// React `DRIVER_SETTINGS_SECTION`.
#[allow(dead_code)] // contract constant; consumed by the decision and tests.
pub const DRIVER_SETTINGS_SECTION: &str = "extensions";

/// React `KvSlotAction` from `@datazen/driver-sdk`. The `Unknown` variant stands
/// for a union member this build does not know (React's `default` branch).
#[allow(dead_code)] // VM mirror; supplied by the driver slot.
#[derive(Clone, Debug, PartialEq)]
pub enum KvSlotAction {
    Refresh,
    NewKey,
    Import,
    Export,
    FlushDb,
    OpenMonitor,
    OpenSettings,
    SetScanBudget { value: f64 },
    SelectDatabase { database: String },
    Unknown(String),
}

impl KvSlotAction {
    /// React `action.type` — the string used in the warning and switch.
    #[allow(dead_code)] // pure helper; consumed by the decision and tests.
    pub fn type_name(&self) -> &str {
        match self {
            Self::Refresh => "refresh",
            Self::NewKey => "newKey",
            Self::Import => "import",
            Self::Export => "export",
            Self::FlushDb => "flushDb",
            Self::OpenMonitor => "openMonitor",
            Self::OpenSettings => "openSettings",
            Self::SetScanBudget { .. } => "setScanBudget",
            Self::SelectDatabase { .. } => "selectDatabase",
            Self::Unknown(name) => name,
        }
    }
}

/// React `WIRED_ACTIONS`, in declaration order.
#[allow(dead_code)] // contract mirror; consumed by the warning text and tests.
pub fn wired_actions() -> &'static [&'static str] {
    &["refresh", "openSettings", "selectDatabase"]
}

/// React's developer warning text for an unwired action: it names the action
/// and advertises what *is* wired, so a missing sink is diagnosable.
#[allow(dead_code)] // pure helper; consumed by the host and tests.
pub fn warn_unwired_text(action_type: &str) -> String {
    format!(
        "[kv-slot-actions] no host handler for \"{action_type}\" (wired: {}); ignored. A driver slot may offer it, but nothing will happen until the host wires it.",
        wired_actions().join(", ")
    )
}

/// Which confirm dialog the dangerous branch asks for (React `kind`).
#[allow(dead_code)] // VM mirror; consumed by the host confirm dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmKind {
    Info,
    Warning,
}

/// The shared confirm dialog spec plus whether the branch may proceed.
#[allow(dead_code)] // VM mirror; consumed by the host and tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DangerDialog {
    pub title_key: &'static str,
    pub message_key: &'static str,
    pub confirm_label_key: &'static str,
    pub kind: ConfirmKind,
    /// Safe Mode replaces the prompt with a dismiss-only info dialog; consent
    /// is impossible in that state.
    pub safe_mode: bool,
}

/// React `FLUSH_DB_COPY` keys — no inlined English.
#[allow(dead_code)] // contract mirror; consumed by the gate and tests.
pub const FLUSH_TITLE_KEY: &str = "redis.kvSlot.flushTitle";
#[allow(dead_code)] // contract mirror; consumed by the gate and tests.
pub const FLUSH_MESSAGE_KEY: &str = "redis.kvSlot.flushMessage";
#[allow(dead_code)] // contract mirror; consumed by the gate and tests.
pub const FLUSH_BLOCKED_KEY: &str = "redis.kvSlot.flushBlocked";

/// React `gateDangerous` (PRD I-6): read Safe Mode live at request time. Safe
/// Mode hard-blocks with an info/dismiss dialog; otherwise a warning/confirm
/// dialog asks. Returns the spec the host must show.
#[allow(dead_code)] // pure gate; remove with the host wiring.
pub fn flush_db_gate(safe_mode: bool) -> DangerDialog {
    if safe_mode {
        DangerDialog {
            title_key: "settings.safeMode",
            message_key: FLUSH_BLOCKED_KEY,
            confirm_label_key: "common.dismiss",
            kind: ConfirmKind::Info,
            safe_mode: true,
        }
    } else {
        DangerDialog {
            title_key: FLUSH_TITLE_KEY,
            message_key: FLUSH_MESSAGE_KEY,
            confirm_label_key: "common.confirm",
            kind: ConfirmKind::Warning,
            safe_mode: false,
        }
    }
}

/// What the host must carry out for a slot request.
#[allow(dead_code)] // host effect; remove with the host wiring.
#[derive(Clone, Debug, PartialEq)]
pub enum KvSlotDecision {
    /// Call the panel refresh sink (and nothing else).
    Refresh,
    /// Open a settings window at the given section.
    OpenSettings { section: String },
    /// Activate/open the panel for this database label.
    SelectDatabase { database: String },
    /// Show the dangerous-action confirm dialog before anything else.
    ConfirmDanger { dialog: DangerDialog },
    /// Log the unwired warning; no sink is called and no dialog opens.
    WarnUnwired { action_type: String },
    /// Internal marker for a decision that does nothing (post-gate decline).
    None,
}

/// React `request`'s switch, as a pure decision. `has_select_database` models
/// whether the host threaded `onSelectDatabase` through — without it the
/// `selectDatabase` action degrades to a warning, never a throw. `safe_mode` is
/// read live at request time. The dangerous branch always returns the gate
/// first, so an unwired executor can never skip it.
#[allow(dead_code)] // pure dispatcher; remove with the host wiring.
pub fn dispatch(action: &KvSlotAction, has_select_database: bool, safe_mode: bool) -> KvSlotDecision {
    match action {
        KvSlotAction::Refresh => KvSlotDecision::Refresh,
        KvSlotAction::OpenSettings => KvSlotDecision::OpenSettings {
            section: DRIVER_SETTINGS_SECTION.to_owned(),
        },
        KvSlotAction::SelectDatabase { database } => {
            if has_select_database {
                KvSlotDecision::SelectDatabase {
                    database: database.clone(),
                }
            } else {
                KvSlotDecision::WarnUnwired {
                    action_type: action.type_name().to_owned(),
                }
            }
        }
        KvSlotAction::FlushDb => KvSlotDecision::ConfirmDanger {
            dialog: flush_db_gate(safe_mode),
        },
        KvSlotAction::NewKey
        | KvSlotAction::Import
        | KvSlotAction::Export
        | KvSlotAction::OpenMonitor
        | KvSlotAction::SetScanBudget { .. }
        | KvSlotAction::Unknown(_) => KvSlotDecision::WarnUnwired {
            action_type: action.type_name().to_owned(),
        },
    }
}

/// React `.then(approved => { if (approved) warnUnwired(action) })` after the
/// dangerous gate resolves. Safe Mode or a decline ends with no warning and no
/// sink call; consent only reaches the warning because the host must not name
/// the driver's `flush_db` command itself.
#[allow(dead_code)] // pure post-gate decision; remove with the host wiring.
pub fn flush_db_result(dialog: &DangerDialog, user_confirmed: bool) -> KvSlotDecision {
    if !dialog.safe_mode && user_confirmed {
        KvSlotDecision::WarnUnwired {
            action_type: KvSlotAction::FlushDb.type_name().to_owned(),
        }
    } else {
        KvSlotDecision::None
    }
}
