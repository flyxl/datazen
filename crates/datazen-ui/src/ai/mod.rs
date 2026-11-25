//! AI state machines ported from React `packages/frontend/src/ai/`.
//!
//! These are pure logic modules: no Rune tree, no `AppState` dependency and no
//! IPC. The host embeds the machine (e.g. `GrantState`) in its own state slice
//! and executes the [`grant_state::GrantEffect`]s the machine emits, feeding
//! outcomes back through the `on_*` methods. See `grant_state.rs` for the
//! honest description of how React promises/timers map to immediate mode.

#[allow(dead_code)] // T049; no host caller until the AI chat panel lands. Delete when it does.
pub mod grant_state;
pub mod session_cancel;
#[allow(dead_code)] // T133; consumer: AI chat panel (not landed). Delete when it lands.
pub mod stream_registry;
/// targetClaim.ts's wire contracts (T086's seam; segment 1's T162 owns the
/// file formally — see the module doc). T162's version reuses the receipt and
/// cancel types from the sibling state machines rather than duplicating them.
#[allow(dead_code)] // T164/T162; consumer: AI chat panel claim wiring (not landed). Delete when it lands.
pub mod target_claim;
