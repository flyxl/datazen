//! Driver-owned whole-plan transaction proof recipe. SQL stays process-local.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowTransactionPlan {
    /// Versioned driver opt-in, rechecked against the frozen profile before use.
    pub proof_profile: String,
    /// Strictly parsed SQL with every relation bound to the explicit namespace.
    pub statements: Vec<String>,
    /// Executed after BEGIN on the same retained resource, before any plan SQL.
    pub lock_sql: Vec<String>,
    /// Must return exactly one boolean true after locks are held until finish.
    pub metadata_probe_sql: String,
}
