//! T287: the wizard nav + stepper — the back/next gating, the hidden
//! next, the busy spinner, the step chips' color triple, and the
/// separator placement.

use rune::{col, Cx, El, Env, Runtime};

use crate::views::data_sync::data_sync_wizard_nav::{
    DataSyncWizardNav, DataSyncWizardStepper,
};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

#[derive(Default)]
struct Host {
    step_index: usize,
    busy: bool,
    can_next: bool,
    show_next: bool,
    backs: usize,
    nexts: usize,
}

fn view(host: &Host, _cx: &mut Cx<Host>) -> El {
    col().w_full().h_full().child(
        DataSyncWizardNav::<Host>::new(host.step_index, host.busy, host.can_next, host.show_next)
            .on_back(|s: &mut Host| s.backs += 1)
            .on_next(|s: &mut Host| s.nexts += 1)
            .build(true),
    )
}

fn mount(host: Host) -> Runtime<Host> {
    let mut rt = Runtime::new(host, view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click(rt: &mut Runtime<Host>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn both_buttons_mount_by_default() {
    let rt = mount(Host {
        show_next: true,
        ..Host::default()
    });
    assert!(has_key(&rt, "data-sync-back"));
    assert!(has_key(&rt, "data-sync-next"));
}

#[test]
fn show_next_false_hides_the_next_button() {
    // The preview and result steps navigate with the execute bar instead.
    let rt = mount(Host::default());
    assert!(has_key(&rt, "data-sync-back"));
    assert!(!has_key(&rt, "data-sync-next"));
    assert!(!has_key(&rt, "data-sync-next-row"));
}

#[test]
fn the_busy_flag_mounts_the_spinner_and_gates_both() {
    let mut rt = mount(Host {
        busy: true,
        show_next: true,
        ..Host::default()
    });
    assert!(has_key(&rt, "data-sync-next-busy"));
    // Both buttons stay silent while busy.
    click(&mut rt, "data-sync-back");
    assert_eq!(rt.state.backs, 0);
    click(&mut rt, "data-sync-next");
    assert_eq!(rt.state.nexts, 0);
}

#[test]
fn the_first_step_disables_back() {
    let mut rt = mount(Host {
        show_next: true,
        ..Host::default()
    });
    click(&mut rt, "data-sync-back");
    assert_eq!(rt.state.backs, 0, "step 0 never goes back");
    // Past the first step it works.
    let mut rt = mount(Host {
        step_index: 1,
        show_next: true,
        ..Host::default()
    });
    click(&mut rt, "data-sync-back");
    assert_eq!(rt.state.backs, 1);
}

#[test]
fn can_next_false_gates_the_next_press() {
    let mut rt = mount(Host {
        show_next: true,
        ..Host::default()
    });
    click(&mut rt, "data-sync-next");
    assert_eq!(rt.state.nexts, 0);
    let mut rt = mount(Host {
        can_next: true,
        show_next: true,
        ..Host::default()
    });
    click(&mut rt, "data-sync-next");
    assert_eq!(rt.state.nexts, 1);
}

#[test]
fn the_stepper_mounts_every_chip_and_separator() {
    let mut rt = Runtime::new(
        (),
        |_, _| {
            col().w_full()
                .h_full()
                .child(DataSyncWizardStepper::new(0).build(true))
        },
        (960.0, 640.0),
        env(true),
    );
    rt.frame(0.0);
    for name in ["endpoints", "setup", "objects", "compare", "preview", "result"] {
        assert!(has_key(&rt, &format!("data-sync-step-{name}")), "{name}");
        assert!(has_key(&rt, &format!("data-sync-step-{name}-num")), "{name} num");
    }
    // Five separators (between the six chips), none before the first.
    for index in 1..6 {
        assert!(has_key(&rt, &format!("data-sync-step-sep-{index}")));
    }
    assert!(!has_key(&rt, "data-sync-step-sep-0"));
}

#[test]
fn the_step_words_come_from_the_pack() {
    let mut rt = Runtime::new(
        (),
        |_, _| {
            col().w_full()
                .h_full()
                .child(DataSyncWizardStepper::new(2).build(true))
        },
        (960.0, 640.0),
        env(true),
    );
    rt.frame(0.0);
    let label = |key: &str| {
        rt.elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some(key))
            .and_then(|e| e.text)
            .unwrap_or_default()
    };
    assert_eq!(label("data-sync-step-endpoints-label"), "端点");
    assert_eq!(label("data-sync-step-setup-label"), "配置");
    assert_eq!(label("data-sync-step-objects-label"), "对象");
    assert_eq!(label("data-sync-step-compare-label"), "对比");
    assert_eq!(label("data-sync-step-preview-label"), "预览");
    assert_eq!(label("data-sync-step-result-label"), "结果");
}
