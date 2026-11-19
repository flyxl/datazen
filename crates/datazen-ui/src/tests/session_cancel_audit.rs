//! T097 independent audit — coverage the coder's branch suite does not pin,
//! with literal (non-constant-derived) numbers so a timeout-constant mutation
//! cannot keep the assertion green by editing both sides.
//!
//! Focus:
//!   * the injected mark gate (`mark_cancelling() == false` must never arm),
//!   * the 8000ms constant asserted literally at the effect boundary,
//!   * the `settled`-only listener (Started/Streaming/Cancelling are ignored),
//!   * per-request deadline isolation with two concurrent requests,
//!   * disposed early-returns and idempotent dispose,
//!   * the `entry vanished` fallback branch + its ClearTimer effect,
//!   * a real `Runtime::wake_deadline()` pin at literal 8.0s.
//!
//! Every case drives the machine only through its public API plus a fake
//! registry; no product source is mutated here.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rune::{Cx, El, Env, Runtime, Tx};

use crate::ai::session_cancel::*;

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

#[derive(Default)]
struct AuditRegistry {
    entries: HashMap<String, StreamEntrySnapshot>,
    mark_ok: bool,
    mark_calls: usize,
    listeners: Vec<(u64, RegistryListener)>,
    next_sub: u64,
    settle_calls: Vec<(String, StreamTerminalKind)>,
}

impl AuditRegistry {
    fn with(ids: &[&str], mark_ok: bool) -> Self {
        let mut entries = HashMap::new();
        for id in ids {
            entries.insert(
                (*id).to_owned(),
                StreamEntrySnapshot {
                    request_id: (*id).to_owned(),
                    phase: StreamPhase::Streaming,
                    terminal_kind: None,
                },
            );
        }
        Self {
            entries,
            mark_ok,
            next_sub: 1,
            ..Default::default()
        }
    }

    /// Deliver an arbitrary registry event to the machine's listener.
    fn emit(&mut self, event: StreamEvent) {
        for (_, listener) in &mut self.listeners {
            listener(event.clone());
        }
    }
}

impl StreamRegistry for Rc<RefCell<AuditRegistry>> {
    fn mark_cancelling(&mut self, request_id: &str) -> bool {
        let mut r = self.borrow_mut();
        r.mark_calls += 1;
        if !r.mark_ok {
            return false;
        }
        match r.entries.get_mut(request_id) {
            Some(entry) if entry.phase != StreamPhase::Terminal => {
                entry.phase = StreamPhase::Cancelling;
                true
            }
            _ => false,
        }
    }

    fn get(&self, request_id: &str) -> Option<StreamEntrySnapshot> {
        self.borrow().entries.get(request_id).cloned()
    }

    fn settle(&mut self, request_id: &str, kind: StreamTerminalKind) -> bool {
        let mut r = self.borrow_mut();
        let Some(entry) = r.entries.get_mut(request_id) else {
            return false;
        };
        if entry.phase == StreamPhase::Terminal {
            return false;
        }
        entry.phase = StreamPhase::Terminal;
        entry.terminal_kind = Some(kind);
        r.settle_calls.push((request_id.to_owned(), kind));
        let event = StreamEvent::Settled {
            request_id: request_id.to_owned(),
            terminal_kind: kind,
        };
        for (_, listener) in &mut r.listeners {
            listener(event.clone());
        }
        true
    }

    fn subscribe(&mut self, listener: RegistryListener) -> u64 {
        let mut r = self.borrow_mut();
        let id = r.next_sub;
        r.next_sub += 1;
        r.listeners.push((id, listener));
        id
    }

    fn unsubscribe(&mut self, id: u64) {
        self.borrow_mut().listeners.retain(|(c, _)| *c != id);
    }
}

fn machine(
    ids: &[&str],
    mark_ok: bool,
) -> (SessionCanceller, Rc<RefCell<AuditRegistry>>) {
    let shared = Rc::new(RefCell::new(AuditRegistry::with(ids, mark_ok)));
    let canceller = SessionCanceller::new(Box::new(shared.clone()), CANCEL_TIMEOUT_MS);
    (canceller, shared)
}

fn marked(id: &str) -> CancelReceipt {
    CancelReceipt::Marked {
        request_id: id.to_owned(),
    }
}

fn deadline_of(effects: &[CancelEffect]) -> Option<i64> {
    effects.iter().find_map(|e| match e {
        CancelEffect::ArmFallback { deadline_ms, .. } => Some(*deadline_ms),
        _ => None,
    })
}

fn timeouts(effects: &[CancelEffect]) -> Vec<&str> {
    effects
        .iter()
        .filter_map(|e| match e {
            CancelEffect::CancelTimeout { request_id } => Some(request_id.as_str()),
            _ => None,
        })
        .collect()
}

fn clears(effects: &[CancelEffect]) -> Vec<CancelTimerHandle> {
    effects
        .iter()
        .filter_map(|e| match e {
            CancelEffect::ClearTimer { handle } => Some(*handle),
            _ => None,
        })
        .collect()
}

// --- 1. The mark gate is the ONLY thing that arms ----------------------------

#[test]
fn audit_mark_refused_never_arms_and_never_touches_state() {
    let (mut c, reg) = machine(&["r"], false);
    let step = c.on_cancel_result("r", Ok(marked("r")), 5_000);

    assert_eq!(reg.borrow().mark_calls, 1, "markCancelling must be consulted once");
    assert_eq!(step.receipt, Some(marked("r")));
    assert!(
        deadline_of(&step.effects).is_none(),
        "a refused mark must not arm the 8s fallback"
    );
    assert!(!c.has_timer("r"));
    assert_eq!(
        reg.borrow().entries["r"].phase,
        StreamPhase::Streaming,
        "state must be untouched when the mark is refused"
    );
}

// --- 2. Literal 8000 (not derived from the constant under test) -------------

#[test]
fn audit_timeout_is_literally_8000ms_at_the_effect_boundary() {
    assert_eq!(CANCEL_TIMEOUT_MS, 8_000, "CANCEL_TIMEOUT_MS must equal 8000");

    let (mut c, _reg) = machine(&["r"], true);
    let step = c.on_cancel_result("r", Ok(marked("r")), 5_000);
    assert_eq!(
        deadline_of(&step.effects),
        Some(13_000), // 5_000 + literal 8_000
        "deadline must be now + 8000ms even if the constant were edited"
    );
}

// --- 3. Only the `settled` event clears a timer ------------------------------

#[test]
fn audit_only_settled_events_clear_the_fallback_timer() {
    let (mut c, reg) = machine(&["r"], true);
    c.on_cancel_result("r", Ok(marked("r")), 0);
    assert!(c.has_timer("r"), "fallback armed");

    for evt in [
        StreamEvent::Started { request_id: "r".into() },
        StreamEvent::Streaming { request_id: "r".into() },
        StreamEvent::Cancelling { request_id: "r".into() },
    ] {
        reg.borrow_mut().emit(evt);
        assert!(c.has_timer("r"), "non-settled events must NOT clear the timer");
    }

    reg.borrow_mut().emit(StreamEvent::Settled {
        request_id: "r".into(),
        terminal_kind: StreamTerminalKind::Done,
    });
    assert!(!c.has_timer("r"), "a settled event clears that request's timer");
}

// --- 4. Per-request deadline isolation --------------------------------------

#[test]
fn audit_two_requests_settle_only_when_their_own_deadline_reaches() {
    let (mut c, reg) = machine(&["a", "b"], true);
    c.on_cancel_result("a", Ok(marked("a")), 0);
    c.on_cancel_result("b", Ok(marked("b")), 1_000);

    let at_first = c.tick(8_000);
    assert_eq!(timeouts(&at_first), vec!["a"], "only a is due at 8000");
    assert!(!c.has_timer("a"), "a's fired timer is removed from the map");
    assert!(c.has_timer("b"), "b is still armed at 8000");
    assert_eq!(
        reg.borrow().settle_calls,
        vec![("a".to_owned(), StreamTerminalKind::Cancelled)]
    );

    let before_second = c.tick(8_999);
    assert!(timeouts(&before_second).is_empty(), "b not due at 8999");
    let at_second = c.tick(9_000);
    assert_eq!(timeouts(&at_second), vec!["b"], "b is due at its own 9000");
    assert!(!c.has_timer("b"));
}

// --- 5. Disposed early returns + idempotent dispose -------------------------

#[test]
fn audit_disposed_machine_ignores_results_and_dispose_is_idempotent() {
    let (mut c, reg) = machine(&["r"], true);
    let first = c.dispose();
    assert!(first.is_empty(), "dispose with no timers emits nothing");

    let step = c.on_cancel_result("r", Ok(marked("r")), 0);
    assert_eq!(step.receipt, None, "a disposed machine returns no receipt");
    assert!(step.effects.is_empty(), "a disposed machine emits no effects");
    assert!(!c.has_timer("r"));
    assert_eq!(
        reg.borrow().entries["r"].phase,
        StreamPhase::Streaming,
        "a disposed machine must not mark state"
    );
    assert!(c.cancel("r").is_empty());
    assert!(c.tick(100_000).is_empty());
    assert!(c.dispose().is_empty(), "second dispose is a no-op");
    assert_eq!(reg.borrow().mark_calls, 0, "disposed machine never consults registry");
}

// --- 6. An entry that vanished mid-flight: ClearTimer, no timeout -----------

#[test]
fn audit_vanished_entry_is_cleared_without_a_forced_settle() {
    let (mut c, reg) = machine(&["r"], true);
    let arm = c.on_cancel_result("r", Ok(marked("r")), 0);
    let handle = match &arm.effects[0] {
        CancelEffect::ArmFallback { handle, .. } => *handle,
        other => panic!("expected ArmFallback first, got {other:?}"),
    };

    // The registry entry disappears (e.g. conversation torn down) without a
    // settled event reaching the machine, so the timer is still live.
    reg.borrow_mut().entries.remove("r");

    let effects = c.tick(8_000);
    assert_eq!(clears(&effects), vec![handle], "the fired timer is cleared");
    assert!(timeouts(&effects).is_empty(), "no CancelTimeout for a missing entry");
    assert!(reg.borrow().settle_calls.is_empty(), "no settle on a missing entry");
    assert!(!c.has_timer("r"));
}

// --- 7. Real runtime wake, pinned at a literal 8.0s -------------------------

#[derive(Default)]
struct Probe {
    canceller: Option<SessionCanceller>,
    press_clock: Option<f64>,
}

fn probe() -> Probe {
    let reg = Rc::new(RefCell::new(AuditRegistry::with(&["r"], true)));
    Probe {
        canceller: Some(SessionCanceller::new(Box::new(reg), CANCEL_TIMEOUT_MS)),
        ..Default::default()
    }
}

fn probe_view(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
    rune::row()
        .key("go")
        .role("button")
        .label("go")
        .w(4.0)
        .h(4.0)
        .on_press_tx(|s: &mut Probe, tx: &mut Tx| {
            s.press_clock = Some(tx.clock());
            let now_ms = (tx.clock() * 1_000.0) as i64;
            let c = s.canceller.as_mut().expect("canceller present");
            let _ = c.cancel("r");
            let step = c.on_cancel_result("r", Ok(marked("r")), now_ms);
            for effect in step.effects {
                if let CancelEffect::ArmFallback { deadline_ms, .. } = effect {
                    tx.wake_at(deadline_ms as f64 / 1_000.0);
                }
            }
        })
}

#[test]
fn audit_real_runtime_wake_is_exactly_8s_after_press() {
    let mut rt = Runtime::new(probe(), probe_view, (40.0, 40.0), env());
    rt.frame(0.0);
    assert_eq!(rt.wake_deadline(), None, "no wake before a cancel is armed");

    let (x, y) = rt.find("go").expect("go is laid out");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);

    let press = rt.state.press_clock.expect("press handler ran");
    let deadline = rt
        .wake_deadline()
        .expect("a marked cancel must schedule a real runtime wake");
    assert!(
        (deadline - press - 8.0).abs() <= 0.01,
        "wake must be literally 8.0s after press: press={press} deadline={deadline}"
    );
}
