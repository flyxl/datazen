//! T097: `packages/frontend/src/ai/sessionCancel.ts` port — real branch
//! coverage with a fake clock/registry, plus one `Runtime`-backed test that
//! pins the 8s fallback deadline through the runtime's wake API
//! (`Frame::wake_deadline`, re-exported on `Runtime`).

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

// --- A faithful-enough injected registry -------------------------------------

#[derive(Default)]
struct FakeRegistry {
    entries: HashMap<String, StreamEntrySnapshot>,
    mark_result: bool,
    listeners: Vec<(u64, RegistryListener)>,
    next_sub: u64,
    settle_calls: Vec<(String, StreamTerminalKind)>,
}

impl FakeRegistry {
    fn streaming(request_id: &str) -> Self {
        let mut entries = HashMap::new();
        entries.insert(
            request_id.to_owned(),
            StreamEntrySnapshot {
                request_id: request_id.to_owned(),
                phase: StreamPhase::Streaming,
                terminal_kind: None,
            },
        );
        Self {
            entries,
            mark_result: true,
            next_sub: 1,
            ..Default::default()
        }
    }
}

impl StreamRegistry for Rc<RefCell<FakeRegistry>> {
    fn mark_cancelling(&mut self, request_id: &str) -> bool {
        let mut r = self.borrow_mut();
        if !r.mark_result {
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
        self.borrow_mut().listeners.retain(|(candidate, _)| *candidate != id);
    }
}

fn harness(mark: bool, phase: StreamPhase) -> (SessionCanceller, Rc<RefCell<FakeRegistry>>) {
    let mut registry = FakeRegistry::streaming("req-1");
    registry.mark_result = mark;
    registry.entries.get_mut("req-1").expect("req-1 exists").phase = phase;
    let shared = Rc::new(RefCell::new(registry));
    let canceller = SessionCanceller::new(Box::new(shared.clone()), CANCEL_TIMEOUT_MS);
    (canceller, shared)
}

fn marked(id: &str) -> CancelReceipt {
    CancelReceipt::Marked {
        request_id: id.to_owned(),
    }
}

fn warns(effects: &[CancelEffect]) -> Vec<(&String, &Option<String>)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            CancelEffect::Warn { message, error } => Some((message, error)),
            _ => None,
        })
        .collect()
}

fn armed(effects: &[CancelEffect]) -> Option<(CancelTimerHandle, i64)> {
    effects.iter().find_map(|effect| match effect {
        CancelEffect::ArmFallback {
            handle, deadline_ms, ..
        } => Some((*handle, *deadline_ms)),
        _ => None,
    })
}

fn timeouts(effects: &[CancelEffect]) -> Vec<&String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            CancelEffect::CancelTimeout { request_id } => Some(request_id),
            _ => None,
        })
        .collect()
}

// --- Receipt kinds + transport failure ---------------------------------------

#[test]
fn a_marked_receipt_marks_cancelling_and_arms_the_fallback() {
    let (mut canceller, registry) = harness(true, StreamPhase::Streaming);
    let step = canceller.on_cancel_result("req-1", Ok(marked("req-1")), 1_000);

    assert_eq!(step.receipt, Some(marked("req-1")), "marked receipt is returned");
    assert_eq!(
        registry.borrow().entries["req-1"].phase,
        StreamPhase::Cancelling,
        "mark_cancelling moved the entry to cancelling"
    );
    assert_eq!(
        armed(&step.effects),
        Some((1, 1_000 + CANCEL_TIMEOUT_MS)),
        "the fallback deadline is exactly now + 8000ms"
    );
    assert!(canceller.has_timer("req-1"), "a fallback timer is pending");
}

#[test]
fn a_forbidden_receipt_warns_once_without_touching_state() {
    let (mut canceller, registry) = harness(true, StreamPhase::Streaming);
    let step = canceller.on_cancel_result(
        "req-1",
        Ok(CancelReceipt::Forbidden {
            request_id: "req-1".into(),
        }),
        1_000,
    );

    assert_eq!(
        step.receipt,
        Some(CancelReceipt::Forbidden {
            request_id: "req-1".into()
        })
    );
    let w = warns(&step.effects);
    assert_eq!(w.len(), 1, "forbidden must warn exactly once, got {w:?}");
    assert_eq!(
        w[0].0, "ai_cancel forbidden for request req-1",
        "exact forbidden message"
    );
    assert_eq!(w[0].1, &None, "forbidden has no error payload");
    assert!(armed(&step.effects).is_none(), "forbidden arms no fallback");
    assert!(!canceller.has_timer("req-1"));
    assert_eq!(
        registry.borrow().entries["req-1"].phase,
        StreamPhase::Streaming,
        "state untouched on forbidden"
    );
}

#[test]
fn an_already_finished_receipt_changes_nothing_and_does_not_warn() {
    let (mut canceller, registry) = harness(true, StreamPhase::Streaming);
    let step = canceller.on_cancel_result(
        "req-1",
        Ok(CancelReceipt::AlreadyFinished {
            request_id: "req-1".into(),
        }),
        1_000,
    );

    assert_eq!(
        step.receipt,
        Some(CancelReceipt::AlreadyFinished {
            request_id: "req-1".into()
        })
    );
    assert!(step.effects.is_empty(), "alreadyFinished emits nothing");
    assert!(!canceller.has_timer("req-1"));
    assert_eq!(
        registry.borrow().entries["req-1"].phase,
        StreamPhase::Streaming,
        "state untouched on alreadyFinished"
    );
}

#[test]
fn a_transport_failure_warns_with_the_exact_message_and_returns_no_receipt() {
    let (mut canceller, registry) = harness(true, StreamPhase::Streaming);
    let step = canceller.on_cancel_result("req-1", Err("ipc down".to_owned()), 1_000);

    assert_eq!(
        step.receipt, None,
        "no receipt means no proof the host observed the cancel"
    );
    let w = warns(&step.effects);
    assert_eq!(w.len(), 1, "transport failure warns exactly once, got {w:?}");
    assert_eq!(
        w[0].0, "ai_cancel failed for request req-1",
        "exact transport-failure message"
    );
    assert_eq!(w[0].1, &Some("ipc down".to_owned()), "original error is carried");
    assert!(armed(&step.effects).is_none(), "failure arms no fallback");
    assert!(!canceller.has_timer("req-1"));
    assert_eq!(
        registry.borrow().entries["req-1"].phase,
        StreamPhase::Streaming,
        "state untouched on transport failure"
    );
}

// --- The mark gate -----------------------------------------------------------

#[test]
fn marked_with_mark_cancelling_false_does_not_arm_a_timer() {
    let (mut canceller, registry) = harness(false, StreamPhase::Streaming);
    let step = canceller.on_cancel_result("req-1", Ok(marked("req-1")), 1_000);

    assert_eq!(step.receipt, Some(marked("req-1")));
    assert!(
        armed(&step.effects).is_none(),
        "mark_cancelling()==false must not arm the fallback"
    );
    assert!(!canceller.has_timer("req-1"));
    assert_eq!(
        registry.borrow().entries["req-1"].phase,
        StreamPhase::Streaming,
        "state untouched when the mark is refused"
    );
}

// --- Fallback timing ---------------------------------------------------------

#[test]
fn the_fallback_forces_settle_cancelled_at_exactly_8s_and_notifies_once() {
    let (mut canceller, registry) = harness(true, StreamPhase::Streaming);
    canceller.on_cancel_result("req-1", Ok(marked("req-1")), 1_000);
    assert_eq!(registry.borrow().entries["req-1"].phase, StreamPhase::Cancelling);

    let before = canceller.tick(1_000 + CANCEL_TIMEOUT_MS - 1);
    assert!(before.is_empty(), "1ms before the deadline nothing fires");
    assert!(
        timeouts(&before).is_empty()
            && registry.borrow().entries["req-1"].phase == StreamPhase::Cancelling,
        "still cancelling 1ms before the deadline"
    );

    let at = canceller.tick(1_000 + CANCEL_TIMEOUT_MS);
    assert_eq!(
        timeouts(&at),
        vec!["req-1"],
        "the fallback calls onCancelTimeout exactly once at 8s"
    );
    assert_eq!(
        registry.borrow().entries["req-1"].phase,
        StreamPhase::Terminal,
        "forced terminal at 8s"
    );
    assert_eq!(
        registry.borrow().entries["req-1"].terminal_kind,
        Some(StreamTerminalKind::Cancelled),
        "forced terminal kind is cancelled"
    );
    assert_eq!(
        registry.borrow().settle_calls,
        vec![("req-1".to_owned(), StreamTerminalKind::Cancelled)],
        "settle(req-1, cancelled) was called exactly once"
    );
    assert!(!canceller.has_timer("req-1"), "the fired timer is deleted");

    let after = canceller.tick(1_000 + CANCEL_TIMEOUT_MS + 1);
    assert!(timeouts(&after).is_empty(), "a late tick must be a no-op");
}

#[test]
fn a_normal_settled_event_clears_the_timer_and_suppresses_the_timeout() {
    let (mut canceller, mut registry) = harness(true, StreamPhase::Streaming);
    canceller.on_cancel_result("req-1", Ok(marked("req-1")), 1_000);
    assert!(canceller.has_timer("req-1"));

    // The backend delivers its normal terminal chunk before 8s.
    registry.settle("req-1", StreamTerminalKind::Done);
    assert!(
        !canceller.has_timer("req-1"),
        "the settled subscription must clear that request's fallback"
    );

    let effects = canceller.tick(1_000 + CANCEL_TIMEOUT_MS);
    assert!(
        timeouts(&effects).is_empty(),
        "no onCancelTimeout after a normal settle"
    );
    assert_eq!(
        registry.borrow().settle_calls,
        vec![("req-1".to_owned(), StreamTerminalKind::Done)],
        "the fallback must not force a second settle"
    );
}

#[test]
fn a_timer_that_fires_after_an_external_terminal_is_a_noop() {
    let (mut canceller, registry) = harness(true, StreamPhase::Streaming);
    canceller.on_cancel_result("req-1", Ok(marked("req-1")), 1_000);

    // Direct test-only mutation: entry reached terminal without a settled
    // event, so the timer is still armed. The fallback must still do nothing.
    registry.borrow_mut().entries.get_mut("req-1").expect("req-1").phase = StreamPhase::Terminal;

    let effects = canceller.tick(1_000 + CANCEL_TIMEOUT_MS);
    assert!(timeouts(&effects).is_empty(), "no timeout for an already-terminal entry");
    assert!(
        registry.borrow().settle_calls.is_empty(),
        "no forced settle when the entry is already terminal"
    );
}

// --- Disposal + idempotent re-arm -------------------------------------------

#[test]
fn re_arm_is_idempotent_and_dispose_clears_timers_and_unsubscribes() {
    let (mut canceller, registry) = harness(true, StreamPhase::Streaming);
    let first = canceller.on_cancel_result("req-1", Ok(marked("req-1")), 1_000);
    let first_handle = armed(&first.effects).expect("first arm").0;

    // markCancelling() on an already-cancelling entry returns true, so a second
    // marked receipt re-arms: clear the old handle, install a fresh deadline.
    let second = canceller.on_cancel_result("req-1", Ok(marked("req-1")), 2_000);
    assert!(
        second
            .effects
            .contains(&CancelEffect::ClearTimer { handle: first_handle }),
        "armFallback clears the existing timer first"
    );
    let (second_handle, second_deadline) = armed(&second.effects).expect("re-arm");
    assert_ne!(second_handle, first_handle, "re-arm allocates a fresh handle");
    assert_eq!(
        second_deadline,
        2_000 + CANCEL_TIMEOUT_MS,
        "re-arm times out from the new mark, not the old one"
    );

    assert_eq!(registry.borrow().listeners.len(), 1, "one subscription is live");
    let disposed = canceller.dispose();
    assert!(
        disposed.contains(&CancelEffect::ClearTimer { handle: second_handle }),
        "dispose clears the pending timer"
    );
    assert!(!canceller.has_timer("req-1"));
    assert_eq!(
        registry.borrow().listeners.len(),
        0,
        "dispose unsubscribes from the registry"
    );
    assert!(canceller.cancel("req-1").is_empty(), "a disposed canceller emits nothing");
    assert!(
        canceller.tick(2_000 + CANCEL_TIMEOUT_MS).is_empty(),
        "a disposed canceller never fires the fallback"
    );
}

// --- The runtime wake is real, not a boolean that never expires -------------

#[derive(Default)]
struct Probe {
    canceller: Option<SessionCanceller>,
    press_clock: Option<f64>,
    log: Vec<String>,
}

fn wake_probe() -> Probe {
    let registry = Rc::new(RefCell::new(FakeRegistry::streaming("req-1")));
    Probe {
        canceller: Some(SessionCanceller::new(
            Box::new(registry),
            CANCEL_TIMEOUT_MS,
        )),
        ..Default::default()
    }
}

fn wake_view(_state: &Probe, _cx: &mut Cx<Probe>) -> El {
    rune::row()
        .key("cancel")
        .role("button")
        .label("cancel")
        .w(4.0)
        .h(4.0)
        .on_press_tx(|state: &mut Probe, tx: &mut Tx| {
            state.press_clock = Some(tx.clock());
            let now_ms = (tx.clock() * 1_000.0) as i64;
            let canceller = state
                .canceller
                .as_mut()
                .expect("the probe holds a canceller");
            let _ = canceller.cancel("req-1");
            let step = canceller.on_cancel_result("req-1", Ok(marked("req-1")), now_ms);
            for effect in step.effects {
                match effect {
                    CancelEffect::ArmFallback { deadline_ms, .. } => {
                        // Host bridge: an armed deadline becomes a real runtime
                        // wake, which `rt.wake_deadline()` observes.
                        tx.wake_at(deadline_ms as f64 / 1_000.0);
                    }
                    other => state.log.push(format!("{other:?}")),
                }
            }
        })
}

fn click(rt: &mut Runtime<Probe>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn a_marked_cancel_schedules_a_real_8s_runtime_wake() {
    let mut rt = Runtime::new(wake_probe(), wake_view, (40.0, 40.0), env());
    rt.frame(0.0);
    assert_eq!(
        rt.wake_deadline(),
        None,
        "nothing should wake before a cancel is armed"
    );

    click(&mut rt, "cancel");
    let press = rt.state.press_clock.expect("the press handler ran");
    let deadline = rt
        .wake_deadline()
        .expect("a marked cancel must schedule a runtime wake for the fallback");
    assert!(
        (deadline - press - 8.0).abs() <= 0.01,
        "the fallback wake must be exactly 8s after the press clock: press={press} deadline={deadline}"
    );
}
