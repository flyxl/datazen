//! AI cancel wrapper + 8s terminal fallback, ported from
//! `packages/frontend/src/ai/sessionCancel.ts`.
//!
//! ```text
//! cancel() ──CallCancel──▶ on_cancel_result
//!                              │ marked + mark_cancelling()==true
//!                              ▼
//!                         arm fallback (deadline = now + 8s)
//!                              │ tick(now) reaches deadline
//!                              ▼
//!                    get() terminal? ──yes──▶ no-op
//!                              │ no
//!                              ▼
//!                    settle(id, cancelled) ──▶ CancelTimeout effect
//! ```
//!
//! # Faithfulness
//!
//! Every branch of the React class has a counterpart: the three
//! [`CancelReceipt`] kinds, the transport-failure (`null`) path with its exact
//! warn message, the `mark_cancelling()` gate on arming, the per-request timer
//! map with "clear before re-arm" idempotency, the `settled` subscription that
//! clears a timer on *any* terminal transition, and the fallback callback that
//! deletes its own handle before testing `get()` for a terminal phase.
//!
//! # Immediate-mode adaptation (honest)
//!
//! React injects async `cancel` / `setTimeout` / `clearTimeout` closures. Rune
//! has no promises and no JS timers, so:
//!
//! * **No fake async.** `cancel()` emits a [`CancelEffect::CallCancel`]; the
//!   host runs the transport and delivers the outcome through
//!   [`SessionCanceller::on_cancel_result`]. Nothing fabricates a `Future`.
//! * **Deadlines, not booleans.** Arming stores an absolute `deadline_ms` and
//!   emits [`CancelEffect::ArmFallback`]. The host turns that into
//!   `Tx::wake_at(deadline_ms / 1000)`; when the runtime wakes, the host calls
//!   [`SessionCanceller::tick`] with the clock. A wake that fires after the
//!   timer was cleared is a no-op because `tick` finds no entry — the runtime
//!   equivalent of `clearTimeout` suppressing the callback. No thread is
//!   spawned.
//! * **Subscription via a shared timer map.** The React constructor calls
//!   `registry.subscribe(listener)` where the listener calls `clearTimer`.
//!   Here the listener owns an `Rc` clone of the timer map, so a `settled`
//!   event clears the right request without a self-borrow. `dispose()`
//!   unsubscribes through [`StreamRegistry::unsubscribe`].
//! * **Callbacks are effects.** `onWarn` / `onCancelTimeout` become
//!   [`CancelEffect::Warn`] / [`CancelEffect::CancelTimeout`] for the host to
//!   execute, exactly as in `grant_state.rs`.
//!
//! # Removal stage
//!
//! `StreamRegistry` is a deliberately minimal local port trait. T133 lands the
//! full `streamRegistry.ts` port; delete this trait and store that concrete
//! type instead. Every public item is `#[allow(dead_code)]` until the AI chat
//! panel (the consumer) embeds this machine in its state slice.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// `CANCEL_TIMEOUT_MS` — exactly 8,000ms, the wait for the terminal chunk.
#[allow(dead_code)] // T097; consumer: AI chat panel (not landed). Delete then.
pub(crate) const CANCEL_TIMEOUT_MS: i64 = 8_000;

/// Opaque, machine-owned timer handle. Stands in for the JS `TimerHandle`
/// (`ReturnType<typeof setTimeout>`) so the machine can "store the handle,
/// clear it later" without depending on a platform timer type.
#[allow(dead_code)] // T097; consumer: AI chat panel (not landed). Delete then.
pub(crate) type CancelTimerHandle = u64;

/// `CancelReceipt` from `ai/targetClaim.ts` — the three-state answer to
/// `ai_cancel`.
#[allow(dead_code)] // T097; consumer: AI chat panel (not landed). Delete then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CancelReceipt {
    Marked { request_id: String },
    AlreadyFinished { request_id: String },
    Forbidden { request_id: String },
}

impl CancelReceipt {
    #[allow(dead_code)] // T097; consumer: AI chat panel (not landed). Delete then.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            CancelReceipt::Marked { .. } => "marked",
            CancelReceipt::AlreadyFinished { .. } => "alreadyFinished",
            CancelReceipt::Forbidden { .. } => "forbidden",
        }
    }

    #[allow(dead_code)] // T097; consumer: AI chat panel (not landed). Delete then.
    pub(crate) fn request_id(&self) -> &str {
        match self {
            CancelReceipt::Marked { request_id }
            | CancelReceipt::AlreadyFinished { request_id }
            | CancelReceipt::Forbidden { request_id } => request_id,
        }
    }
}

/// `StreamPhase` / `StreamTerminalKind` from `ai/streamRegistry.ts`. Only the
/// fields the fallback reads are mirrored.
#[allow(dead_code)] // T097; consumer: AI chat panel (not landed). Delete then.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StreamPhase {
    Starting,
    Streaming,
    Cancelling,
    Terminal,
}

#[allow(dead_code)] // T097; consumer: AI chat panel (not landed). Delete then.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StreamTerminalKind {
    Done,
    Cancelled,
    Error,
}

/// The read-only view of a registry entry the fallback needs (`registry.get`).
#[allow(dead_code)] // T097; consumer: AI chat panel (not landed). Delete then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StreamEntrySnapshot {
    pub request_id: String,
    pub phase: StreamPhase,
    pub terminal_kind: Option<StreamTerminalKind>,
}

/// `StreamRegistryEvent` from `ai/streamRegistry.ts`. The canceller subscribes
/// for the `Settled` arm only; the other variants are carried for parity with
/// T133's registry and are ignored by [`SessionCanceller`].
#[allow(dead_code)] // T097; consumer: AI chat panel (not landed). Delete then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StreamEvent {
    Started { request_id: String },
    Streaming { request_id: String },
    Cancelling { request_id: String },
    Settled {
        request_id: String,
        terminal_kind: StreamTerminalKind,
    },
}

/// Listener passed to [`StreamRegistry::subscribe`]. `u64` ids replace the TS
/// unsubscribe closure (a closure borrowing the machine cannot live beside it
/// in `S`), so dispose pairs with [`StreamRegistry::unsubscribe`].
#[allow(dead_code)] // T097; consumer: AI chat panel (not landed). Delete then.
pub(crate) type RegistryListener = Box<dyn FnMut(StreamEvent)>;

/// Minimal injected `StreamRegistry` port. Removal stage: T133 lands the real
/// `streamRegistry.ts` port; delete this trait and use that type directly.
#[allow(dead_code)] // T097; consumer: AI chat panel (not landed). Delete then.
pub(crate) trait StreamRegistry {
    /// React `markCancelling`: `true` when the entry exists, is non-terminal,
    /// current generation and now `cancelling`.
    fn mark_cancelling(&mut self, request_id: &str) -> bool;
    /// React `get`: a snapshot of the entry, or `None` for unknown requests.
    fn get(&self, request_id: &str) -> Option<StreamEntrySnapshot>;
    /// React `settle`: force a terminal state; `false` for unknown/terminal.
    fn settle(&mut self, request_id: &str, kind: StreamTerminalKind) -> bool;
    /// React `subscribe`: register a listener, returning an id to unsubscribe.
    fn subscribe(&mut self, listener: RegistryListener) -> u64;
    /// Detach the listener previously returned by [`Self::subscribe`].
    fn unsubscribe(&mut self, id: u64);
}

/// Work the host performs on behalf of the machine. Mirrors the React injected
/// async and timer functions without performing I/O or spawning threads.
#[allow(dead_code)] // T097; consumer: AI chat panel (not landed). Delete then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CancelEffect {
    /// Execute the injected `cancel(requestId)`; deliver with
    /// [`SessionCanceller::on_cancel_result`].
    CallCancel { request_id: String },
    /// Arm the 8s fallback. Host: `tx.wake_at(deadline_ms / 1000)`.
    ArmFallback {
        request_id: String,
        handle: CancelTimerHandle,
        deadline_ms: i64,
    },
    /// React `clearTimeout`: cancel a previously armed handle (bookkeeping for
    /// the host; a wake that still fires is ignored by `tick`).
    ClearTimer { handle: CancelTimerHandle },
    /// The injected `onWarn(message, error)` call.
    Warn {
        message: String,
        error: Option<String>,
    },
    /// The fallback forced a terminal state: host finalises the UI.
    CancelTimeout { request_id: String },
}

/// Synchronous result of [`SessionCanceller::on_cancel_result`], mirroring the
/// value the TS `Promise<CancelReceipt | null>` resolves to. `None` receipt =
/// transport failure ("no receipt means no proof").
#[allow(dead_code)] // T097; consumer: AI chat panel (not landed). Delete then.
#[derive(Debug)]
pub(crate) struct CancelStep {
    pub receipt: Option<CancelReceipt>,
    pub effects: Vec<CancelEffect>,
}

struct TimerState {
    handle: CancelTimerHandle,
    deadline_ms: i64,
}

/// `SessionCanceller` — the ported state machine. Embed in `S`; drive with
/// `cancel` / `on_cancel_result` / `tick` and execute the returned effects.
#[allow(dead_code)] // T097; consumer: AI chat panel (not landed). Delete then.
pub(crate) struct SessionCanceller {
    registry: Box<dyn StreamRegistry>,
    timeout_ms: i64,
    timers: Rc<RefCell<HashMap<String, TimerState>>>,
    next_timer_handle: CancelTimerHandle,
    subscription: Option<u64>,
    disposed: bool,
}

#[allow(dead_code)] // T097; consumer: AI chat panel (not landed). Delete then.
impl SessionCanceller {
    /// Mirrors the React constructor: subscribe to the registry so any
    /// `settled` event clears that request's fallback timer.
    pub(crate) fn new(mut registry: Box<dyn StreamRegistry>, timeout_ms: i64) -> Self {
        let timers = Rc::new(RefCell::new(HashMap::new()));
        let listener_timers = Rc::clone(&timers);
        let subscription = registry.subscribe(Box::new(move |event| {
            if let StreamEvent::Settled { request_id, .. } = event {
                listener_timers.borrow_mut().remove(&request_id);
            }
        }));
        Self {
            registry,
            timeout_ms,
            timers,
            next_timer_handle: 1,
            subscription: Some(subscription),
            disposed: false,
        }
    }

    /// `cancel(requestId)` — emit the injected transport call. The receipt is
    /// delivered later through [`Self::on_cancel_result`].
    pub(crate) fn cancel(&mut self, request_id: &str) -> Vec<CancelEffect> {
        if self.disposed {
            return Vec::new();
        }
        vec![CancelEffect::CallCancel {
            request_id: request_id.to_owned(),
        }]
    }

    /// Resolution of the injected `cancel` call (the TS `.then`/`.catch`).
    /// `Err(error)` is a transport failure: warn and return `None` leaving the
    /// stream untouched. Each receipt kind is translated exactly as React.
    pub(crate) fn on_cancel_result(
        &mut self,
        request_id: &str,
        result: Result<CancelReceipt, String>,
        now_ms: i64,
    ) -> CancelStep {
        if self.disposed {
            return CancelStep {
                receipt: None,
                effects: Vec::new(),
            };
        }
        let receipt = match result {
            Err(error) => {
                return CancelStep {
                    receipt: None,
                    effects: vec![CancelEffect::Warn {
                        message: format!("ai_cancel failed for request {request_id}"),
                        error: Some(error),
                    }],
                };
            }
            Ok(receipt) => receipt,
        };

        let mut effects = Vec::new();
        match &receipt {
            CancelReceipt::Forbidden { .. } => {
                effects.push(CancelEffect::Warn {
                    message: format!("ai_cancel forbidden for request {request_id}"),
                    error: None,
                });
            }
            CancelReceipt::AlreadyFinished { .. } => {}
            CancelReceipt::Marked { .. } => {
                if self.registry.mark_cancelling(request_id) {
                    self.arm_fallback(request_id, now_ms, &mut effects);
                }
            }
        }
        CancelStep {
            receipt: Some(receipt),
            effects,
        }
    }

    /// Drive the fallback with the current clock (ms). A pending timer whose
    /// deadline has been reached is deleted first; an unknown or already
    /// terminal entry is a no-op; otherwise force `settle(cancelled)` and emit
    /// [`CancelEffect::CancelTimeout`] only when the settle actually forced
    /// the terminal state.
    pub(crate) fn tick(&mut self, now_ms: i64) -> Vec<CancelEffect> {
        if self.disposed {
            return Vec::new();
        }
        let due: Vec<(String, CancelTimerHandle)> = self
            .timers
            .borrow()
            .iter()
            .filter(|(_, timer)| timer.deadline_ms <= now_ms)
            .map(|(id, timer)| (id.clone(), timer.handle))
            .collect();

        let mut effects = Vec::new();
        for (request_id, handle) in due {
            // React: `this.timers.delete(requestId)` before any registry read.
            self.timers.borrow_mut().remove(&request_id);
            effects.push(CancelEffect::ClearTimer { handle });

            let entry = self.registry.get(&request_id);
            let already_terminal = entry
                .as_ref()
                .is_some_and(|e| e.phase == StreamPhase::Terminal);
            if entry.is_none() || already_terminal {
                continue;
            }
            if self
                .registry
                .settle(&request_id, StreamTerminalKind::Cancelled)
            {
                effects.push(CancelEffect::CancelTimeout { request_id });
            }
        }
        effects
    }

    /// Clear all fallback timers and detach from the registry.
    pub(crate) fn dispose(&mut self) -> Vec<CancelEffect> {
        if self.disposed {
            return Vec::new();
        }
        self.disposed = true;
        let mut effects = Vec::new();
        for (_, timer) in self.timers.borrow_mut().drain() {
            effects.push(CancelEffect::ClearTimer { handle: timer.handle });
        }
        if let Some(id) = self.subscription.take() {
            self.registry.unsubscribe(id);
        }
        effects
    }

    /// Test/observability hook: is a fallback pending for `request_id`?
    pub(crate) fn has_timer(&self, request_id: &str) -> bool {
        self.timers.borrow().contains_key(request_id)
    }

    /// Idempotent re-arm: clear any existing handle first, then store a fresh
    /// deadline and emit the wake the host turns into `tx.wake_at`.
    fn arm_fallback(&mut self, request_id: &str, now_ms: i64, effects: &mut Vec<CancelEffect>) {
        if let Some(previous) = self.timers.borrow_mut().remove(request_id) {
            effects.push(CancelEffect::ClearTimer {
                handle: previous.handle,
            });
        }
        let handle = self.next_timer_handle;
        self.next_timer_handle += 1;
        let deadline_ms = now_ms + self.timeout_ms;
        self.timers.borrow_mut().insert(
            request_id.to_owned(),
            TimerState {
                handle,
                deadline_ms,
            },
        );
        effects.push(CancelEffect::ArmFallback {
            request_id: request_id.to_owned(),
            handle,
            deadline_ms,
        });
    }
}
