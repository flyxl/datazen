//! Port of `components/sql-editor/paste/createPasteExtensions.ts` (T263,
//! split) — the paste/drop/multiple-selection compartment's host
//! factory (§Track S5-A).
//!
//! The assembly's decision is this module's: the compartment is ALWAYS
//! the multiple-selections base (the community feature, no gate) plus
//! whatever the privileged `sqlEditorEnhancedEP` contributes — the
//! enhanced features (paste-as-IN, drop caret) are supplied via the EP,
//! and community edition does not provide them (a missing or
//! non-implementing EP degrades to nothing, the SafeCompartmentWrapper
//! ruling T335 owns).
//!
//! ## What maps where
//!
//! - `createMultipleSelectionsExtension` is `multipleSelections.ts`'s
//!   (T320, the same directory's own task): the multi-cursor machine
//!   (Mod+D, the pointer chords, rectangular selection, the Escape
//!   precedence table) lands there. Here it is the always-mounted base
//!   flag on [`PasteCompartmentPayload`].
//! - The enhanced half is a seam: [`PasteCompartmentPayload::Enhanced`]
//!   carries the forwarded options verbatim (the connection id for
//!   cross-connection drop validation, the drop callbacks, the
//!   privileged settings bag — the G3 generic path, never a bespoke
//!   channel).
//! - `SafeCompartmentWrapper`'s circuit breaker (a throwing EP factory
//!   degrades to `[]` and unregisters) is T335's machinery.
//!
//! Known deltas: none behavioural — the assembly rule is the React
//! file's verbatim.

/// The forwarded options (React `PasteCompartmentOptions`; the callbacks
/// are the host's seams — the enhanced EP invokes them when a drop
/// lands).
pub struct PasteCompartmentOptions<'a> {
    /// The connection the editor edits (cross-connection drop
    /// validation's input).
    pub connection_id: Option<&'a str>,
    /// The privileged settings bag, forwarded verbatim (G3).
    pub pro_settings: Option<&'a serde_json::Value>,
}

/// What the paste compartment holds after the factory ran.
#[derive(Clone, Debug, PartialEq)]
pub enum PasteCompartmentPayload {
    /// The base alone: the multiple-selections machine (T320's seam)
    /// mounted, the enhanced EP absent or non-implementing — the
    /// community shape.
    BaseOnly,
    /// The base plus the enhanced EP's contributions: the options are
    /// forwarded verbatim for the EP's own factory (paste-as-IN, the
    /// drop caret).
    Enhanced {
        /// The forwarded options bag (the EP reads the same values the
        /// host was given — one source, no overrides).
        connection_id: Option<String>,
        pro_settings: Option<serde_json::Value>,
    },
}

/// `createPasteExtensions`: the compartment is the multiple-selections
/// base plus the enhanced EP's half. `enhanced_registered` is whether a
/// privileged EP implementing `createPasteExtensions` is present (the
/// registry lookup T335 owns; the host answers it).
pub fn create_paste_extensions(
    opts: Option<&PasteCompartmentOptions<'_>>,
    enhanced_registered: bool,
) -> PasteCompartmentPayload {
    if !enhanced_registered {
        // `enhanced.createPasteExtensions?.(opts) ?? []`: a missing or
        // non-implementing EP contributes nothing — the base still
        // mounts (community edition does not provide paste-as-IN).
        return PasteCompartmentPayload::BaseOnly;
    }
    match opts {
        None => PasteCompartmentPayload::Enhanced {
            connection_id: None,
            pro_settings: None,
        },
        Some(opts) => PasteCompartmentPayload::Enhanced {
            connection_id: opts.connection_id.map(str::to_owned),
            pro_settings: opts.pro_settings.cloned(),
        },
    }
}
