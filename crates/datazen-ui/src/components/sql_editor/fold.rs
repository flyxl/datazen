//! Port of `components/sql-editor/fold/foldCompartment.ts` (T196, split)
//! — the host side of the `fold` Pro compartment.
//!
//! The split: this module owns the host's single decision (**whether**
//! to ask for folding at all); the compartment registry itself — the id
//! set, the mount/reconfigure machinery, `SafeCompartmentWrapper`, the
//! `extra` overflow — landed as [`super::pro_compartments`] (T335).
//! [`ProSettingsBag`] and [`pro_setting_flag`] moved home with it; the
//! re-export below keeps this module's pre-T335 path stable (the T155
//! `lib_helpers` absorption precedent).
//!
//! ## What maps where
//!
//! - The `codeFolding` key travels the *same* generic settings bag every
//!   other privileged key rides (`readProSettingsBag` →
//!   [`pro_setting_flag`]), not a bespoke channel.
//! - `proSettingFlag` treats a missing key **and any non-boolean** as the
//!   fallback: a hand-edited or corrupted bag cannot silently switch a
//!   feature off that the user believes is on. The fallback for folding
//!   is `true` (folding is the common case).
//! - One flag, one value: the host gate and the extension gate read the
//!   same key on the same bag — an earlier draft's `enabled` override
//!   gave one setting two readers with contradictory instructions, and a
//!   test caught exactly that. [`FoldCompartmentPayload::Empty`]
//!   structurally cannot forward a bag, so the contradiction cannot be
//!   expressed.
//! - `[]` when off reconfigures the slot to empty rather than unmounting
//!   it: the slot stays mounted so flipping the setting back on is an
//!   ordinary reconfigure, not a remount that would lose the editor's
//!   scroll and selection.
//! - `createFoldExtensions`'s registry half (the extension lookup, the
//!   degrade-to-`[]` when the EP is absent or does not implement it,
//!   the throwing-factory circuit breaker) is T335's machinery; the
//!   engine payload here is the decision, with the bag forwarded
//!   verbatim when folding is on.
//!
//! Known deltas: none behavioural — the gate's rules are the React
//! file's verbatim.

/// `ProSettingsBag` (proCompartments.ts, its definition home since T335):
/// the privileged settings object as persisted under
/// `driverSettings[extensionId]` (a JSON object; the flag reader
/// tolerates anything else exactly as JS's `bag?.[key]` yields
/// undefined). Re-exported at its pre-T335 path.
pub(crate) use super::pro_compartments::ProSettingsBag;

/// `proSettingFlag` (proCompartments.ts, its definition home since T335;
/// re-exported at its pre-T335 path): read a boolean setting, falling
/// back when the key is absent **or non-boolean** — only a real boolean
/// switches, so settings corruption degrades to the documented default
/// instead of silently disabling a feature.
pub(crate) use super::pro_compartments::pro_setting_flag;

/// `foldSettingEnabled`: read the `codeFolding` flag off the shared bag.
/// The fallback is `true` — folding is the common case, and a corrupted
/// bag should not remove it.
pub(crate) fn fold_setting_enabled(pro_settings: Option<&ProSettingsBag>) -> bool {
    pro_setting_flag(pro_settings, "codeFolding", true)
}

/// What the `fold` compartment slot holds after the host's decision.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FoldCompartmentPayload {
    /// Folding is switched off: the slot reconfigures to **empty** (it
    /// stays mounted — `[]`, not an unmount — so flipping back on is an
    /// ordinary reconfigure). No bag is forwarded: the extension sees
    /// nothing, so no instruction can contradict the gate.
    Empty,
    /// Folding is on: the extension's payload (T335's registry builds
    /// it; a missing or non-implementing EP degrades to empty there).
    /// The bag is forwarded **verbatim** — the same key the gate read,
    /// the same object the extension reads.
    Extension { pro_settings: Option<ProSettingsBag> },
}

/// `createFoldCompartmentExtensions`: build the `fold` compartment
/// payload. There is deliberately one flag with one value — the
/// `codeFolding` key on the shared bag.
pub(crate) fn create_fold_compartment_extensions(
    pro_settings: Option<&ProSettingsBag>,
) -> FoldCompartmentPayload {
    if !fold_setting_enabled(pro_settings) {
        return FoldCompartmentPayload::Empty;
    }
    FoldCompartmentPayload::Extension { pro_settings: pro_settings.cloned() }
}
