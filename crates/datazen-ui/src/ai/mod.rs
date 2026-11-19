//! AI state machines ported from React `packages/frontend/src/ai/`.
//!
//! These are pure logic modules: no Rune tree, no `AppState` dependency and no
//! IPC. The host embeds the machine (e.g. `GrantState`) in its own state slice
//! and executes the [`grant_state::GrantEffect`]s the machine emits, feeding
//! outcomes back through the `on_*` methods. See `grant_state.rs` for the
//! honest description of how React promises/timers map to immediate mode.

#[allow(dead_code)] // T049; no host caller until the AI chat panel lands. Delete when it does.
pub(crate) mod grant_state;
pub(crate) mod session_cancel;
