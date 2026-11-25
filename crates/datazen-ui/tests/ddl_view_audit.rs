//! T322 independent audit (tester, not coder).
//!
//! Precise probes of branches the shipped 8 tests do not pin: the exact 2000ms
//! feedback wake (their tolerance is ±500ms), the copied deadline boundary, a
//! non-vacuous `visible`-rect geometry channel, and a temporary probe for a
//! whitespace-only DDL answer where React's `!result` differs from the port.

use std::rc::Rc;

use rune::{clipboard, col, Cx, El, Env, Runtime};

use datazen_ui::views::ddl_view::{build, DdlIdentity, DdlViewState};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct AuditScene {
    state: DdlViewState,
    dark: bool,
}

impl Default for AuditScene {
    fn default() -> Self {
        Self { state: DdlViewState::default(), dark: true }
    }
}

fn view(s: &AuditScene, _cx: &mut Cx<AuditScene>) -> El {
    let mut layer = Vec::new();
    let clock = Rc::new(std::cell::Cell::new(0.0f64));
    // Record the press clock through a copy hook; the hook also writes it into
    // the scene on the next frame via the runtime state.
    let hook_clock = clock.clone();
    let on_copy: Rc<dyn Fn(&mut AuditScene, &mut rune::Tx)> =
        Rc::new(move |_s: &mut AuditScene, tx: &mut rune::Tx| hook_clock.set(tx.clock()));
    let identity = DdlIdentity {
        table_name: "users".to_owned(),
        database: "app".to_owned(),
        database_type: None,
        is_view: false,
    };
    build::<AuditScene>(
        &identity,
        &s.state,
        "ddl-view",
        0.0,
        None,
        Some(on_copy),
        s.dark,
        &mut layer,
    );
    col().w_full().h_full().children(layer)
}

fn runtime(scene: AuditScene, size: (f32, f32)) -> Runtime<AuditScene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn copy_wake_is_exactly_2000ms_not_merely_within_500ms() {
    let mut scene = AuditScene::default();
    scene.state.ddl = "CREATE TABLE t();".to_owned();
    let mut rt = runtime(scene, DESIGN);
    let clip = clipboard::memory();
    rt.use_clipboard(&clip);
    rt.frame(0.0);
    click(&mut rt, "ddl-view-copy");
    let deadline = rt.wake_deadline().expect("copy schedules a wake");
    // The press happens at 1/60s into the frame, so the deadline is press + 2.0.
    assert!(
        (deadline - 2.0 - 1.0 / 60.0).abs() <= 0.02,
        "the feedback wake must be exactly 2000ms after the press, got {deadline}"
    );
}

#[test]
fn copied_flag_reverts_at_the_host_deadline() {
    let mut state = DdlViewState::default();
    state.copied_until = Some(5.0);
    assert!(state.copied(4.999), "just before expiry");
    assert!(!state.copied(5.0), "at expiry the feedback is gone");
    assert!(!state.copied(6.0), "after expiry");
    assert!(!DdlViewState::default().copied(0.0), "no deadline means never copied");
}

#[test]
fn all_branches_stay_inside_via_visible_rects() {
    for (name, state) in [
        ("loading", DdlViewState { loading: true, ..Default::default() }),
        ("error", DdlViewState { error: "x".repeat(400), ..Default::default() }),
        (
            "loaded",
            DdlViewState {
                ddl: "SELECT 'long line'.repeat(40);".to_owned(),
                ..Default::default()
            },
        ),
    ] {
        for size in [MIN, DESIGN] {
            for dark in [true, false] {
                let rt = runtime(AuditScene { state: state.clone(), dark, ..AuditScene::default() }, size);
                assert!(
                    !rt.elements().is_empty(),
                    "{name} {size:?} dark={dark}: something must be laid out"
                );
                for e in rt.elements() {
                    let [x, y, w, h] = e.visible;
                    assert!(
                        x >= -0.5 && y >= -0.5 && x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5,
                        "{name} {size:?} dark={dark}: {} visible={:?} rect={:?} crosses window",
                        e.key.as_deref().unwrap_or(&e.path), e.visible, e.rect
                    );
                }
            }
        }
    }
}

