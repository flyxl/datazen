//! `data` / `backup` the two remaining optional-capability domains on
//! [`CapabilitySet`](crate::CapabilitySet).
//!
//! [`CapabilitySet`](crate::CapabilitySet) already carried `namespace`,
//! `session`, `transaction` and `snapshot`; `platform-development-plan.md:93`
//! requires all six, and `data` / `backup` were the two gaps. This file is the
//! home for those two domains, split out of `capabilities.rs` so neither file
//! grows past the 800-line ceiling in AGENTS.md.
//!
//! ## Why the vocabulary here is not a pair of booleans
//!
//! `data` is three keys in one declaration (`rowRead` / `rowWrite` /
//! `streamingResults`) and `backup` is two (`artifact` / `restore`). Neither
//! is a clean yes/no: a driver can read every row but only through a fully
//! materialized buffer, or it can produce a backup artifact but never consume
//! one back. Flattening that to `bool` would force the caller to re-derive the
//! distinction the driver already knows, and would make the degraded path a
//! silent no-op — the exact defect
//! `driver-capability-migration.md` §6.1 and the [`CapabilityError`]
//! rejection surface exist to prevent.
//!
//! ## Three states, not two
//!
//! `Unknown` is a first-class variant, never a stand-in for "not supported":
//!
//! * `Unknown` — nothing has been declared. A driver that has not been
//!   migrated lands here, and it stays here until someone measures it.
//! * `Unsupported` — someone measured and it is genuinely not there.
//!
//! Collapsing the two would let an unmeasured driver masquerade as a
//! measured refusal, and the difference is what tells the host whether it may
//! retry later. Both open no feature, so the fail-closed property is the
//! same; the diagnosis is not.
//!
//! [`Weaker`]: Both enums carry their degraded values as named variants rather
//! than a `Weaker(bool)` wrapper, matching how `SnapshotSupport::PerTable`
//! already names a weaker snapshot than `SnapshotSupport::Coordinated`. The
//! caller can therefore branch on *how* it is weaker instead of only *that* it
//! is weaker.
//!
//! The use-case-layer mirror of this vocabulary is
//! `packages/application/src/capability/domain.rs`; the two stay separate
//! because that one is keyed by a per-key `CapabilityKey` map while this one
//! is what a provider can express as a single self-consistent answer.

use serde::{Deserialize, Serialize};

/// The `data` domain: `rowRead` / `rowWrite` / `streamingResults` in one
/// self-consistent declaration.
///
/// Ordered from nominal to weakest. Every variant except [`DataSupport::Unknown`]
/// is an answer somebody had to measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DataSupport {
    /// Nominal. Rows can be read, written, and streamed incrementally.
    ///
    /// The only variant that satisfies [`DataSupport::enables_feature`].
    StreamingReadWrite,

    /// Weaker than nominal on one axis: reads stream, but the resource is
    /// read-only, so `rowWrite` does not hold.
    ///
    /// A read-only resource is not a degraded read-write resource, and a
    /// caller that treats it as one will issue writes into a sink that drops
    /// them.
    StreamingReadOnly,

    /// Weaker than nominal on a different axis: rows can be read and written,
    /// but a result set must be fully materialized before any of it is
    /// returned, so `streamingResults` does not hold.
    ///
    /// The caller must not open a long-lived stream against this; there is
    /// nothing to pull incrementally.
    BufferedReadWrite,

    /// Measured and genuinely absent. A caller may stop asking and surface
    /// the refusal as a permanent one.
    Unsupported,

    /// Nothing has been declared. The [`Default`].
    ///
    /// An un-migrated driver lands here and therefore claims nothing.
    Unknown,
}

impl DataSupport {
    /// Rows can be read at all. True for every measured variant.
    pub fn enables_row_read(self) -> bool {
        matches!(
            self,
            Self::StreamingReadWrite | Self::StreamingReadOnly | Self::BufferedReadWrite
        )
    }

    /// Rows can be written. False for [`DataSupport::StreamingReadOnly`],
    /// [`DataSupport::Unsupported`] and [`DataSupport::Unknown`].
    pub fn enables_row_write(self) -> bool {
        matches!(self, Self::StreamingReadWrite | Self::BufferedReadWrite)
    }

    /// Results arrive incrementally. False for
    /// [`DataSupport::BufferedReadWrite`], which must materialize first.
    pub fn enables_streaming_results(self) -> bool {
        matches!(self, Self::StreamingReadWrite | Self::StreamingReadOnly)
    }

    /// The nominal guarantee, and nothing weaker.
    pub fn enables_feature(self) -> bool {
        matches!(self, Self::StreamingReadWrite)
    }

    /// Measured and degraded, rather than absent or unmeasured.
    pub fn is_weaker_than_nominal(self) -> bool {
        matches!(self, Self::StreamingReadOnly | Self::BufferedReadWrite)
    }

    /// Nothing was ever declared. Distinct from
    /// [`DataSupport::Unsupported`], which is a measured refusal.
    pub fn is_undeclared(self) -> bool {
        matches!(self, Self::Unknown)
    }
}

impl Default for DataSupport {
    fn default() -> Self {
        Self::Unknown
    }
}

/// The `backup` domain: `artifact` / `restore` in one self-consistent
/// declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackupSupport {
    /// Nominal. The driver produces a backup artifact and consumes one back.
    ///
    /// The only variant that satisfies [`BackupSupport::enables_feature`].
    ArtifactAndRestore,

    /// Weaker than nominal on the restore axis: it can produce an artifact
    /// but never read one, so `restore` does not hold.
    ///
    /// A backup driver that cannot restore is still worth a backup tab; the
    /// caller must not offer a restore affordance for it.
    ArtifactOnly,

    /// Weaker than nominal on the artifact axis: it consumes a backup
    /// artifact someone else produced, but does not produce its own, so
    /// `artifact` does not hold.
    RestoreOnly,

    /// Measured and genuinely absent.
    Unsupported,

    /// Nothing has been declared. The [`Default`].
    ///
    /// An un-migrated driver lands here and therefore claims nothing.
    Unknown,
}

impl BackupSupport {
    /// The driver can produce a backup artifact.
    pub fn enables_artifact(self) -> bool {
        matches!(self, Self::ArtifactAndRestore | Self::ArtifactOnly)
    }

    /// The driver can consume a backup artifact back into a resource.
    pub fn enables_restore(self) -> bool {
        matches!(self, Self::ArtifactAndRestore | Self::RestoreOnly)
    }

    /// The nominal guarantee, and nothing weaker.
    pub fn enables_feature(self) -> bool {
        matches!(self, Self::ArtifactAndRestore)
    }

    /// Measured and degraded, rather than absent or unmeasured.
    pub fn is_weaker_than_nominal(self) -> bool {
        matches!(self, Self::ArtifactOnly | Self::RestoreOnly)
    }

    /// Nothing was ever declared. Distinct from
    /// [`BackupSupport::Unsupported`], which is a measured refusal.
    pub fn is_undeclared(self) -> bool {
        matches!(self, Self::Unknown)
    }
}

impl Default for BackupSupport {
    fn default() -> Self {
        Self::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATA_VARIANTS: [DataSupport; 5] = [
        DataSupport::StreamingReadWrite,
        DataSupport::StreamingReadOnly,
        DataSupport::BufferedReadWrite,
        DataSupport::Unsupported,
        DataSupport::Unknown,
    ];

    const BACKUP_VARIANTS: [BackupSupport; 5] = [
        BackupSupport::ArtifactAndRestore,
        BackupSupport::ArtifactOnly,
        BackupSupport::RestoreOnly,
        BackupSupport::Unsupported,
        BackupSupport::Unknown,
    ];

    #[test]
    fn default_data_support_is_unknown_not_unsupported() {
        // "Nobody declared it" and "somebody measured and it is absent" are
        // different diagnoses, so the default must not collapse into
        // `Unsupported`.
        assert_eq!(DataSupport::default(), DataSupport::Unknown);
        assert!(DataSupport::default().is_undeclared());
        assert!(!DataSupport::Unsupported.is_undeclared());
    }

    #[test]
    fn default_backup_support_is_unknown_not_unsupported() {
        assert_eq!(BackupSupport::default(), BackupSupport::Unknown);
        assert!(BackupSupport::default().is_undeclared());
        assert!(!BackupSupport::Unsupported.is_undeclared());
    }

    #[test]
    fn undeclared_opens_no_data_feature() {
        for variant in [DataSupport::Unknown, DataSupport::Unsupported] {
            assert!(
                !variant.enables_row_read(),
                "{variant:?} must not enable row_read"
            );
            assert!(
                !variant.enables_row_write(),
                "{variant:?} must not enable row_write"
            );
            assert!(
                !variant.enables_streaming_results(),
                "{variant:?} must not enable streaming_results"
            );
            assert!(
                !variant.enables_feature(),
                "{variant:?} must not enable the nominal feature"
            );
            assert!(
                !variant.is_weaker_than_nominal(),
                "{variant:?} is not degraded"
            );
        }
    }

    #[test]
    fn undeclared_opens_no_backup_feature() {
        for variant in [BackupSupport::Unknown, BackupSupport::Unsupported] {
            assert!(
                !variant.enables_artifact(),
                "{variant:?} must not enable artifact"
            );
            assert!(
                !variant.enables_restore(),
                "{variant:?} must not enable restore"
            );
            assert!(
                !variant.enables_feature(),
                "{variant:?} must not enable the nominal feature"
            );
            assert!(
                !variant.is_weaker_than_nominal(),
                "{variant:?} is not degraded"
            );
        }
    }

    #[test]
    fn read_only_is_weaker_on_the_write_axis_only() {
        let degraded = DataSupport::StreamingReadOnly;
        assert!(degraded.is_weaker_than_nominal());
        assert!(degraded.enables_row_read());
        assert!(degraded.enables_streaming_results());
        assert!(
            !degraded.enables_row_write(),
            "a read-only resource takes no writes"
        );
        assert!(!degraded.enables_feature(), "degraded is not nominal");
    }

    #[test]
    fn buffered_is_weaker_on_the_streaming_axis_only() {
        let degraded = DataSupport::BufferedReadWrite;
        assert!(degraded.is_weaker_than_nominal());
        assert!(degraded.enables_row_read());
        assert!(degraded.enables_row_write());
        assert!(
            !degraded.enables_streaming_results(),
            "a buffered result set has nothing to pull incrementally"
        );
        assert!(!degraded.enables_feature(), "degraded is not nominal");
    }

    #[test]
    fn artifact_only_backup_can_never_restore() {
        let degraded = BackupSupport::ArtifactOnly;
        assert!(degraded.is_weaker_than_nominal());
        assert!(degraded.enables_artifact());
        assert!(!degraded.enables_restore());
        assert!(!degraded.enables_feature());
    }

    #[test]
    fn restore_only_backup_can_never_produce_an_artifact() {
        let degraded = BackupSupport::RestoreOnly;
        assert!(degraded.is_weaker_than_nominal());
        assert!(!degraded.enables_artifact());
        assert!(degraded.enables_restore());
        assert!(!degraded.enables_feature());
    }

    #[test]
    fn exactly_one_variant_of_each_domain_is_nominal() {
        assert_eq!(
            DATA_VARIANTS.iter().filter(|v| v.enables_feature()).count(),
            1
        );
        assert_eq!(
            BACKUP_VARIANTS
                .iter()
                .filter(|v| v.enables_feature())
                .count(),
            1
        );
    }

    #[test]
    fn a_read_write_capability_always_supports_reading() {
        // The two axes are not independent: you cannot write a row you cannot
        // read back. If a future variant ever breaks this, the guard test
        // fails instead of the mistake reaching a driver.
        for variant in DATA_VARIANTS {
            if variant.enables_row_write() {
                assert!(
                    variant.enables_row_read(),
                    "{variant:?} writes rows it cannot read"
                );
            }
        }
    }

    #[test]
    fn the_wire_names_are_stable() {
        // These strings land in serialized capability snapshots, so a rename
        // here is a protocol change, not a refactor.
        assert_eq!(
            serde_json::to_string(&DataSupport::StreamingReadOnly).expect("serialize"),
            "\"streamingReadOnly\""
        );
        assert_eq!(
            serde_json::to_string(&BackupSupport::ArtifactAndRestore).expect("serialize"),
            "\"artifactAndRestore\""
        );
        assert_eq!(
            serde_json::from_str::<DataSupport>("\"unknown\"").expect("deserialize"),
            DataSupport::Unknown
        );
    }
}
