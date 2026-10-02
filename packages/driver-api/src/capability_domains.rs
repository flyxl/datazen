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

    /// Weaker than nominal on both remaining axes at once: rows can be read,
    /// but only through a fully materialized buffer, and the resource is
    /// read-only.
    ///
    /// Neither neighbour describes this driver. [`DataSupport::StreamingReadOnly`]
    /// streams a resource it may write; [`DataSupport::BufferedReadWrite`]
    /// writes a resource it streams. A caller offered only those two has to
    /// pick one and is then wrong about an axis — so the combination is named
    /// here rather than left for the caller to re-derive.
    ///
    /// Declaring [`DataSupport::Unsupported`] would be a false measured
    /// refusal: the driver demonstrably reads.
    ///
    /// The caller must not open a long-lived stream against this, and must not
    /// issue writes into it.
    BufferedReadOnly,

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
    ///
    /// [`DataSupport::BufferedReadOnly`] reads rows exactly like the other
    /// measured variants; that its rows are buffered is the
    /// [`Self::enables_streaming_results`] axis, not this one.
    pub fn enables_row_read(self) -> bool {
        matches!(
            self,
            Self::StreamingReadWrite
                | Self::StreamingReadOnly
                | Self::BufferedReadWrite
                | Self::BufferedReadOnly
        )
    }

    /// Rows can be written. False for [`DataSupport::StreamingReadOnly`],
    /// [`DataSupport::BufferedReadOnly`], [`DataSupport::Unsupported`] and
    /// [`DataSupport::Unknown`].
    ///
    /// [`DataSupport::BufferedReadOnly`] is listed here deliberately rather
    /// than by omission: its being buffered says nothing about writes, and it
    /// is read-only for the same reason [`DataSupport::StreamingReadOnly`] is.
    pub fn enables_row_write(self) -> bool {
        matches!(self, Self::StreamingReadWrite | Self::BufferedReadWrite)
    }

    /// Results arrive incrementally. False for
    /// [`DataSupport::BufferedReadWrite`] and [`DataSupport::BufferedReadOnly`],
    /// which must materialize first.
    ///
    /// [`DataSupport::BufferedReadOnly`] is excluded by name because a
    /// read-only resource buffers the same way a writable one does; folding
    /// the exclusion into [`Self::enables_row_write`] would hide that it is
    /// two independent reasons landing in the same place.
    pub fn enables_streaming_results(self) -> bool {
        matches!(self, Self::StreamingReadWrite | Self::StreamingReadOnly)
    }

    /// The nominal guarantee, and nothing weaker.
    ///
    /// [`DataSupport::BufferedReadOnly`] cannot qualify on either of the two
    /// weaker variants' terms, so it is absent rather than merely unchecked.
    pub fn enables_feature(self) -> bool {
        matches!(self, Self::StreamingReadWrite)
    }

    /// Measured and degraded, rather than absent or unmeasured.
    ///
    /// [`DataSupport::BufferedReadOnly`] is the weakest measured answer, not
    /// the weakest answer overall: [`DataSupport::Unsupported`] and
    /// [`DataSupport::Unknown`] are below it and are not degraded, because
    /// nobody measured them to be.
    pub fn is_weaker_than_nominal(self) -> bool {
        matches!(
            self,
            Self::StreamingReadOnly | Self::BufferedReadWrite | Self::BufferedReadOnly
        )
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

    const DATA_VARIANTS: [DataSupport; 6] = [
        DataSupport::StreamingReadWrite,
        DataSupport::StreamingReadOnly,
        DataSupport::BufferedReadWrite,
        DataSupport::BufferedReadOnly,
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
    fn every_data_variant_answers_all_six_functions_explicitly() {
        // There is deliberately no wildcard arm in this `match`. Adding a
        // `DataSupport` variant breaks it at compile time, which forces the
        // author to decide where the new answer lands instead of letting it
        // fall through every `matches!` above and quietly become "enables
        // nothing". This is the only compile-time link between the enum and
        // the six predicates; `DATA_VARIANTS` below is the iteration surface,
        // and a variant added here but not there would compile.
        let answers = |variant: DataSupport| -> [bool; 6] {
            match variant {
                // row_read, row_write, streaming, feature, weaker, undeclared
                DataSupport::StreamingReadWrite => [true, true, true, true, false, false],
                DataSupport::StreamingReadOnly => [true, false, true, false, true, false],
                DataSupport::BufferedReadWrite => [true, true, false, false, true, false],
                DataSupport::BufferedReadOnly => [true, false, false, false, true, false],
                DataSupport::Unsupported => [false, false, false, false, false, false],
                DataSupport::Unknown => [false, false, false, false, false, true],
            }
        };
        for variant in DATA_VARIANTS {
            let [read, write, stream, feature, weaker, undeclared] = answers(variant);
            assert_eq!(variant.enables_row_read(), read, "{variant:?} row_read");
            assert_eq!(variant.enables_row_write(), write, "{variant:?} row_write");
            assert_eq!(
                variant.enables_streaming_results(),
                stream,
                "{variant:?} streaming_results"
            );
            assert_eq!(variant.enables_feature(), feature, "{variant:?} feature");
            assert_eq!(
                variant.is_weaker_than_nominal(),
                weaker,
                "{variant:?} weaker_than_nominal"
            );
            assert_eq!(
                variant.is_undeclared(),
                undeclared,
                "{variant:?} undeclared"
            );
        }
    }

    #[test]
    fn buffered_read_only_is_weaker_on_both_remaining_axes() {
        let degraded = DataSupport::BufferedReadOnly;
        assert!(degraded.is_weaker_than_nominal());
        assert!(
            degraded.enables_row_read(),
            "buffering a read is still a read"
        );
        assert!(!degraded.enables_row_write(), "the resource is read-only");
        assert!(
            !degraded.enables_streaming_results(),
            "a buffered result set has nothing to pull incrementally"
        );
        assert!(!degraded.enables_feature(), "degraded is not nominal");
        assert!(!degraded.is_undeclared());
        // The whole reason to name it: it is neither of its two neighbours, so
        // a caller cannot get it right by reusing either one.
        assert_ne!(degraded, DataSupport::StreamingReadOnly);
        assert_ne!(degraded, DataSupport::BufferedReadWrite);
        assert_ne!(degraded, DataSupport::Unsupported);
    }

    #[test]
    fn a_measured_read_only_variant_is_not_a_refusal() {
        // `BufferedReadOnly` is the case that had no home before: the driver
        // reads, measurably, so declaring `Unsupported` would be a refusal
        // nobody made. This asserts the distinction the new variant exists to
        // preserve.
        let measured = DataSupport::BufferedReadOnly;
        assert!(measured.enables_row_read());
        assert!(!DataSupport::Unsupported.enables_row_read());
        assert!(measured.is_weaker_than_nominal());
        assert!(!DataSupport::Unsupported.is_weaker_than_nominal());
        assert!(!DataSupport::Unknown.is_weaker_than_nominal());
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
        // `bufferedReadOnly` is a new string in a serialized capability
        // snapshot, so it is pinned the same way the others are.
        assert_eq!(
            serde_json::to_string(&DataSupport::BufferedReadOnly).expect("serialize"),
            "\"bufferedReadOnly\""
        );
        assert_eq!(
            serde_json::from_str::<DataSupport>("\"bufferedReadOnly\"").expect("deserialize"),
            DataSupport::BufferedReadOnly
        );
    }
}
