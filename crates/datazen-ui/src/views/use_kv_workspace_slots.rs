//! `windows/connection/useKvWorkspaceSlots.ts` — KV workspace slot bindings,
//! ported as a pure decision module for T618.
//!
//! React resolves, for the active connection-workspace state, which of the
//! four driver-contributable KV surfaces actually exist and freezes the props
//! each one gets. A binding appears only when BOTH gates pass: the driver's
//! `kvWorkspace` meta declares the slot, and this build registered a component
//! for it. Failing either gate yields "render today's default UI", never an
//! error. The host owns the component registry, the per-panel state atoms and
//! the panel-open callback — this module only decides identity, props and
//! lifecycle; it does no IPC, React rendering or timer work.
//!
//! Also ported: the two helpers the hook leans on — `resolveKvDatabaseIndex`
//! and the slot capability reader — plus the 屏 A → 屏 B overview jump mapping
//! and the slot-state relay's no-op / pruning rules.
//!
//! ## Consumer / removal stage
//!
//! No production KV host mounts these decisions yet. Every reusable item
//! carries a per-item `#[allow(dead_code)]` with this stage as its removal note.
//! No locale key, colour or icon is introduced: the React hook has no user-
//! facing copy of its own, and the unhandled-jump warning is a host diagnostic.

use std::collections::{BTreeMap, BTreeSet};

/// React `KvSlotName`, in render order (`KV_SLOT_NAMES`).
#[allow(dead_code)] // contract mirror; supplied by the driver registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum KvSlotName {
    ContextBar,
    StatusBar,
    KeyPropsSidebar,
    ConnectionHome,
}

impl KvSlotName {
    #[allow(dead_code)] // contract mirror; consumed by the resolver and tests.
    pub const ALL: [Self; 4] = [
        Self::ContextBar,
        Self::StatusBar,
        Self::KeyPropsSidebar,
        Self::ConnectionHome,
    ];

    #[allow(dead_code)] // contract mirror; consumed by the registry lookup.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ContextBar => "contextBar",
            Self::StatusBar => "statusBar",
            Self::KeyPropsSidebar => "keyPropsSidebar",
            Self::ConnectionHome => "connectionHome",
        }
    }

    /// React `capabilityKeyForSlot`: `connectionHome` reads the `home` flag;
    /// the other three keep their own names.
    #[allow(dead_code)] // pure helper; consumed by the capability reader.
    pub fn capability_key(self) -> &'static str {
        match self {
            Self::ConnectionHome => "home",
            _ => self.as_str(),
        }
    }
}

/// `DatabaseTypeMeta.kvWorkspace` plus the field-type half `resolveKvDatabaseIndex`
/// reads. Missing meta means "not a KV driver" and keeps the status quo.
#[allow(dead_code)] // VM mirror; supplied by the host DB registry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KvDriverMeta {
    pub database_field_type: String,
    pub context_bar: bool,
    pub status_bar: bool,
    pub key_props_sidebar: bool,
    pub home: bool,
}

/// This build's codegen-contributed components, keyed by (driver type, slot).
#[allow(dead_code)] // VM mirror; supplied by the generated driver registry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KvSlotRegistry {
    pub contributed: Vec<(String, KvSlotName)>,
}

impl KvSlotRegistry {
    #[allow(dead_code)] // pure lookup; consumed by the slot resolver and tests.
    pub fn has(&self, database_type: &str, slot: KvSlotName) -> bool {
        self.contributed.iter().any(|(kind, name)| kind == database_type && *name == slot)
    }
}

/// The active panel's identity half (React `Panel` fields the hook reads).
#[allow(dead_code)] // VM mirror; supplied by the panel store.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KvPanelRef {
    pub id: String,
    pub database_type: Option<String>,
    pub database: Option<String>,
}

/// React `ConnectionContext` — the landing screen has no panel, hence no atom.
#[allow(dead_code)] // VM mirror; supplied by the connection page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KvConnectionContext {
    pub connection_id: String,
    pub db_session_id: String,
    pub connection_name: String,
    pub database_type: String,
}

/// The shared `KvPanelSlotProps` bundle. `request` is deliberately NOT here:
/// only the context bar may ask the host for something.
#[allow(dead_code)] // props mirror; frozen once per panel identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KvPanelSlotProps {
    pub connection_id: String,
    pub db_session_id: String,
    pub connection_name: String,
    pub database_type: String,
    pub database: Option<String>,
    pub db_index: Option<u64>,
    /// The per-panel state atom's identity (React's `state` object reference).
    pub state_key: String,
}

/// One in-panel binding. The context bar alone carries the action channel.
#[allow(dead_code)] // binding mirror; consumed by the host surfaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KvSlotBinding {
    pub slot: KvSlotName,
    pub props: KvPanelSlotProps,
    pub has_request: bool,
}

/// React `KvConnectionHomeBinding` props plus the presence of the jump bridge.
#[allow(dead_code)] // binding mirror; consumed by the landing host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KvHomeBinding {
    pub connection_id: String,
    pub db_session_id: String,
    pub connection_name: String,
    pub database_type: String,
    pub initial_database: Option<String>,
    pub has_open_target: bool,
}

/// The hook's full result: four optional bindings and the panel-state key.
#[allow(dead_code)] // VM mirror; owned by the connection-workspace host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KvWorkspaceSlots {
    pub context_bar: Option<KvSlotBinding>,
    pub status_bar: Option<KvSlotBinding>,
    pub key_props_sidebar: Option<KvSlotBinding>,
    pub connection_home: Option<KvHomeBinding>,
    pub panel_state_key: Option<String>,
}

#[allow(dead_code)] // VM mirror; supplied by the connection-workspace host.
#[derive(Clone, Debug, Default)]
pub struct KvSlotsInput {
    pub active_panel: Option<KvPanelRef>,
    pub is_kv_panel: bool,
    pub database_type: Option<String>,
    pub database: Option<String>,
    pub db_session_id: String,
    pub connection_id: String,
    pub connection_name: String,
    pub connection_context: Option<KvConnectionContext>,
    pub initial_database: Option<String>,
    pub has_slot_action: bool,
    pub has_select_kv_db: bool,
    pub meta: Option<KvDriverMeta>,
    pub registry: KvSlotRegistry,
}

// ── Pure helpers ──────────────────────────────────────────────────────────

/// React `resolveKvDatabaseIndex`: only `databaseFieldType: 'index'` drivers
/// address databases by number; the label's trailing digits are the index and
/// must be a JS-safe integer (no rounded float reaching the driver).
#[allow(dead_code)] // pure helper; consumed by the resolver and tests.
pub fn resolve_kv_database_index(
    meta: Option<&KvDriverMeta>,
    database: Option<&str>,
) -> Option<u64> {
    let meta = meta?;
    if meta.database_field_type != "index" {
        return None;
    }
    let database = database?;
    if database.is_empty() {
        return None;
    }
    let digits = database.chars().rev().take_while(|ch| ch.is_ascii_digit()).collect::<String>();
    if digits.is_empty() {
        return None;
    }
    let parsed = digits.chars().rev().collect::<String>().parse::<u64>().ok()?;
    if parsed <= 9_007_199_254_740_991 {
        Some(parsed)
    } else {
        None
    }
}

/// React `hasKvSlotCapability(meta, slot)` — missing meta or capability is a
/// quiet `false`, which leaves non-KV drivers on the old rendering path.
#[allow(dead_code)] // pure helper; consumed by the resolver and tests.
pub fn has_slot_capability(meta: Option<&KvDriverMeta>, slot: KvSlotName) -> bool {
    let Some(meta) = meta else { return false };
    match slot {
        KvSlotName::ContextBar => meta.context_bar,
        KvSlotName::StatusBar => meta.status_bar,
        KvSlotName::KeyPropsSidebar => meta.key_props_sidebar,
        KvSlotName::ConnectionHome => meta.home,
    }
}

/// React `getKvSlotComponent`: capability AND contribution must both hold.
#[allow(dead_code)] // pure helper; consumed by the resolver and tests.
pub fn has_slot_component(
    meta: Option<&KvDriverMeta>,
    registry: &KvSlotRegistry,
    database_type: Option<&str>,
    slot: KvSlotName,
) -> bool {
    let Some(database_type) = database_type else { return false };
    has_slot_capability(meta, slot) && registry.has(database_type, slot)
}

/// The panel-state atom key exists only for a real, KV, identified panel. A
/// panel without an id must never mint a shared `undefined` atom.
#[allow(dead_code)] // pure helper; consumed by the resolver and tests.
pub fn panel_state_key(input: &KvSlotsInput) -> Option<String> {
    let panel = input.active_panel.as_ref()?;
    if !input.is_kv_panel || panel.id.is_empty() {
        return None;
    }
    Some(panel.id.clone())
}

/// React `panelSlotProps`: a panel atom and a resolved driver type are both
/// required before a driver component may render against the panel.
#[allow(dead_code)] // pure helper; consumed by the resolver and tests.
pub fn shared_panel_props(input: &KvSlotsInput, state_key: Option<&str>) -> Option<KvPanelSlotProps> {
    let state_key = state_key?;
    let database_type = input.database_type.as_ref()?;
    Some(KvPanelSlotProps {
        connection_id: input.connection_id.clone(),
        db_session_id: input.db_session_id.clone(),
        connection_name: input.connection_name.clone(),
        database_type: database_type.clone(),
        database: input.database.clone(),
        db_index: resolve_kv_database_index(input.meta.as_ref(), input.database.as_deref()),
        state_key: state_key.to_owned(),
    })
}

/// `inPanelKvContext` — slots never mount for a non-KV or half-resolved panel.
#[allow(dead_code)] // pure gate; consumed by the resolver and tests.
pub fn in_panel_context(input: &KvSlotsInput, props: Option<&KvPanelSlotProps>) -> bool {
    input.active_panel.is_some() && input.is_kv_panel && props.is_some()
}

/// Resolve all four slots plus the panel-state key in one pure decision.
#[allow(dead_code)] // decision; remove when the KV host wires the slots.
pub fn resolve_slots(input: &KvSlotsInput) -> KvWorkspaceSlots {
    let key = panel_state_key(input);
    let props = shared_panel_props(input, key.as_deref());
    let mut slots = KvWorkspaceSlots { panel_state_key: key, ..KvWorkspaceSlots::default() };

    if let Some(props) = props.as_ref().filter(|_| in_panel_context(input, props.as_ref())) {
        let database_type = input.database_type.as_deref();
        for (slot, has_component) in [
            (
                KvSlotName::ContextBar,
                has_slot_component(input.meta.as_ref(), &input.registry, database_type, KvSlotName::ContextBar),
            ),
            (
                KvSlotName::StatusBar,
                has_slot_component(input.meta.as_ref(), &input.registry, database_type, KvSlotName::StatusBar),
            ),
            (
                KvSlotName::KeyPropsSidebar,
                has_slot_component(input.meta.as_ref(), &input.registry, database_type, KvSlotName::KeyPropsSidebar),
            ),
        ] {
            if !has_component {
                continue;
            }
            let binding = KvSlotBinding {
                slot,
                props: props.clone(),
                has_request: slot == KvSlotName::ContextBar && input.has_slot_action,
            };
            match slot {
                KvSlotName::ContextBar => slots.context_bar = Some(binding),
                KvSlotName::StatusBar => slots.status_bar = Some(binding),
                KvSlotName::KeyPropsSidebar => slots.key_props_sidebar = Some(binding),
                KvSlotName::ConnectionHome => {}
            }
        }
    }

    // The landing screen has no panel, hence no selection/dirty atom and no
    // in-panel clusters. Its component is resolved against the context's type.
    let home_context = if input.active_panel.is_some() { None } else { input.connection_context.as_ref() };
    if let Some(context) = home_context {
        let home_type = context.database_type.as_str();
        if has_slot_component(input.meta.as_ref(), &input.registry, Some(home_type), KvSlotName::ConnectionHome) {
            slots.connection_home = Some(KvHomeBinding {
                connection_id: context.connection_id.clone(),
                db_session_id: context.db_session_id.clone(),
                connection_name: context.connection_name.clone(),
                database_type: context.database_type.clone(),
                initial_database: input.initial_database.clone(),
                has_open_target: input.has_select_kv_db,
            });
        }
    }
    slots
}

// ── 屏 A → 屏 B overview jump ─────────────────────────────────────────────

/// React `OverviewJumpTarget.kind` plus the payload fields the bridge reads.
#[allow(dead_code)] // VM mirror; supplied by the driver overview.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KvJumpKind {
    Database,
    NewKey,
    Key,
    Console,
    Pubsub,
    Monitor,
    ImportExport,
    Unknown(String),
}

#[allow(dead_code)] // VM mirror; supplied by the driver overview.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KvJumpTarget {
    pub kind: KvJumpKind,
    pub db_index: Option<u64>,
    pub key: Option<String>,
    pub section: Option<String>,
}

impl Default for KvJumpKind {
    fn default() -> Self {
        Self::Unknown(String::new())
    }
}

/// React `RedisPendingAction` — the one-shot payload handed to the host's
/// `onSelectKvDb` alongside the target database name.
#[allow(dead_code)] // VM mirror; consumed by the jump decision.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KvPendingAction {
    pub open_new_key: bool,
    pub select_key: Option<String>,
    pub key_db_index: Option<u64>,
    pub tab: Option<String>,
    pub monitor_sub_page: String,
    pub open_import_export: bool,
}

#[allow(dead_code)] // decision; remove when the KV home host wires the bridge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KvJumpAction {
    Open { database: String, pending: Option<KvPendingAction> },
    /// Unhandled kinds are a host diagnostic; no panel is opened.
    Unhandled,
}

/// React's `onOpenTarget` switch. An absent numeric index reads as `0`; tab
/// jumps use `initialDatabase ?? 'db0'` (nullish — an empty string is kept);
/// a monitor section is the cast string forwarded verbatim, defaulting to
/// `info` only when absent.
#[allow(dead_code)] // decision; remove when the KV home host wires the bridge.
pub fn plan_overview_jump(
    target: &KvJumpTarget,
    initial_database: Option<&str>,
) -> KvJumpAction {
    let db_num = target.db_index.unwrap_or(0);
    let database = format!("db{db_num}");
    let fallback = initial_database.unwrap_or("db0");
    let open = |database: String, pending: KvPendingAction| KvJumpAction::Open {
        database,
        pending: Some(pending),
    };
    match &target.kind {
        KvJumpKind::Database => KvJumpAction::Open { database, pending: None },
        KvJumpKind::NewKey => {
            open(database, KvPendingAction { open_new_key: true, ..KvPendingAction::default() })
        }
        KvJumpKind::Key => open(
            database,
            KvPendingAction {
                select_key: target.key.clone(),
                key_db_index: Some(db_num),
                ..KvPendingAction::default()
            },
        ),
        KvJumpKind::Console => {
            open(fallback.to_owned(), KvPendingAction { tab: Some("console".to_owned()), ..KvPendingAction::default() })
        }
        KvJumpKind::Pubsub => {
            open(fallback.to_owned(), KvPendingAction { tab: Some("pubsub".to_owned()), ..KvPendingAction::default() })
        }
        KvJumpKind::Monitor => {
            let sub = target.section.as_deref().unwrap_or("info");
            open(
                fallback.to_owned(),
                KvPendingAction {
                    tab: Some("monitor".to_owned()),
                    monitor_sub_page: sub.to_owned(),
                    ..KvPendingAction::default()
                },
            )
        }
        KvJumpKind::ImportExport => {
            open(fallback.to_owned(), KvPendingAction { open_import_export: true, ..KvPendingAction::default() })
        }
        KvJumpKind::Unknown(_) => KvJumpAction::Unhandled,
    }
}

// ── Slot-state relay lifecycle ────────────────────────────────────────────

/// The immutable snapshot every relay getter reads a scalar out of (React
/// `KvSlotSnapshot`). Kept as one record so a patch can compare fields.
#[allow(dead_code)] // VM mirror; published by the KV driver.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KvSlotSnapshot {
    pub selected_key: Option<String>,
    pub dirty: bool,
    pub loaded_count: u64,
    pub scan_cursor: String,
    pub scanning: bool,
    pub scan_budget_used: u64,
    pub scan_budget_total: u64,
    pub selection_count: u64,
    pub last_write_command: Option<String>,
    pub last_write_duration_ms: Option<f64>,
}

impl KvSlotSnapshot {
    #[allow(dead_code)] // VM mirror; the fresh-panel snapshot.
    pub fn empty() -> Self {
        Self { scan_cursor: "0".to_owned(), ..Self::default() }
    }
}

/// A partial relay update. `None` means "this field was not part of the patch".
#[allow(dead_code)] // VM mirror; published by the KV driver.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KvSlotPatch {
    pub selected_key: Option<Option<String>>,
    pub dirty: Option<bool>,
    pub loaded_count: Option<u64>,
    pub scan_cursor: Option<String>,
    pub scanning: Option<bool>,
    pub scan_budget: Option<(u64, u64)>,
    pub selection_count: Option<u64>,
    pub last_write: Option<(Option<String>, Option<f64>)>,
}

fn same_f64(left: Option<f64>, right: Option<f64>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(a), Some(b)) => a.to_bits() == b.to_bits(),
        _ => false,
    }
}

/// React `commit`: writing the value already there notifies nobody. Returns
/// `None` for a silent no-op and the new snapshot otherwise (object identity
/// changes exactly once per real patch, so consumers never render-storm).
#[allow(dead_code)] // pure decision; remove when the KV host wires the relay.
pub fn commit_slot_patch(
    current: &KvSlotSnapshot,
    patch: &KvSlotPatch,
) -> Option<KvSlotSnapshot> {
    let changed = patch.selected_key.as_ref().is_some_and(|v| *v != current.selected_key)
        || patch.dirty.is_some_and(|v| v != current.dirty)
        || patch.loaded_count.is_some_and(|v| v != current.loaded_count)
        || patch.scan_cursor.as_ref().is_some_and(|v| *v != current.scan_cursor)
        || patch.scanning.is_some_and(|v| v != current.scanning)
        || patch.scan_budget.is_some_and(|(used, total)| {
            used != current.scan_budget_used || total != current.scan_budget_total
        })
        || patch.selection_count.is_some_and(|v| v != current.selection_count)
        || patch.last_write.as_ref().is_some_and(|(command, duration)| {
            *command != current.last_write_command
                || !same_f64(*duration, current.last_write_duration_ms)
        });
    if !changed {
        return None;
    }
    let mut next = current.clone();
    if let Some(value) = &patch.selected_key {
        next.selected_key = value.clone();
    }
    if let Some(value) = patch.dirty {
        next.dirty = value;
    }
    if let Some(value) = patch.loaded_count {
        next.loaded_count = value;
    }
    if let Some(value) = &patch.scan_cursor {
        next.scan_cursor = value.clone();
    }
    if let Some(value) = patch.scanning {
        next.scanning = value;
    }
    if let Some((used, total)) = patch.scan_budget {
        next.scan_budget_used = used;
        next.scan_budget_total = total;
    }
    if let Some(value) = patch.selection_count {
        next.selection_count = value;
    }
    if let Some((command, duration)) = &patch.last_write {
        next.last_write_command = command.clone();
        next.last_write_duration_ms = *duration;
    }
    Some(next)
}

/// React `pruneKvSlotStates`: drop every atom whose panel is gone. The host
/// never disposes one panel's atom directly, so this is the only recycling
/// path. Returns the keys it removed, in stable order.
#[allow(dead_code)] // pure decision; remove when the KV host wires the relay.
pub fn prune_slot_states<V>(
    atoms: &BTreeMap<String, V>,
    live_panel_ids: &BTreeSet<String>,
) -> Vec<String> {
    atoms.keys().filter(|id| !live_panel_ids.contains(*id)).cloned().collect()
}
