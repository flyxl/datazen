//! The failures the named-view rules can produce.

/// Why a named-view operation was refused.
///
/// The two rule failures carry a `grid.view.*` prefix, and the prefix is the
/// **first** thing in the message: the frontend classifies on it (00-contracts
/// §8.2), and a prefix that is not at the start is not a prefix.
///
/// The storage failure carries **no** `grid.` prefix on purpose: the error
/// code space is closed (45 codes, 00-contracts §8.2), and a disk that cannot
/// be written is not one of the grid rules. The frontend shows its raw text.
#[derive(Debug, thiserror::Error)]
pub enum NamedViewError {
    /// A view with this name already exists in the scope.
    #[error("grid.view.nameExists: a saved view named '{0}' already exists for this table")]
    NameExists(String),
    /// The record does not satisfy the shared rules.
    #[error("grid.view.invalid: {0}")]
    Invalid(String),
    /// The store could not be read or written.
    #[error("named view storage failed: {0}")]
    Storage(String),
}

impl NamedViewError {
    /// The contract code of this failure, when it has one.
    ///
    /// The frontend branches on the code rather than on the text, so the two
    /// cannot drift apart.
    pub fn code(&self) -> Option<&'static str> {
        match self {
            Self::NameExists(_) => Some("grid.view.nameExists"),
            Self::Invalid(_) => Some("grid.view.invalid"),
            Self::Storage(_) => None,
        }
    }
}
