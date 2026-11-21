//! Port of `components/sql-editor/proCompartments.ts` (T335, split) —
//! the privileged-extension (EP) plumbing for the SQL editor: the
//! compartment registry, the atomic batch reconfiguration, the generic
//! settings bag (G3), and the three generic hooks
//! (`createExtraExtensions` / `createFoldExtensions` /
//! `createExtraKeymap`).
//!
//! The React file's whole point is two costs the old six-slot wiring
//! paid: **non-atomic** reconfiguration (six dispatches, observers could
//! see half-configured frames) and a **closed** slot set (a privileged
//! extension could not add a capability without a host release). The
//! port keeps both rulings: [`ProReconfigure`] is one transaction by
//! construction (a single effect vec the host applies whole or not at
//! all — [`ProReconfigure::dispatched`] is the early-return's flag),
//! and the registry grows at runtime
//! ([`ProCompartmentRegistry::ensure`]).
//!
//! ## What maps where
//!
//! | React | Rune |
//! | --- | --- |
//! | `compartmentRegistry` module singleton + `compartments` Record | [`ProCompartmentRegistry`] (an owned struct the host shares; `Default` pre-registers the base set, the module init `for (const id of BASE) ensureProCompartment(id)`) |
//! | `Compartment` (opaque CM6 value) | [`ProCompartment`] (id + registration sequence — the identity `ensure`'s idempotence returns) |
//! | `ensureProCompartment(id)` | [`ProCompartmentRegistry::ensure`] (idempotent, appends after the base set) |
//! | `getProCompartment(id)` | [`ProCompartmentRegistry::get`] |
//! | `registeredProCompartmentIds()` | [`ProCompartmentRegistry::ids`] (registration order, base first) |
//! | `ProCompartmentPayload` (`Record<string, Extension[]>`) | [`ProCompartmentPayload`] (insertion-ordered; `Extension` values are opaque to this module — [`ProExtension`] tokens the host resolves, identity and order is all the machinery reads) |
//! | `mountProCompartments(payload)` | [`mount_pro_compartments`] (registers every payload id FIRST, then the mount list in payload order) |
//! | CodeMirror's per-state `compartment → content` map | [`ProCompartmentState`] — one per editor instance; presence IS mountedness (`Compartment.get(state) !== undefined`) |
//! | `reconfigureProCompartments(view, payload)` | [`reconfigure_pro_compartments`] (the classification + single-transaction plan) + [`ProCompartmentState::apply`] (the dispatch) |
//! | `readProSettingsBag` / `proSettingFlag` / `PRO_SETTINGS_EXTENSION_IDS` / `EMPTY_PRO_SETTINGS` | [`read_pro_settings_bag`] / [`pro_setting_flag`] / [`PRO_SETTINGS_EXTENSION_IDS`] — the definitions move here from `fold` (T196 carried them); `fold` re-exports keep its pre-T335 path stable (the T155 `lib_helpers` precedent) |
//! | `SafeCompartmentWrapper` circuit breaker | [`safe_compartment_wrapper`] — the engine cannot catch a throw across a seam, so the host reports it ([`EpHookAnswer::Threw`]) and the wrapper's ruling degrades to the fallback AND flags the unregister |
//! | `createProExtraExtensions` / `createFoldExtensions` | [`create_pro_extra_extensions`] / [`create_fold_extensions`] (absent-means-empty, the contract 1.1.0 rule) |
//! | `createProKeymapExtension` (`Prec.highest(keymap.of(...))`) | [`create_pro_keymap_extension`] → [`ProKeymapExtension`] at [`ProKeymapPrecedence::Highest`] — the precedence is load-bearing, not decoration (see the React header: `defaultKeymap` registers several always-`true` chords first, so a Pro binding at default precedence is never consulted) |
//!
//! ## The overflow merge's non-guard (ep-hooks-settings-BUG-003)
//!
//! The React comment pins it: `overflow` can never contain `extra`
//! itself — reaching the fold branch means `extra` IS mounted, and a
//! mounted `extra` was filed under `directIds` by the same predicate.
//! So the merge needs no per-id guard. The consequence the port keeps
//! verbatim: when the payload names `extra` AND some late id folds,
//! `extra` gets TWO effects in one transaction — its own direct
//! reconfigure, then the merged one — and the merged (later) one wins.
//! [`ProCompartmentState::apply`] applies sequentially, last write
//! wins; the test pins the end state.
//!
//! ## Multi-instance isolation
//!
//! The registry is shared (the React module singleton) but the mounted
//! map is per-editor: a reconfigure applied to one
//! [`ProCompartmentState`] can never reach another — structurally, and
//! pinned by test (the React file has a dedicated test for the same
//! invariant).
//!
//! Known deltas: `EMPTY_PRO_SETTINGS`'s frozen-singleton identity is a
//! JS `useMemo` concern (a fresh `{}` per read would spin the
//! reconfigure effect forever); the Rust port returns a structurally
//! empty object and the host's resync discipline is idempotent by
//! construction. The skipped-overflow `console.warn` becomes
//! [`ProReconfigure::skipped_unmounted`] — the host formats the message
//! (register them with `ensureProCompartment()` before the editor
//! mounts). `compartments.statement`-style property access is the
//! registry lookup. `BaseProCompartmentId` is a TS type-level name;
//! the id constants below are its runtime surface.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Compartment ids
// ---------------------------------------------------------------------------

/// Slot hosting `createExtraKeymap` output (`KEYMAP_COMPARTMENT_ID`).
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) const KEYMAP_COMPARTMENT_ID: &str = "keymap";
/// Generic slot hosting `createExtraExtensions` output and any overflow
/// (`EXTRA_COMPARTMENT_ID`).
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) const EXTRA_COMPARTMENT_ID: &str = "extra";
/// Slot hosting `createFoldExtensions` output — code folding
/// (`FOLD_COMPARTMENT_ID`). A dedicated slot rather than a corner of
/// `extra` because folding carries its own keymap: it needs a known
/// position in the mount order and independent reconfigurability.
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) const FOLD_COMPARTMENT_ID: &str = "fold";

/// Priority-ordered ids every editor instance mounts out of the box
/// (`BASE_PRO_COMPARTMENT_IDS`). The order is the mount order, which is
/// the order extension precedence resolves in.
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) const BASE_PRO_COMPARTMENT_IDS: [&str; 9] = [
    "statement",
    "completion",
    "intention",
    "hover",
    "paste",
    "linter",
    FOLD_COMPARTMENT_ID,
    KEYMAP_COMPARTMENT_ID,
    EXTRA_COMPARTMENT_ID,
];

// ---------------------------------------------------------------------------
// The extensible registry
// ---------------------------------------------------------------------------

/// CodeMirror's `Compartment`: an opaque, named slot handle. Unique per
/// registration (the [`ProCompartment::seq`] is the identity
/// `ensureProCompartment` hands back on repeat calls).
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) struct ProCompartment {
    /// The compartment id — the registry key and the per-state mounted
    /// map's key.
    pub(crate) id: String,
    /// Registration sequence: the identity two `ensure` calls for one
    /// id share.
    pub(crate) seq: u64,
}

/// The compartment registry (React's module-level `compartmentRegistry`
/// singleton plus the `compartments` Record). Owned by the host and
/// shared across editor instances; the per-instance isolation lives in
/// [`ProCompartmentState`].
#[derive(Clone, Debug)]
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) struct ProCompartmentRegistry {
    /// Registered ids in registration order (base ids first —
    /// [`ProCompartmentRegistry::default`] seeds them).
    order: Vec<String>,
    map: HashMap<String, ProCompartment>,
    next_seq: u64,
}

impl ProCompartmentRegistry {
    /// A fresh registry with every base id pre-registered — the React
    /// module init `for (const id of BASE_PRO_COMPARTMENT_IDS)
    /// ensureProCompartment(id)`.
    #[allow(dead_code)] // reusable port; the workspace view track wires it
    pub(crate) fn new() -> Self {
        let mut registry = ProCompartmentRegistry::default();
        for id in BASE_PRO_COMPARTMENT_IDS {
            registry.ensure(id);
        }
        registry
    }

    /// `ensureProCompartment(id)`: register an id (idempotent) and
    /// return its compartment. Call before the editor mounts — a
    /// brand-new compartment cannot be retro-fitted into an existing
    /// state, so an id discovered for the first time after mount rides
    /// the [`EXTRA_COMPARTMENT_ID`] overflow instead (the
    /// classification in [`reconfigure_pro_compartments`], never a
    /// throw).
    pub(crate) fn ensure(&mut self, id: &str) -> ProCompartment {
        if let Some(existing) = self.map.get(id) {
            return existing.clone();
        }
        let created = ProCompartment { id: id.to_owned(), seq: self.next_seq };
        self.next_seq += 1;
        self.order.push(id.to_owned());
        self.map.insert(id.to_owned(), created.clone());
        created
    }

    /// `getProCompartment(id)`: the compartment registered for `id`, or
    /// `None` when it is unknown.
    pub(crate) fn get(&self, id: &str) -> Option<&ProCompartment> {
        self.map.get(id)
    }

    /// `registeredProCompartmentIds()`: registered ids in registration
    /// order (base ids first).
    #[allow(dead_code)] // reusable port; the workspace view track wires it
    pub(crate) fn ids(&self) -> Vec<String> {
        self.order.clone()
    }
}

impl Default for ProCompartmentRegistry {
    fn default() -> Self {
        ProCompartmentRegistry { order: Vec::new(), map: HashMap::new(), next_seq: 0 }
    }
}

// ---------------------------------------------------------------------------
// Payload
// ---------------------------------------------------------------------------

/// The React `Extension` values are opaque to this module (CodeMirror
/// opaque values); the engine seam carries them as tokens the host
/// resolves. Identity and order are all the machinery reads.
pub(crate) type ProExtension = String;

/// `ProCompartmentPayload = Record<string, Extension[]>`: per-slot
/// extension bags keyed by compartment id, open by design (an extension
/// that registered its own id gets a key here without the type growing
/// a member). Insertion order preserved — JS Record key order, which
/// the reconfigure classification and the overflow merge both walk.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) struct ProCompartmentPayload {
    entries: Vec<(String, Vec<ProExtension>)>,
}

impl ProCompartmentPayload {
    #[allow(dead_code)] // reusable port; the workspace view track wires it
    pub(crate) fn new() -> Self {
        ProCompartmentPayload::default()
    }

    /// Record assignment `payload[id] = extensions`: a fresh key
    /// appends, an existing key replaces in place (position kept).
    #[allow(dead_code)] // reusable port; the workspace view track wires it
    pub(crate) fn insert(&mut self, id: &str, extensions: Vec<ProExtension>) -> &mut Self {
        match self.entries.iter_mut().find(|(key, _)| key == id) {
            Some((_, slot)) => *slot = extensions,
            None => self.entries.push((id.to_owned(), extensions)),
        }
        self
    }

    /// `payload[id]` — `None` when the payload does not name the id.
    pub(crate) fn get(&self, id: &str) -> Option<&[ProExtension]> {
        self.entries.iter().find(|(key, _)| key == id).map(|(_, slot)| slot.as_slice())
    }

    /// `Object.keys(payload)` in insertion order.
    pub(crate) fn keys(&self) -> Vec<String> {
        self.entries.iter().map(|(id, _)| id.clone()).collect()
    }

    /// Entry iteration in insertion order (id, extensions).
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&str, &[ProExtension])> {
        self.entries.iter().map(|(id, slot)| (id.as_str(), slot.as_slice()))
    }
}

/// One `Compartment.of(payload[id])` mount entry: the slot and its
/// initial contents, in payload order.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) struct MountedProCompartment {
    pub(crate) id: String,
    pub(crate) extensions: Vec<ProExtension>,
}

/// `mountProCompartments(payload)`: the extension list installed into
/// `state.extensions` at editor mount time. Registers every payload id
/// FIRST — so the returned list covers both the base slots and any
/// extension-owned slot — then yields one mount entry per payload key,
/// in payload order.
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) fn mount_pro_compartments(
    registry: &mut ProCompartmentRegistry,
    payload: &ProCompartmentPayload,
) -> Vec<MountedProCompartment> {
    for id in payload.keys() {
        registry.ensure(&id);
    }
    payload
        .iter()
        .map(|(id, extensions)| MountedProCompartment {
            id: id.to_owned(),
            extensions: extensions.to_vec(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The per-editor mounted state
// ---------------------------------------------------------------------------

/// One editor instance's mounted compartments — CodeMirror's per-state
/// `compartment → content` map. Presence is mountedness
/// (`Compartment.get(state) !== undefined`); the contents are the slot's
/// current extension list. Isolation: reconfigure touches only the
/// instance it is applied to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) struct ProCompartmentState {
    mounted: Vec<(String, Vec<ProExtension>)>,
}

impl ProCompartmentState {
    /// Install the mount list [`mount_pro_compartments`] produced (the
    /// editor state's initial per-compartment contents).
    #[allow(dead_code)] // reusable port; the workspace view track wires it
    pub(crate) fn mount(&mut self, mounts: &[MountedProCompartment]) {
        self.mounted = mounts.iter().map(|m| (m.id.clone(), m.extensions.clone())).collect();
    }

    /// `Compartment.get(state) !== undefined` — the only reliable
    /// presence check CM6 offers, and the classification's predicate.
    pub(crate) fn is_mounted(&self, id: &str) -> bool {
        self.mounted.iter().any(|(key, _)| key == id)
    }

    /// The slot's current contents, or `None` when unmounted.
    #[allow(dead_code)] // reusable port; the workspace view track wires it
    pub(crate) fn contents(&self, id: &str) -> Option<&[ProExtension]> {
        self.mounted.iter().find(|(key, _)| key == id).map(|(_, slot)| slot.as_slice())
    }

    /// Apply a reconfigure's effects as the single transaction they were
    /// built as: every effect lands, sequentially, last write wins per
    /// compartment (the extra double-write the overflow merge can
    /// produce — see the module header). Atomic by construction: the
    /// host applies the whole vec or none of it.
    ///
    /// `Err` models CodeMirror's throw when reconfiguring a compartment
    /// absent from the state — unreachable by design, because the
    /// classification only ever emits effects for mounted slots (that
    /// is its whole job); the arm exists for host integration parity.
    #[allow(dead_code)] // reusable port; the workspace view track wires it
    pub(crate) fn apply(&mut self, reconfigure: &ProReconfigure) -> Result<(), String> {
        for effect in &reconfigure.effects {
            match self.mounted.iter_mut().find(|(id, _)| *id == effect.compartment_id) {
                Some((_, slot)) => *slot = effect.extensions.clone(),
                None => {
                    return Err(format!(
                        "reconfigure of unmounted compartment `{}`",
                        effect.compartment_id
                    ))
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Atomic batch reconfiguration
// ---------------------------------------------------------------------------

/// One `compartment.reconfigure(extensions)` effect.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) struct ProReconfigureEffect {
    pub(crate) compartment_id: String,
    pub(crate) extensions: Vec<ProExtension>,
}

/// What `reconfigureProCompartments` built: the single transaction it
/// dispatched (or the decision not to), plus the overflow bookkeeping
/// the React function only logged.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) struct ProReconfigure {
    /// The reconfigure effects in dispatch order: every direct id in
    /// payload order, then the merged `extra` overflow when it applies.
    /// One transaction — all or nothing ([`ProReconfigure::dispatched`]
    /// is the early return).
    pub(crate) effects: Vec<ProReconfigureEffect>,
    /// Late ids folded into [`EXTRA_COMPARTMENT_ID`] (announced after
    /// this view was built, but `extra` was mounted so they ride it).
    pub(crate) folded_into_extra: Vec<String>,
    /// Late ids skipped with the warning (no mounted slot at all — the
    /// host formats the message the React `console.warn` carried).
    pub(crate) skipped_unmounted: Vec<String>,
    /// `Transaction.addToHistory.of(false)`: the reconfigure is
    /// invisible to undo. Verbatim — always `false` when dispatched.
    pub(crate) add_to_history: bool,
}

impl ProReconfigure {
    /// Whether `view.dispatch` happens at all: the React early return
    /// when nothing applies (an all-overflow payload with `extra`
    /// unmounted dispatches nothing).
    #[allow(dead_code)] // reusable port; the workspace view track wires it
    pub(crate) fn dispatched(&self) -> bool {
        !self.effects.is_empty()
    }
}

/// `reconfigureProCompartments(view, payload)`: atomically reconfigure
/// every Pro-driven compartment. The classification, verbatim:
///
/// - A payload id whose compartment is registered AND mounted in this
///   view is reconfigured directly (payload order).
/// - Every other id is overflow: folded into [`EXTRA_COMPARTMENT_ID`]
///   when that slot is mounted (merged = `payload[extra] ?? []` plus
///   each overflow id's list in payload order), else skipped with the
///   warning — degrades, never throws, because a rejected reconfigure
///   would leave the editor half-applied.
///
/// Call [`ProCompartmentState::apply`] on the target view to land the
/// plan; other views are structurally untouched.
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) fn reconfigure_pro_compartments(
    registry: &ProCompartmentRegistry,
    state: &ProCompartmentState,
    payload: &ProCompartmentPayload,
) -> ProReconfigure {
    let mut direct: Vec<ProReconfigureEffect> = Vec::new();
    let mut direct_ids: Vec<String> = Vec::new();
    let mut overflow: Vec<String> = Vec::new();

    for id in payload.keys() {
        // `compartment && compartment.get(view.state) !== undefined` —
        // registered AND mounted in THIS view.
        let mounted = registry.get(&id).is_some() && state.is_mounted(&id);
        if mounted {
            direct_ids.push(id);
        } else {
            overflow.push(id);
        }
    }

    for id in &direct_ids {
        // The classification guarantees presence; the `??` never fires
        // for a direct id (it walked out of the payload's own keys).
        let extensions =
            payload.get(id).map(|slot| slot.to_vec()).unwrap_or_default();
        direct.push(ProReconfigureEffect { compartment_id: id.clone(), extensions });
    }

    let mut folded_into_extra = Vec::new();
    let mut skipped_unmounted = Vec::new();

    if !overflow.is_empty() {
        if state.is_mounted(EXTRA_COMPARTMENT_ID) {
            // `overflow` cannot contain `extra` itself: reaching this
            // branch means `extra` is mounted, and a mounted `extra` was
            // filed under `directIds` by the same predicate. No per-id
            // guard — one here would be dead code (ep-hooks-settings-
            // BUG-003). Consequence: a payload naming BOTH `extra` and
            // a late id produces two `extra` effects in one transaction
            // and the merged (later) one wins on apply.
            let mut merged: Vec<ProExtension> = payload
                .get(EXTRA_COMPARTMENT_ID)
                .map(|slot| slot.to_vec())
                .unwrap_or_default();
            for id in &overflow {
                merged.extend(payload.get(id).map(|slot| slot.to_vec()).unwrap_or_default());
            }
            direct.push(ProReconfigureEffect {
                compartment_id: EXTRA_COMPARTMENT_ID.to_owned(),
                extensions: merged,
            });
            folded_into_extra = overflow;
        } else {
            skipped_unmounted = overflow;
        }
    }

    ProReconfigure {
        effects: direct,
        folded_into_extra,
        skipped_unmounted,
        add_to_history: false,
    }
}

// ---------------------------------------------------------------------------
// Generic settings bag (G3) — absorbed from `fold` (T196 carried them)
// ---------------------------------------------------------------------------

/// Privileged extension settings, as persisted under
/// `driverSettings[extensionId]` (a JSON object; the flag reader
/// tolerates anything else exactly as JS's `bag?.[key]` yields
/// undefined).
pub(crate) type ProSettingsBag = serde_json::Value;

/// Settings namespaces owned by the SQL editor privileged EP
/// (`PRO_SETTINGS_EXTENSION_IDS`). Order matters: the first id whose
/// value is a real object wins.
pub(crate) const PRO_SETTINGS_EXTENSION_IDS: [&str; 2] =
    ["sql-editor-enhanced", "sql-editor-pro"];

/// `readProSettingsBag`: read the privileged settings bag for the
/// extension ids above — the first value that is a non-array object
/// (`bag && typeof bag === 'object' && !Array.isArray(bag)`). The bag
/// rides every compartment factory wholesale, so any key an extension
/// declares in its `settingsContributions` can influence the editor
/// (previously the editor read five hard-coded keys and ignored the
/// rest).
///
/// The React `EMPTY_PRO_SETTINGS` frozen singleton exists so a
/// `useMemo`/selector sees a stable identity; the Rust port returns a
/// structurally empty object (see the module header's known deltas).
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) fn read_pro_settings_bag(
    driver_settings: Option<&serde_json::Value>,
) -> ProSettingsBag {
    if let Some(serde_json::Value::Object(map)) = driver_settings {
        for id in PRO_SETTINGS_EXTENSION_IDS {
            if let Some(bag) = map.get(id) {
                if bag.is_object() {
                    return bag.clone();
                }
            }
        }
    }
    serde_json::Value::Object(serde_json::Map::new())
}

/// `proSettingFlag`: read a boolean setting, falling back when the key
/// is absent **or non-boolean** — only a real boolean switches, so
/// settings corruption degrades to the documented default instead of
/// silently disabling a feature.
#[allow(dead_code)] // consumed via fold's re-export; wired with the workspace view
pub(crate) fn pro_setting_flag(bag: Option<&ProSettingsBag>, key: &str, fallback: bool) -> bool {
    match bag.and_then(|bag| bag.get(key)) {
        Some(serde_json::Value::Bool(value)) => *value,
        _ => fallback,
    }
}

// ---------------------------------------------------------------------------
// Generic hooks (SafeCompartmentWrapper + the three factories)
// ---------------------------------------------------------------------------

/// The host's pre-resolved answer for one EP hook — the engine cannot
/// run (or catch) a JS factory, so the seam reports which of the
/// wrapper's paths applies.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) enum EpHookAnswer<T> {
    /// The EP is not registered, or does not implement the hook:
    /// `enhanced.createXxx?.(opts) ?? []` (the contract 1.1.0
    /// absent-means-empty rule — no `EXTENSION_POINTS_VERSION` bump).
    Absent,
    /// The hook ran (the host forwarded `opts` verbatim) and produced
    /// this value.
    Value(T),
    /// The factory threw: the circuit breaker trips.
    Threw,
}

/// `SafeCompartmentWrapper`'s ruling: the value the editor mounts (the
/// EP's answer or the fallback) and whether the EP must be
/// unregistered (a throwing factory loses its registration, so the
/// NEXT call is the unregistered path, not another throw).
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) struct SafeCompartment<T> {
    pub(crate) value: T,
    /// Whether the host must unregister the EP from the extension
    /// registry.
    pub(crate) unregister: bool,
}

/// `SafeCompartmentWrapper({ point, featureName }, thunk, fallback)`:
/// circuit-broken like every other privileged factory — a throwing
/// extension degrades to the fallback and unregisters the EP instead of
/// taking the editor down. `feature_name` rides for the host's
/// diagnostics (the wrapper logs it before unregistering).
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) fn safe_compartment_wrapper<T>(
    _feature_name: &str,
    answer: EpHookAnswer<T>,
    fallback: T,
) -> SafeCompartment<T> {
    match answer {
        EpHookAnswer::Value(value) => SafeCompartment { value, unregister: false },
        EpHookAnswer::Absent | EpHookAnswer::Threw => SafeCompartment {
            value: fallback,
            unregister: matches!(answer, EpHookAnswer::Threw),
        },
    }
}

/// `createProExtraExtensions` → the generic `extra` compartment's EP
/// half. Circuit-broken like every other privileged factory.
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) fn create_pro_extra_extensions(
    answer: EpHookAnswer<Vec<ProExtension>>,
) -> SafeCompartment<Vec<ProExtension>> {
    safe_compartment_wrapper("createExtraExtensions", answer, Vec::new())
}

/// `createFoldExtensions` → the `fold` compartment's EP half.
/// Optional hook, absent-means-empty like every other contract 1.1.0
/// hook: an extension that predates code folding simply does not
/// implement it and the slot reconfigures to `[]`.
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) fn create_fold_extensions(
    answer: EpHookAnswer<Vec<ProExtension>>,
) -> SafeCompartment<Vec<ProExtension>> {
    safe_compartment_wrapper("createFoldExtensions", answer, Vec::new())
}

/// One EP-provided key binding: the chord spellings plus the opaque
/// handler (the React `KeyBinding`'s `run` never crosses into this
/// module — only the wrapping precedence does).
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) struct ProKeyBinding {
    /// The `key:` spelling (`Mod-` is Cmd on macOS, Ctrl elsewhere).
    pub(crate) key: String,
    /// The `mac:` variant, when the chord differs.
    pub(crate) mac: Option<String>,
}

/// The precedence the Pro keymap mounts at. `Prec.highest` is
/// load-bearing, not decoration: `createBaseEditorExtensions`
/// registers `keymap.of([...defaultKeymap, ...])` BEFORE any Pro
/// compartment, and several `defaultKeymap` commands
/// (`copyLineUp`/`copyLineDown`, `moveLineUp`/`moveLineDown`,
/// `addCursorAbove`/`addCursorBelow`) return `true` whenever they do
/// anything — CodeMirror stops at the first `true`, so an extension
/// binding one of those chords at default precedence is never even
/// consulted. The corollary carries too: a hook handler returning
/// `false` still hands the chord on — `Prec.highest` widens reach, it
/// does not make the hook greedy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) enum ProKeymapPrecedence {
    /// `Prec.highest(keymap.of(bindings))`.
    Highest,
}

/// `createProKeymapExtension`'s output: the EP's bindings wrapped at
/// [`ProKeymapPrecedence::Highest`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) struct ProKeymapExtension {
    pub(crate) bindings: Vec<ProKeyBinding>,
    pub(crate) precedence: ProKeymapPrecedence,
}

/// `createProKeymapExtension(opts)`: the EP's `createExtraKeymap`
/// bindings, circuit-broken, wrapped at highest precedence.
#[allow(dead_code)] // reusable port; the workspace view track wires it
pub(crate) fn create_pro_keymap_extension(
    answer: EpHookAnswer<Vec<ProKeyBinding>>,
) -> SafeCompartment<ProKeymapExtension> {
    let SafeCompartment { value: bindings, unregister } =
        safe_compartment_wrapper("createExtraKeymap", answer, Vec::new());
    SafeCompartment {
        value: ProKeymapExtension { bindings, precedence: ProKeymapPrecedence::Highest },
        unregister,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payload(entries: &[(&str, &[&str])]) -> ProCompartmentPayload {
        let mut payload = ProCompartmentPayload::new();
        for (id, extensions) in entries {
            payload.insert(
                id,
                extensions.iter().map(|token| (*token).to_owned()).collect(),
            );
        }
        payload
    }

    fn full_state(registry: &mut ProCompartmentRegistry) -> ProCompartmentState {
        // The realistic editor: every base id mounted with a starter list.
        let mounts = mount_pro_compartments(
            registry,
            &payload(&[
                ("statement", &["stmt"]),
                ("completion", &["cmpl"]),
                ("intention", &["intent"]),
                ("hover", &["hov"]),
                ("paste", &["paste"]),
                ("linter", &["lint"]),
                ("fold", &["fold"]),
                ("keymap", &["km"]),
                ("extra", &["ex"]),
            ]),
        );
        let mut state = ProCompartmentState::default();
        state.mount(&mounts);
        state
    }

    // ---- the registry ------------------------------------------------------

    #[test]
    fn the_base_ids_are_pre_registered_in_order() {
        let registry = ProCompartmentRegistry::new();
        let registry_ids = registry.ids();
        assert_eq!(registry_ids, BASE_PRO_COMPARTMENT_IDS.map(String::from).to_vec());
        // The named-slot consts are members of the base set.
        let ids: Vec<&str> = registry_ids.iter().map(String::as_str).collect();
        assert!(ids.contains(&KEYMAP_COMPARTMENT_ID));
        assert!(ids.contains(&EXTRA_COMPARTMENT_ID));
        assert!(ids.contains(&FOLD_COMPARTMENT_ID));
        // Base order: fold, keymap, extra are the last three (mount
        // order = precedence order).
        assert_eq!(&ids[6..], &[FOLD_COMPARTMENT_ID, KEYMAP_COMPARTMENT_ID, EXTRA_COMPARTMENT_ID]);
    }

    #[test]
    fn ensure_is_idempotent_and_appends_after_the_base_set() {
        let mut registry = ProCompartmentRegistry::new();
        let first = registry.ensure("diagnostics");
        let again = registry.ensure("diagnostics");
        // Same compartment back — the identity the React singleton hands
        // out for repeat registrations.
        assert_eq!(first, again);
        assert_eq!(registry.ids().len(), BASE_PRO_COMPARTMENT_IDS.len() + 1);
        assert_eq!(registry.ids().last().map(String::as_str), Some("diagnostics"));
        // getProCompartment: known vs unknown.
        assert_eq!(registry.get("diagnostics"), Some(&first));
        assert_eq!(registry.get("never-registered"), None);
    }

    // ---- mount ---------------------------------------------------------------

    #[test]
    fn mount_registers_every_payload_id_first_in_payload_order() {
        let mut registry = ProCompartmentRegistry::new();
        let mounts = mount_pro_compartments(
            &mut registry,
            &payload(&[("statement", &["s1"]), ("ext-owned", &["x1"]), ("extra", &[])]),
        );
        // The extension-owned id was registered BEFORE the mount list
        // was built, so both base and owned slots are covered.
        assert!(registry.get("ext-owned").is_some());
        let ids: Vec<&str> = mounts.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["statement", "ext-owned", "extra"]);
        assert_eq!(mounts[1].extensions, ["x1"]);

        let mut state = ProCompartmentState::default();
        state.mount(&mounts);
        // Mountedness is per-slot presence: the base ids the payload
        // did not name are NOT mounted in this view.
        assert!(state.is_mounted("statement"));
        assert!(state.is_mounted("ext-owned"));
        assert!(!state.is_mounted("completion"));
        assert_eq!(state.contents("statement"), Some(&["s1".to_owned()][..]));
        assert_eq!(state.contents("completion"), None);
    }

    // ---- reconfigure: direct ---------------------------------------------------

    #[test]
    fn direct_reconfigures_land_in_one_transaction_in_payload_order() {
        let mut registry = ProCompartmentRegistry::new();
        let mut state = full_state(&mut registry);
        let plan = reconfigure_pro_compartments(
            &registry,
            &state,
            &payload(&[("linter", &["lint2"]), ("statement", &["stmt2"])]),
        );
        // Payload order, not base order; a single dispatched transaction.
        assert!(plan.dispatched());
        let ids: Vec<&str> = plan.effects.iter().map(|e| e.compartment_id.as_str()).collect();
        assert_eq!(ids, ["linter", "statement"]);
        assert_eq!(plan.effects[1].extensions, ["stmt2"]);
        // addToHistory.of(false) rides the same dispatch.
        assert!(!plan.add_to_history);
        assert!(plan.folded_into_extra.is_empty());
        assert!(plan.skipped_unmounted.is_empty());

        state.apply(&plan).unwrap();
        assert_eq!(state.contents("statement"), Some(&["stmt2".to_owned()][..]));
        assert_eq!(state.contents("linter"), Some(&["lint2".to_owned()][..]));
        // Untouched slots keep their contents.
        assert_eq!(state.contents("completion"), Some(&["cmpl".to_owned()][..]));
    }

    // ---- reconfigure: the overflow merge ------------------------------------------

    #[test]
    fn late_ids_fold_into_extra_when_it_is_mounted() {
        let mut registry = ProCompartmentRegistry::new();
        let mut state = full_state(&mut registry);
        // A slot announced AFTER this view was built: registered now,
        // but this view never mounted it.
        registry.ensure("late-slot");

        let plan = reconfigure_pro_compartments(
            &registry,
            &state,
            &payload(&[("statement", &["stmt2"]), ("late-slot", &["late1"])]),
        );
        assert!(plan.dispatched());
        assert_eq!(plan.folded_into_extra, ["late-slot"]);
        assert!(plan.skipped_unmounted.is_empty());
        // Direct first, then the merged extra effect.
        let ids: Vec<&str> = plan.effects.iter().map(|e| e.compartment_id.as_str()).collect();
        assert_eq!(ids, ["statement", "extra"]);
        // merged = payload[extra] ?? [] + the overflow list — the
        // payload named no `extra`, so the merge starts empty.
        assert_eq!(plan.effects[1].extensions, ["late1"]);

        state.apply(&plan).unwrap();
        assert_eq!(state.contents("extra"), Some(&["late1".to_owned()][..]));
        // The late slot itself is still not mounted in this view.
        assert!(!state.is_mounted("late-slot"));
    }

    #[test]
    fn a_payload_naming_extra_and_a_late_id_writes_extra_twice_merged_wins() {
        // The non-guard's consequence (ep-hooks-settings-BUG-003): when
        // the payload names `extra` AND a late id folds, `extra` gets
        // two effects in one transaction — its direct reconfigure, then
        // the merged one — and the merged (later) one wins on apply.
        let mut registry = ProCompartmentRegistry::new();
        let mut state = full_state(&mut registry);
        registry.ensure("late-slot");

        let plan = reconfigure_pro_compartments(
            &registry,
            &state,
            &payload(&[("extra", &["ex2"]), ("late-slot", &["late1"])]),
        );
        let ids: Vec<&str> = plan.effects.iter().map(|e| e.compartment_id.as_str()).collect();
        assert_eq!(ids, ["extra", "extra"]);
        // merged = payload[extra] ++ payload[late-slot].
        assert_eq!(plan.effects[0].extensions, ["ex2"]);
        assert_eq!(plan.effects[1].extensions, ["ex2", "late1"]);

        state.apply(&plan).unwrap();
        assert_eq!(state.contents("extra"), Some(&["ex2".to_owned(), "late1".to_owned()][..]));
    }

    #[test]
    fn multiple_late_ids_merge_in_payload_order() {
        let mut registry = ProCompartmentRegistry::new();
        let state = full_state(&mut registry);
        registry.ensure("late-a");
        registry.ensure("late-b");

        let plan = reconfigure_pro_compartments(
            &registry,
            &state,
            &payload(&[("extra", &["ex0"]), ("late-b", &["b1"]), ("late-a", &["a1"])]),
        );
        // merged starts from payload[extra], then each overflow id's
        // list in PAYLOAD order (late-b before late-a, as written).
        assert_eq!(plan.effects.last().unwrap().extensions, ["ex0", "b1", "a1"]);
    }

    // ---- reconfigure: the skip path and the no-op -----------------------------------

    #[test]
    fn late_ids_are_skipped_with_a_warning_when_extra_is_not_mounted() {
        let mut registry = ProCompartmentRegistry::new();
        // A view that mounted WITHOUT the extra slot (a payload subset).
        let mounts = mount_pro_compartments(
            &mut registry,
            &payload(&[("statement", &["stmt"]), ("extra", &["ex"])]),
        );
        let mut partial = ProCompartmentState::default();
        let mounts: Vec<MountedProCompartment> =
            mounts.into_iter().filter(|m| m.id != EXTRA_COMPARTMENT_ID).collect();
        partial.mount(&mounts);
        assert!(!partial.is_mounted(EXTRA_COMPARTMENT_ID));

        registry.ensure("late-slot");
        let plan = reconfigure_pro_compartments(
            &registry,
            &partial,
            &payload(&[("statement", &["stmt2"]), ("late-slot", &["late1"])]),
        );
        // The mounted direct id still lands; the late id degrades to
        // the warning instead of throwing.
        assert!(plan.dispatched());
        assert!(plan.folded_into_extra.is_empty());
        assert_eq!(plan.skipped_unmounted, ["late-slot"]);
        let ids: Vec<&str> = plan.effects.iter().map(|e| e.compartment_id.as_str()).collect();
        assert_eq!(ids, ["statement"]);

        partial.apply(&plan).unwrap();
        assert_eq!(partial.contents("statement"), Some(&["stmt2".to_owned()][..]));
    }

    #[test]
    fn a_view_with_no_applicable_slots_dispatches_nothing() {
        let mut registry = ProCompartmentRegistry::new();
        let state = full_state(&mut registry);
        registry.ensure("late-slot");

        // Only a late id, and this view mounted no extra slot to fold
        // into... it DID (full_state mounts extra) — so use a payload
        // with nothing applicable at all: an unknown id with extra
        // unmounted needs the partial view instead.
        let plan = reconfigure_pro_compartments(
            &registry,
            &state,
            &payload(&[("late-slot", &["late1"])]),
        );
        // Full state mounts extra: the late id folds (nothing to skip).
        assert!(plan.dispatched());
        assert_eq!(plan.folded_into_extra, ["late-slot"]);

        // The real early return: extra NOT mounted, only overflow —
        // direct is empty, so NO view.dispatch happens at all.
        let mounts = mount_pro_compartments(&mut registry, &payload(&[("statement", &["stmt"])]));
        let mut minimal = ProCompartmentState::default();
        minimal.mount(&mounts);
        let plan = reconfigure_pro_compartments(
            &registry,
            &minimal,
            &payload(&[("late-slot", &["late1"])]),
        );
        assert!(!plan.dispatched(), "direct is empty — the React early return");
        assert!(plan.effects.is_empty());
        assert_eq!(plan.skipped_unmounted, ["late-slot"]);
        minimal.apply(&plan).unwrap();
        assert_eq!(minimal.contents("statement"), Some(&["stmt".to_owned()][..]));
    }

    #[test]
    fn an_empty_payload_is_a_no_op() {
        let mut registry = ProCompartmentRegistry::new();
        let state = full_state(&mut registry);
        let plan = reconfigure_pro_compartments(&registry, &state, &ProCompartmentPayload::new());
        assert!(!plan.dispatched());
        assert!(plan.folded_into_extra.is_empty());
        assert!(plan.skipped_unmounted.is_empty());
    }

    // ---- the multi-instance invariant ----------------------------------------------

    #[test]
    fn a_reconfigure_never_reaches_another_view() {
        // The React header's dedicated invariant: the registry (and its
        // compartments) are shared, but the per-state content map is
        // not — a reconfigure dispatched to one view can never reach
        // another.
        let mut registry = ProCompartmentRegistry::new();
        let mut view_a = full_state(&mut registry);
        let view_b = full_state(&mut registry);

        let plan = reconfigure_pro_compartments(
            &registry,
            &view_a,
            &payload(&[("statement", &["only-a"])]),
        );
        view_a.apply(&plan).unwrap();
        assert_eq!(view_a.contents("statement"), Some(&["only-a".to_owned()][..]));
        assert_eq!(view_b.contents("statement"), Some(&["stmt".to_owned()][..]));
    }

    #[test]
    fn apply_reports_the_modeled_throw_for_unmounted_slots() {
        let mut registry = ProCompartmentRegistry::new();
        let mut state = full_state(&mut registry);
        // A hand-built plan targeting an unmounted slot — the
        // classification never produces this; CodeMirror would throw.
        let plan = ProReconfigure {
            effects: vec![ProReconfigureEffect {
                compartment_id: "ghost".to_owned(),
                extensions: vec!["g1".to_owned()],
            }],
            folded_into_extra: Vec::new(),
            skipped_unmounted: Vec::new(),
            add_to_history: false,
        };
        assert_eq!(state.apply(&plan), Err("reconfigure of unmounted compartment `ghost`".to_owned()));
    }

    // ---- the settings bag (G3) --------------------------------------------------------

    #[test]
    fn the_bag_takes_the_first_object_under_either_extension_id() {
        // No driver settings at all: the stable empty bag.
        assert_eq!(read_pro_settings_bag(None), json!({}));
        assert_eq!(read_pro_settings_bag(Some(&json!(null))), json!({}));
        // The first extension id wins when both hold objects.
        let settings = json!({
            "sql-editor-enhanced": { "codeFolding": true },
            "sql-editor-pro": { "codeFolding": false },
        });
        assert_eq!(read_pro_settings_bag(Some(&settings)), json!({ "codeFolding": true }));
        // A non-object first id falls through to the second.
        let settings = json!({
            "sql-editor-enhanced": ["not", "a", "bag"],
            "sql-editor-pro": { "proKey": 7 },
        });
        assert_eq!(read_pro_settings_bag(Some(&settings)), json!({ "proKey": 7 }));
        // Strings, numbers, booleans and null are not bags either.
        for bad in [json!("x"), json!(3), json!(false)] {
            let settings = json!({ "sql-editor-enhanced": bad, "sql-editor-pro": bad });
            assert_eq!(read_pro_settings_bag(Some(&settings)), json!({}));
        }
        // Neither id present: empty, whatever else the settings hold.
        assert_eq!(read_pro_settings_bag(Some(&json!({ "theme": "dark" }))), json!({}));
    }

    #[test]
    fn the_flag_only_switches_on_a_real_boolean() {
        let bag = json!({ "on": true, "off": false });
        assert!(pro_setting_flag(Some(&bag), "on", false));
        assert!(!pro_setting_flag(Some(&bag), "off", true));
        // Missing key: the fallback.
        assert!(pro_setting_flag(Some(&bag), "missing", true));
        assert!(!pro_setting_flag(Some(&bag), "missing", false));
        // No bag at all: the fallback.
        assert!(pro_setting_flag(None, "on", true));
        assert!(!pro_setting_flag(None, "on", false));
        // Corruption (non-boolean values) degrades to the fallback.
        let corrupt = json!({ "a": "yes", "b": 1, "c": null, "d": [] });
        assert!(pro_setting_flag(Some(&corrupt), "a", true));
        assert!(pro_setting_flag(Some(&corrupt), "b", true));
        assert!(pro_setting_flag(Some(&corrupt), "c", true));
        assert!(pro_setting_flag(Some(&corrupt), "d", true));
        assert!(!pro_setting_flag(Some(&corrupt), "a", false));
    }

    // ---- the generic hooks ------------------------------------------------------------

    #[test]
    fn the_extra_and_fold_hooks_degrade_to_empty_and_never_throw() {
        // Absent EP / unimplemented hook: absent-means-empty, no unregister.
        assert_eq!(create_pro_extra_extensions(EpHookAnswer::Absent),
            SafeCompartment { value: Vec::new(), unregister: false });
        assert_eq!(create_fold_extensions(EpHookAnswer::Absent),
            SafeCompartment { value: Vec::new(), unregister: false });
        // A live EP's contribution rides verbatim.
        let live = vec!["ep-ext".to_owned()];
        assert_eq!(create_pro_extra_extensions(EpHookAnswer::Value(live.clone())),
            SafeCompartment { value: live, unregister: false });
        let fold_live = vec!["fold-ext".to_owned()];
        assert_eq!(create_fold_extensions(EpHookAnswer::Value(fold_live.clone())),
            SafeCompartment { value: fold_live, unregister: false });
        // A throwing factory: degrade to [] AND unregister the EP.
        assert_eq!(create_pro_extra_extensions(EpHookAnswer::Threw),
            SafeCompartment { value: Vec::new(), unregister: true });
        assert_eq!(create_fold_extensions(EpHookAnswer::Threw),
            SafeCompartment { value: Vec::new(), unregister: true });
    }

    #[test]
    fn the_pro_keymap_mounts_at_highest_precedence() {
        // Prec.highest is the load-bearing wrap (defaultKeymap's
        // always-true chords own everything at default precedence).
        let bindings = vec![
            ProKeyBinding { key: "Mod-Enter".to_owned(), mac: Some("Cmd-Enter".to_owned()) },
            ProKeyBinding { key: "Shift-Mod-f".to_owned(), mac: None },
        ];
        let wrapped = create_pro_keymap_extension(EpHookAnswer::Value(bindings.clone()));
        assert_eq!(
            wrapped,
            SafeCompartment {
                value: ProKeymapExtension {
                    bindings,
                    precedence: ProKeymapPrecedence::Highest,
                },
                unregister: false,
            }
        );
        // Absent and tripped both degrade to an empty keymap — still at
        // highest precedence (the wrap is what the factory returns).
        let absent = create_pro_keymap_extension(EpHookAnswer::Absent);
        assert_eq!(absent.value.precedence, ProKeymapPrecedence::Highest);
        assert!(absent.value.bindings.is_empty());
        assert!(!absent.unregister);
        let tripped = create_pro_keymap_extension(EpHookAnswer::Threw);
        assert_eq!(tripped.value.precedence, ProKeymapPrecedence::Highest);
        assert!(tripped.value.bindings.is_empty());
        assert!(tripped.unregister);
    }

    #[test]
    fn the_wrapper_carries_the_fallback_for_every_answer_kind() {
        // The generic ruling, independent of the three factories above.
        assert_eq!(
            safe_compartment_wrapper("anyFeature", EpHookAnswer::Value(42), 0),
            SafeCompartment { value: 42, unregister: false }
        );
        assert_eq!(
            safe_compartment_wrapper("anyFeature", EpHookAnswer::Absent, 0),
            SafeCompartment { value: 0, unregister: false }
        );
        assert_eq!(
            safe_compartment_wrapper("anyFeature", EpHookAnswer::<i32>::Threw, 0),
            SafeCompartment { value: 0, unregister: true }
        );
    }
}
