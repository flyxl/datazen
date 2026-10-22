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
    ///
    /// **The message does not carry the name.** The name is whatever the user
    /// typed, and a host message travels into the IPC payload and into the
    /// host's log lines — where the redactor does not cover free text (it
    /// covers the shapes it documents: JSON pairs, query strings, bearer
    /// tokens, URL userinfo). A name like `password=hunter2` would therefore
    /// travel verbatim.
    ///
    /// So the rule is: **a host message says only what the host knows.** The
    /// name stays here as structured data, reachable through
    /// [`NamedViewError::name`], and the frontend shows its own localized
    /// wording anyway (the code is what it branches on).
    #[error("grid.view.nameExists: a saved view with this name already exists for this table")]
    NameExists { name: String },
    /// The record does not satisfy the shared rules.
    ///
    /// The reason is the host's own wording and never quotes what the user
    /// typed, for the same reason as above.
    #[error("grid.view.invalid: {0}")]
    Invalid(String),
    /// The store could not be read or written.
    ///
    /// The text is an I/O message: it names files, and the host masks the
    /// absolute paths on the way out.
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
            Self::NameExists { .. } => Some("grid.view.nameExists"),
            Self::Invalid(_) => Some("grid.view.invalid"),
            Self::Storage(_) => None,
        }
    }

    /// The user-supplied name this failure is about, when there is one.
    ///
    /// The name never reaches the message; it stays here, as structured data,
    /// for the caller that wants to say *which* name was taken.
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::NameExists { name } => Some(name.as_str()),
            Self::Invalid(_) | Self::Storage(_) => None,
        }
    }
}
