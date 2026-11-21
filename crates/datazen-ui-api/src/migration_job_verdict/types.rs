#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelDisposition {
    None,
    NotStarted,
    RequestedInFlight,
    SettledPartially,
    SettledRolledBack,
    SettledCompleted,
    SettledUnknown,
}

impl CancelDisposition {
    pub fn as_str(self) -> &'static str {
        match self {
            CancelDisposition::None => "none",
            CancelDisposition::NotStarted => "notStarted",
            CancelDisposition::RequestedInFlight => "requestedInFlight",
            CancelDisposition::SettledPartially => "settledPartially",
            CancelDisposition::SettledRolledBack => "settledRolledBack",
            CancelDisposition::SettledCompleted => "settledCompleted",
            CancelDisposition::SettledUnknown => "settledUnknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerdictSeverity {
    Ok,
    Partial,
    Uncertain,
    Failed,
}

impl VerdictSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            VerdictSeverity::Ok => "ok",
            VerdictSeverity::Partial => "partial",
            VerdictSeverity::Uncertain => "uncertain",
            VerdictSeverity::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UncertaintyReason {
    None,
    NoCheckpoint,
    RecoveryRejected,
    EffectOutcomeUnknown,
    MissingEvidence,
    ManualReviewRequired,
}

impl UncertaintyReason {
    pub fn as_str(self) -> &'static str {
        match self {
            UncertaintyReason::None => "none",
            UncertaintyReason::NoCheckpoint => "noCheckpoint",
            UncertaintyReason::RecoveryRejected => "recoveryRejected",
            UncertaintyReason::EffectOutcomeUnknown => "effectOutcomeUnknown",
            UncertaintyReason::MissingEvidence => "missingEvidence",
            UncertaintyReason::ManualReviewRequired => "manualReviewRequired",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitBoundaryFacts {
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum JobCounterValue {
    Number(f64),
    Text(String),
    Null,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MigrationJobVerdictInput {
    pub state: String,
    pub effect_outcome: Option<String>,
    pub cancel_requested: bool,
    pub commit_boundaries: Option<Vec<CommitBoundaryFacts>>,
    pub recovery_verdict: Option<String>,
    pub recovery_reason: Option<String>,
    pub error: Option<String>,
    pub committed_rows: Option<i64>,
    pub unknown_rows: Option<i64>,
}

impl Default for MigrationJobVerdictInput {
    fn default() -> Self {
        Self {
            state: String::new(),
            effect_outcome: None,
            cancel_requested: false,
            commit_boundaries: None,
            recovery_verdict: None,
            recovery_reason: None,
            error: None,
            committed_rows: None,
            unknown_rows: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationJobVerdict {
    pub severity: VerdictSeverity,
    pub cancel_disposition: CancelDisposition,
    pub completed: bool,
    pub verified_boundaries: usize,
    pub unverified_boundaries: usize,
    pub uncertainty: UncertaintyReason,
    pub requires_reconcile: bool,
}

pub const PIPELINE_BUDGET_BYTES: usize = 8 * 1024 * 1024;
