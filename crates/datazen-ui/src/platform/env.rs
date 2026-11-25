//! `platform/env.ts` (T160): the compile-time form determination.
//!
//! React reads `TAURI_ENV_PLATFORM` from `import.meta.env` — a constant
//! the Tauri CLI injects at build time, so the browser branch
//! dead-code-eliminates in a desktop bundle and vice versa. Runtime
//! `__TAURI_INTERNALS__` probing is banned (form-split §4.2).
//!
//! In the Rune native port the whole `datazen-ui` crate compiles as the
//! desktop binary: there is no browser bundle, so the constant folds to
//! `true`. A Rune web target would carry its own `cfg`-gated build with
//! the constant `false` — the same compile-time fold, never a runtime
//! probe.
//!
//! Consumers (React): the export dialogs (`windows/connection/ExportDialog`,
//! `BatchExportDialog`, `DataTable/DataExportDialog`), the bootstrap
//! (`bootstrap/startSharedApp`), `lib/dataFetcher`, `lib/windowManager`
//! — each reads it to pick the desktop-only path (file dialogs, the
//! bridge fetch, the real window manager) at compile time.

/// `IS_DESKTOP_BUILD`: this build is the desktop form.
pub const IS_DESKTOP_BUILD: bool = true;
