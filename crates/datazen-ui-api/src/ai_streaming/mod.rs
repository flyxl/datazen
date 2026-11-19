//! Streaming, cancellation and attribution wiring for the AI store (ported
//! from React `packages/frontend/src/stores/ai/streaming.ts`, tasklist T088
//! — the row is marked `split`, and the split marker's scope applies here:
//! this module owns **streaming.ts's** semantics only; the React siblings
//! `ai/streamRegistry.ts`, `ai/sessionCancel.ts`, `ai/targetClaim.ts`,
//! `lib/extractQuestions.ts`, `lib/extractSql.ts`, `commands/ai.ts` and
//! `stores/ai/types.ts` are separate tasks and are injected as seams below).
//!
//! ## What the file is (and therefore what this module is)
//!
//! streaming.ts is the zustand adapter over the pure lifecycle rules that
//! live in `src/ai/streamRegistry` and `src/ai/sessionCancel`: it routes
//! each SSE chunk / out-of-band error to the live session that owns its
//! requestId (the chat session, the workflow chat session, or the NL2SQL
//! slice — which is deliberately **not** part of the chat lifecycle
//! registry), gates late/duplicate chunks through the registry, builds the
//! terminal assistant messages, and wires the cancel orchestrator. The
//! port keeps exactly that adapter role: an
//! [`AiStreaming`](adapter::AiStreaming) struct whose constructor injects
//! every ambient dependency as a seam, while the zustand `get`/`set` pair
//! becomes the [`AiStoreView`](seams::AiStoreView) trait the host
//! implements over its store.
//!
//! ## The seams (each names its owning migration task)
//!
//! | React import | Port seam | Owner |
//! | --- | --- | --- |
//! | `ai/streamRegistry` (module singleton) | [`StreamRegistryOps`](seams::StreamRegistryOps) field — same instance across every call | cross-segment `ai/` port task (not started) |
//! | `ai/sessionCancel` (`SessionCanceller`, lazily built by `cancellerFor`) | [`SessionCancellerOps`](seams::SessionCancellerOps) field | cross-segment `ai/` port task (not started); the wiring is documented on the trait |
//! | `commands/ai` (`aiCommands.cancel`) | [`AiCancelCommand`](seams::AiCancelCommand) — `Result` per the T040 await→`Result` precedent | commands/ai port task |
//! | `ai/targetClaim` (`AiTargetClaim`, `attributionOf`) | lean [`AiTargetClaim`](seams::AiTargetClaim) + the injected `attribution_of` fn | cross-segment `ai/` port task (not started) |
//! | `lib/extractQuestions`, `lib/extractSql` (four fns) | [`ExtractionSeams`](seams::ExtractionSeams) fn fields | T503 / T505 (lib segment, not started) |
//! | `stores/ai/types` (`AiStore`, `Nl2SqlState`) | [`AiStoreView`](seams::AiStoreView) + lean [`Nl2sqlStreamingSlice`](seams::Nl2sqlStreamingSlice) | T126 (this segment) / T128 wiring |
//! | `./sessions` (`persistChatSession`) | [`ChatSessionPersister`](seams::ChatSessionPersister) — already ported (T040): the blanket impl reuses `AiSessionStore` | done (T040) |
//! | `crypto.randomUUID()` | [`NewId`](crate::ai_sessions::NewId) (ai_sessions' seam) | host wiring |
//! | `new Date().toISOString()` | [`StreamingClock`](seams::StreamingClock) | host wiring |
//!
//! ## Module state → struct fields
//!
//! React's three module-level mutable cells — the `streamRegistry`
//! singleton, the lazily-initialised `sessionCanceller`, and the
//! `requestIdToKey` routing map — become `AiStreaming`'s fields, so one
//! instance carries the same state across every call exactly the way the
//! React module does. `getStreamRegistry()` ports as
//! [`AiStreaming::registry`](adapter::AiStreaming::registry) so the send
//! path (the aiStore port, T128) can reach the same instance the
//! chunk/error handlers gate through — the concrete registry handle stays
//! with the host that injected the `Box`, preserving React's
//! shared-singleton guarantee.
//!
//! ## Async half
//!
//! streaming.ts has three async seams. `cancelChat` awaits the canceller
//! and resolves with the receipt or `null` on transport failure — the
//! port returns `Option<CancelReceipt>`. `persistChatSession` returns a
//! promise whose `.then` reports the failure — T040 already ported it to
//! the `Result`-shaped `SessionPersistResult`, so the port calls it
//! synchronously and reports the error inline (the sync folding React's
//! `void …then(...)` defers). `cancellerFor`'s `onCancelTimeout` wiring —
//! the 8s fallback's callback — is the future adapter's to construct, and
//! [`AiStreaming::handle_cancel_timeout`](adapter::AiStreaming::handle_cancel_timeout)
//! is the entry it calls back into.
//!
//! ## Layout
//!
//! - [`seams`] — the seam vocabulary (lean mirrors + traits) and the one
//!   pure helper (`find_mcp_tool_name`), with the owning-task annotations.
//! - [`adapter`] — the `AiStreaming` struct and the operations React
//!   exports (routing, gating, terminal messages, the cancel wiring).
//! - [`test_support`] — the React suites' doubles (test-only).
//! - [`tests`] — the stream lifecycle pins (chunk/error/timeout/persist
//!   arms; test-only).
//! - [`wiring_tests`] — the pure wiring pins (routing map, registry
//!   accessor, attribution forward, cancelChat; test-only).

mod adapter;
mod seams;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod wiring_tests;

pub use adapter::AiStreaming;
pub use seams::{
    AiCancelCommand, AiStoreView, AiTargetClaim, ApplyChunkGate, ApplyGateResult, CancelReceipt,
    ChatSessionPersister, ExtractionSeams, ExtractQuestionsOutcome, Nl2sqlStreamingSlice,
    SessionCancellerOps, StreamRegistryOps, StreamingClock, find_mcp_tool_name,
};
