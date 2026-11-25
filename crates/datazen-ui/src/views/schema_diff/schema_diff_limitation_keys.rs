//! Port of
//! `packages/frontend/src/windows/schema-diff/schemaDiffLimitationKeys.ts`
//! (T323): the schema-diff limitations dialog's key list.
//!
//! The React file is a single `as const` array of six i18n keys. Its only
//! consumer, `SchemaDiffWindow.tsx`, hands it to the shared
//! `LimitationsDialog` (`components/ui/LimitationsDialog.tsx`) as
//! `limitationKeys` alongside `titleKey="schemaDiff.limitations.title"` and
//! `dontShowAgainKey="schemaDiff.limitations.dontShowAgain"` (those two
//! siblings belong to the window's own port, not this file). The dialog
//! maps the list **in order** — each key is one bullet of the pre-deploy
//! limitations notice, so the order is behavior.
//!
//! ## What maps where
//!
//! - `SCHEMA_DIFF_LIMITATION_KEYS` (the `as const` array) →
//!   [`SCHEMA_DIFF_LIMITATION_KEYS`], a `const` array with the keys
//!   verbatim and in the React order. The consumer seam: the schema-diff
//!   window / limitations-dialog port (a later task) iterates it and
//!   renders `crate::i18n::t(key)` per bullet.
//! - No consumer has landed yet — the deploy panel (T025,
//!   `schema_diff_deploy_panel.rs`) does not read this list (React's
//!   deploy panel never did; the keys feed the window-level dialog), so
//!   the const carries the same dead-code silence the deploy panel's own
//!   reusable helpers use.
//!
//! ## Keys
//!
//! The six i18n keys stay verbatim (see the const). There are no
//! `data-testid` values in the React file — the dialog's `schema-diff`
//! testid prefix belongs to the dialog component's own port.

/// React `SCHEMA_DIFF_LIMITATION_KEYS` (`schemaDiffLimitationKeys.ts:1`):
/// the six bullets of the schema-diff limitations notice, in the React
/// order (no views, no online ALTER, no rename guessing, cross-dialect
/// risk, no auto backup, no MCP deploy).
#[allow(dead_code)] // reusable port; wired by the schema-diff window's limitations dialog
pub const SCHEMA_DIFF_LIMITATION_KEYS: [&str; 6] = [
    "schemaDiff.limitations.noViews",
    "schemaDiff.limitations.noOnlineAlter",
    "schemaDiff.limitations.noRenameGuess",
    "schemaDiff.limitations.crossDialect",
    "schemaDiff.limitations.noAutoBackup",
    "schemaDiff.limitations.noMcpDeploy",
];

#[doc(hidden)]  // exposed for integration tests
pub mod tests {
    use super::*;

    /// The six keys, verbatim and in the React order — the dialog maps the
    /// list in order, one bullet per key.
    #[test]
    fn the_six_keys_are_verbatim_and_ordered() {
        assert_eq!(
            SCHEMA_DIFF_LIMITATION_KEYS,
            [
                "schemaDiff.limitations.noViews",
                "schemaDiff.limitations.noOnlineAlter",
                "schemaDiff.limitations.noRenameGuess",
                "schemaDiff.limitations.crossDialect",
                "schemaDiff.limitations.noAutoBackup",
                "schemaDiff.limitations.noMcpDeploy",
            ]
        );
    }

    /// Every key sits under the one `schemaDiff.limitations.` namespace
    /// (the dialog's `titleKey` / `dontShowAgainKey` siblings) and the
    /// list has no duplicates — a repeated key would render its bullet
    /// twice.
    #[test]
    fn every_key_is_a_unique_limitations_key() {
        assert_eq!(SCHEMA_DIFF_LIMITATION_KEYS.len(), 6);
        for key in SCHEMA_DIFF_LIMITATION_KEYS {
            assert!(key.starts_with("schemaDiff.limitations."), "{key} is not a limitations key");
            assert_eq!(
                SCHEMA_DIFF_LIMITATION_KEYS.iter().filter(|k| **k == key).count(),
                1,
                "duplicate key {key}"
            );
        }
    }
}
