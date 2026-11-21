//! T253: `stores/languagePackStore.ts` ported to Rune (74 lines).
//!
//! The installed-language-pack list behind the Settings card and the
//! install dialog: the packs, the loaded flag, the error text, and the
//! two host actions (`fetch` / `remove`) plus the `byCode` lookup.
//!
//! ## Effect seams (the zustand async actions)
//!
//! React's `fetch` / `remove` are async IPC calls; Rune's store is a
//! synchronous state machine, so each action splits into the same
//! three-part shape the `AiStore`'s config effects established:
//!
//! | React action | Rune seam |
//! |---|---|
//! | `fetch` → `listLanguagePacks()` → `set({ packs, loaded, error: null })` | [`LanguagePackStore::begin_fetch`] → [`LanguagePackEffect::ListPacks`] → [`LanguagePackStore::complete_fetch`] |
//! | `fetch`'s catch → `set({ loaded: true, error })` | [`LanguagePackStore::fail_fetch`] |
//! | `remove` → `removeLanguagePack(code)` → `await fetch()` | [`LanguagePackStore::begin_remove`] → [`LanguagePackEffect::RemovePack`] → the host re-runs the fetch seam |
//! | `remove`'s catch → `set({ error })` + `throw` | [`LanguagePackStore::fail_remove`] (the host surfaces the `Err` itself) |
//!
//! ## The changed listener (React 46–74)
//!
//! `ensureLanguagePacksChangedListener` registers the
//! `language-packs:changed` refresh exactly once per module instance:
//! another window installing or removing a pack refreshes this list
//! here. The module-level `subscriptionStarted` flag and its three
//! retry paths (platform not assembled yet, the listen promise
//! rejecting, the synchronous throw) become
//! [`LanguagePacksChangedListener`]: [`Self::ensure`] answers with the
//! outcome and holds the once-flag, and the host wires the event's
//! arrival to the fetch seam (the effect request the store emits).
//!
//! ## Deliberate boundaries
//!
//! * **`LanguagePackSummary` is carried here** (the wire shape,
//!   camelCase `Deserialize`): the commands module
//!   (`commands/languagePacks.ts`) is T331's (segment 4, pending), so
//!   the store carries its read of it — the same carrying shape the
//!   `sql_syntax_preview` presets hold for T632. T331's port replaces
//!   the type, not the semantics.
//! * **`toErrorMessage`**: the host's adapter already returns
//!   error-shaped `String`s (the `toErrorMessage` port's rule), so the
//!   store takes the message as-is.
//! * `LANGUAGE_PACKS_CHANGED_EVENT`'s spelling (`language-packs:changed`)
//!   is [`LANGUAGE_PACKS_CHANGED_EVENT`] verbatim, for the wiring task
//!   that owns the listen call.

// The Settings card / install dialog wiring tasks are the consumers.
#![allow(dead_code)]

use serde::Deserialize;

/// React `LANGUAGE_PACKS_CHANGED_EVENT`
/// (`commands/languagePacks.ts:4`): the cross-window refresh event.
pub(crate) const LANGUAGE_PACKS_CHANGED_EVENT: &str = "language-packs:changed";

/// React `LanguagePackSummary` (`commands/languagePacks.ts:19-27`): one
/// installed pack as listed by `list_language_packs`. Carried here
/// until T331 (its owning commands task) lands — see the module docs.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LanguagePackSummary {
    pub code: String,
    pub label: String,
    pub version: String,
    pub author: Option<String>,
    pub description: Option<String>,
    /// Total translated messages across all domain files.
    pub message_count: u64,
    /// Install directory name (always equals `code`).
    pub file_name: String,
}

/// What the host must do next — the two IPC calls the zustand actions
/// make, as request payloads (the `AiStore`'s `AiConfigEffect` shape).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LanguagePackEffect {
    /// `languagePackCommands.listLanguagePacks()`.
    ListPacks,
    /// `languagePackCommands.removeLanguagePack(code)`.
    RemovePack { code: String },
}

/// The store's slice: the packs, the loaded flag and the error text
/// (React's zustand state minus the two actions).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct LanguagePackStore {
    pub packs: Vec<LanguagePackSummary>,
    pub loaded: bool,
    pub error: Option<String>,
}

impl LanguagePackStore {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// React `byCode`: the pack with the code, or none.
    pub(crate) fn by_code(&self, code: &str) -> Option<&LanguagePackSummary> {
        self.packs.iter().find(|pack| pack.code == code)
    }

    /// `fetch`'s opening: the request the host executes. The state
    /// holds everything until the answer lands (React sets nothing
    /// before the `await`).
    pub(crate) fn begin_fetch(&mut self) -> LanguagePackEffect {
        LanguagePackEffect::ListPacks
    }

    /// `fetch`'s success: `set({ packs, loaded: true, error: null })`.
    pub(crate) fn complete_fetch(&mut self, packs: Vec<LanguagePackSummary>) {
        self.packs = packs;
        self.loaded = true;
        self.error = None;
    }

    /// `fetch`'s catch: `set({ loaded: true, error })` — a failed load
    /// still counts as loaded (the card shows the error, never a
    /// spinner again).
    pub(crate) fn fail_fetch(&mut self, message: &str) {
        self.loaded = true;
        self.error = Some(message.to_owned());
    }

    /// `remove`'s opening: the removal request. Nothing in the state
    /// moves yet (React sets nothing on the success path either — the
    /// refetch that follows writes it).
    pub(crate) fn begin_remove(&mut self, code: &str) -> LanguagePackEffect {
        LanguagePackEffect::RemovePack { code: code.to_owned() }
    }

    /// `remove`'s catch: `set({ error })` — only the error; `loaded`
    /// keeps whatever the last fetch left. The rethrow is the host's
    /// business (the `Err` it already holds).
    pub(crate) fn fail_remove(&mut self, message: &str) {
        self.error = Some(message.to_owned());
    }
}

/// The `ensureLanguagePacksChangedListener` once-latch: the module-level
/// `subscriptionStarted` flag and its three retry paths.
#[derive(Clone, Debug, Default)]
pub(crate) struct LanguagePacksChangedListener {
    started: bool,
}

/// What [`LanguagePacksChangedListener::ensure`] found — each outcome
/// maps one React arm of the try/catch (53–74).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ListenerOutcome {
    /// `getPlatformOrNull()` returned null: not assembled yet (unit
    /// tests / pre-bootstrap). The flag was reset — a later call
    /// retries.
    PlatformMissing,
    /// The listen promise rejected (`p.catch`): the flag was reset, a
    /// later call retries.
    SubscribeFailed,
    /// The synchronous `catch` arm: the flag was reset, a later call
    /// retries.
    ListenThrew,
    /// The listener is live (or was already — the once-per-instance
    /// rule: a second call is a no-op that answers `Listening`).
    Listening,
}

impl LanguagePacksChangedListener {
    pub(crate) fn new() -> Self {
        Self { started: false }
    }

    /// `ensureLanguagePacksChangedListener`: registers the refresh
    /// exactly once per instance. The host reports the platform's
    /// shape (assembled? the listen promise rejected? the call threw?)
    /// and `ensure` answers what happened and holds the latch.
    ///
    /// * The once rule (React 54–55): once `Listening`, every later
    ///   call answers `Listening` without re-reporting — the second
    ///   subscription React would never open.
    /// * The three retry paths (React 59–61, 67–69, 71–73) all reset
    ///   the latch, so the next `ensure` tries again exactly as the
    ///   module flag would.
    pub(crate) fn ensure(
        &mut self,
        platform_assembled: bool,
        subscribe_rejected: bool,
        listen_threw: bool,
    ) -> ListenerOutcome {
        if self.started {
            return ListenerOutcome::Listening;
        }
        // React 57–61: no platform yet — reset and return.
        if !platform_assembled {
            self.started = false;
            return ListenerOutcome::PlatformMissing;
        }
        // React 63: the listen call itself. A synchronous throw
        // (React 71–73) resets the flag.
        if listen_threw {
            self.started = false;
            return ListenerOutcome::ListenThrew;
        }
        // React 65–70: the listen promise. A rejection resets the
        // flag; a settled subscription latches it.
        if subscribe_rejected {
            self.started = false;
            return ListenerOutcome::SubscribeFailed;
        }
        self.started = true;
        ListenerOutcome::Listening
    }

    /// Whether the latch holds a live subscription (the module's
    /// `subscriptionStarted` read).
    pub(crate) fn is_started(&self) -> bool {
        self.started
    }
}
