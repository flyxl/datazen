//! Storage port owned by the memory domain.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::domain::{MemoryEntry, MemoryScope, MemoryStatus};

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("memory repository error: {0}")]
    Repository(String),
}

/// Read-side query passed to the host. `scopes` contains the exact target,
/// ancestor, user, and organization scopes in specificity order; `statuses`
/// already reflects whether candidate preview is enabled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryQuery {
    pub text: String,
    pub scopes: Vec<MemoryScope>,
    pub statuses: Vec<MemoryStatus>,
    pub limit: usize,
}

/// Host persistence boundary. The desktop host implements keyword/FTS recall
/// and scope filtering in T12; the shared layer owns ordering, scoring,
/// conflict marking, and prompt formatting.
#[async_trait]
pub trait MemoryRepository: Send + Sync {
    /// Recall candidates for `query`. Hosts may return a superset; the
    /// retriever filters status/scope and scores deterministically.
    async fn candidates(&self, query: &MemoryQuery) -> Result<Vec<MemoryEntry>, MemoryError>;

    /// List memories belonging to one exact scope, optionally filtered by
    /// status. Used by T12/T13 list/search commands.
    async fn list_by_scope(
        &self,
        scope: &MemoryScope,
        status: Option<MemoryStatus>,
    ) -> Result<Vec<MemoryEntry>, MemoryError>;
}
