//! Port of `packages/frontend/src/hooks/useLocaleDomains.ts` (T363) —
//! ensure lazy i18n domain packs are loaded for the active language.
//!
//! The effective dependency is a stable string key over the SORTED
//! domain list, NOT the `domains` array identity — callers pass inline
//! literals like `useLocaleDomains(['sync'])` and would otherwise
//! re-run the effect on every render (a new array reference →
//! `setReady(false)` → the re-import flash loop the React header
//! records). [`domains_key`] is that key, verbatim
//! (`[...domains].sort().join('\0')`).
//!
//! ## What maps where
//!
//! - `useSettingsStore((s) => s.settings?.language) ?? 'en'` is the
//!   host-delivered language ([`UseLocaleDomainsState::begin`] takes
//!   it; the host re-calls on a change — `language` is in the effect's
//!   deps so switching language re-ensures the packs).
//! - `ensureLocaleDomains(language, list)` is the host seam under the
//!   T018 ruling: the zh-CN table — the lazy domains included — is
//!   compiled in by `build.rs`, so the ensure is host-instant and the
//!   host answers through [`UseLocaleDomainsState::note_ensured`]
//!   (the `cancelled` guard of a fast unmount collapses into the
//!   host not delivering).
//! - The `ready` gate + the `setReady(false)` re-run on a changed
//!   `(language, key)` pair are the state machine, verbatim.
//! - `LocaleDomainLoading` (the not-ready stand-in) is the caller's
//!   concern — the T018-elided sections render nothing for it; the
//!   hook's port only answers the boolean.
//!
//! Known deltas: none behavioural — the key rule and the re-ensure
//! rule are the React file's verbatim; the loading itself is the
//! documented host seam.

// No production consumer yet (the T018-elided sections render nothing
// for the gate); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

/// One lazy domain (React `LazyDomain`: `'sync' | 'workflows' |
/// 'dashboard' | 'mcp'`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LazyDomain {
    Sync,
    Workflows,
    Dashboard,
    Mcp,
}

impl LazyDomain {
    /// The pack spelling (the domains.ts literal).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sync => "sync",
            Self::Workflows => "workflows",
            Self::Dashboard => "dashboard",
            Self::Mcp => "mcp",
        }
    }
}

/// `LAZY_DOMAINS` (the closed list).
pub const LAZY_DOMAINS: [LazyDomain; 4] = [
    LazyDomain::Sync,
    LazyDomain::Workflows,
    LazyDomain::Dashboard,
    LazyDomain::Mcp,
];

/// The stable string key over the domain list:
/// `[...domains].sort().join('\0')` — an inline `['sync']` is a new
/// array every render, but the key stays identical, so the effect
/// does not re-run.
pub fn domains_key(domains: &[LazyDomain]) -> String {
    let mut sorted: Vec<&'static str> = domains.iter().map(|d| d.as_str()).collect();
    sorted.sort_unstable();
    sorted.join("\0")
}

/// The hook's state: the ready gate over the host's ensure answers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UseLocaleDomainsState {
    /// `ready` — flipped by the host's ensure answer; re-armed to
    /// false whenever the `(language, key)` pair changes.
    ready: bool,
    /// The last effect run's `(language, key)` — the dedup the effect
    /// deps perform.
    last_run: Option<(String, String)>,
}

impl UseLocaleDomainsState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The hook's read (`ready`).
    pub fn ready(&self) -> bool {
        self.ready
    }

    /// The mount / dep-change effect: returns whether the ensure must
    /// run for `(language, domains)` — the pair changed (or this is
    /// the mount) — and re-arms the gate when it does. A re-render
    /// with the SAME pair (the inline-literal case) answers `false`
    /// and leaves the gate alone.
    pub fn begin(&mut self, language: &str, domains: &[LazyDomain]) -> bool {
        let key = domains_key(domains);
        if self
            .last_run
            .as_ref()
            .is_some_and(|(ran_language, ran_key)| ran_language == language && *ran_key == key)
        {
            return false;
        }
        self.last_run = Some((language.to_owned(), key));
        self.ready = false;
        true
    }

    /// `ensureLocaleDomains(...).then(() => setReady(true))` — the
    /// host's ensure answer (instant under T018: the packs are
    /// compiled in).
    pub fn note_ensured(&mut self) {
        self.ready = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_sorts_and_joins_with_nul() {
        assert_eq!(domains_key(&[LazyDomain::Sync]), "sync");
        assert_eq!(
            domains_key(&[LazyDomain::Mcp, LazyDomain::Sync]),
            "mcp\0sync",
            "sorted lexicographically (JS .sort()) regardless of the caller's order"
        );
        assert_eq!(domains_key(&[]), "");
        // The same SET in different orders keys identically — the
        // inline-literal flash-loop guard.
        assert_eq!(
            domains_key(&[LazyDomain::Dashboard, LazyDomain::Sync, LazyDomain::Mcp]),
            domains_key(&[LazyDomain::Mcp, LazyDomain::Dashboard, LazyDomain::Sync]),
        );
    }

    #[test]
    fn the_gate_arms_on_a_changed_pair_only() {
        let mut state = UseLocaleDomainsState::new();
        // The mount runs.
        assert!(state.begin("en", &[LazyDomain::Sync]));
        assert!(!state.ready());
        state.note_ensured();
        assert!(state.ready());

        // A re-render with the same pair (the inline literal) does not
        // re-run — and never drops the gate.
        assert!(!state.begin("en", &[LazyDomain::Sync]));
        assert!(state.ready(), "no flash");

        // A new domain list re-runs and re-arms the gate.
        assert!(state.begin("en", &[LazyDomain::Sync, LazyDomain::Mcp]));
        assert!(!state.ready(), "re-armed while the ensure is out");
        state.note_ensured();
        assert!(state.ready());

        // A language switch re-ensures the SAME domains (the effect's
        // `language` dep).
        assert!(state.begin("zh-CN", &[LazyDomain::Sync, LazyDomain::Mcp]));
        assert!(!state.ready());
        state.note_ensured();
        assert!(state.ready());
        assert!(!state.begin("zh-CN", &[LazyDomain::Sync, LazyDomain::Mcp]));
    }

    #[test]
    fn the_domain_vocabulary_is_the_closed_list() {
        assert_eq!(
            LAZY_DOMAINS.map(|d| d.as_str()),
            ["sync", "workflows", "dashboard", "mcp"],
        );
    }
}
