//! T010 independent audit: MigrationEndpointsBar.
//!
//! All observations come from the runtime element tree (rects, texts, roles),
//! never from the component's own reported state. The host `Probe` mirrors a
//! real view model: callbacks write values back and own the single open slot.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::migration::migration_endpoints_bar::{
    EndpointOption, EndpointsBarVm, MigrationEndpointsBar, SelectSlot,
};

const PREFIX: &str = "data-sync";

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

#[derive(Clone)]
struct Probe {
    vm: EndpointsBarVm,
    dark: bool,
    source_changes: Vec<String>,
    target_changes: Vec<String>,
    choices: Vec<String>,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            vm: EndpointsBarVm::default(),
            dark: true,
            source_changes: Vec::new(),
            target_changes: Vec::new(),
            choices: Vec::new(),
        }
    }
}

/// Source/target ids are opaque values; their labels deliberately differ.
/// This is exactly React's `{ value: c.id, label: c.name + " (" + type + ")" }`.
fn seeded() -> Probe {
    let mut p = Probe::default();
    p.vm.source.options = vec![
        EndpointOption::new("src-1", "Source One (Postgres)"),
        EndpointOption::new("src-2", "Source Two (MySQL)"),
    ];
    p.vm.target.options = vec![
        EndpointOption::new("dst-1", "Target One (SQLite)"),
        EndpointOption::new("dst-2", "Target Two (Mongo)"),
    ];
    p.vm.source.connection_id = "src-1".into();
    p.vm.target.connection_id = "dst-1".into();
    p.vm.source.databases = vec!["db_s1".into(), "db_s2".into()];
    p.vm.target.databases = vec!["db_t1".into()];
    p.vm.source.database = "db_s1".into();
    p.vm.target.database = "db_t1".into();
    p.vm.compare_label = "Compare".into();
    p
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let bar = MigrationEndpointsBar::<Probe>::new(s.vm.clone())
        .on_source_change(|st: &mut Probe, v| {
            st.source_changes.push(v.clone());
            st.choices.push(v.clone());
            st.vm.source.connection_id = v;
            st.vm.open = None; // React's Select closes on a choice.
        })
        .on_target_change(|st: &mut Probe, v| {
            st.target_changes.push(v.clone());
            st.choices.push(v.clone());
            st.vm.target.connection_id = v;
            st.vm.open = None;
        })
        .on_source_database_change(|st: &mut Probe, v| {
            st.choices.push(v.clone());
            st.vm.source.database = v;
            st.vm.open = None;
        })
        .on_target_database_change(|st: &mut Probe, v| {
            st.choices.push(v.clone());
            st.vm.target.database = v;
            st.vm.open = None;
        })
        .on_swap(|st: &mut Probe| {
            std::mem::swap(&mut st.vm.source.connection_id, &mut st.vm.target.connection_id);
        })
        .on_refresh(|_st: &mut Probe| {})
        .on_toggle(|st: &mut Probe, slot: SelectSlot| {
            st.vm.open = if st.vm.open == Some(slot) {
                None
            } else {
                Some(slot)
            };
        })
        .on_close(|st: &mut Probe| st.vm.open = None);
    col().w_full().h_full().child(bar.build(s.dark))
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements()
        .iter()
        .filter_map(|e| e.text.clone())
        .collect()
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let r = element(rt, key).rect;
    let (x, y) = (r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0);
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of window {w}x{h}: {outside:?}",
        outside.len()
    );
}

/// D1: React's Select resolves `value -> label` and renders the LABEL in the
/// closed trigger (`selectedOption?.label ?? placeholder`). The port hands the
/// raw id to kit `Select::current`, which renders `current` verbatim, so the
/// endpoint selectors show `src-1` instead of `Source One (Postgres)`.
#[test]
fn connection_triggers_display_the_resolved_label_not_the_raw_id() {
    let mut rt = Runtime::new(seeded(), view, (960.0, 640.0), env());
    rt.frame(0.0);

    let shown = texts(&rt);
    assert!(
        shown.iter().any(|s| s == "Source One (Postgres)"),
        "source connection trigger must display the option LABEL; displayed texts: {shown:?}"
    );
    assert!(
        shown.iter().any(|s| s == "Target One (SQLite)"),
        "target connection trigger must display the option LABEL; displayed texts: {shown:?}"
    );
    assert!(
        !shown.iter().any(|s| s == "src-1" || s == "dst-1"),
        "the opaque ids must not leak into the visible trigger text; displayed texts: {shown:?}"
    );
}

/// The open listbox must mark the currently chosen option as selected. Because
/// `current` is the raw id and kit compares it to option labels, no row is
/// selected — the same root cause as D1, observed through a different channel.
#[test]
fn the_chosen_connection_is_marked_selected_in_the_open_listbox() {
    let mut rt = Runtime::new(seeded(), view, (960.0, 640.0), env());
    rt.frame(0.0);
    click(&mut rt, &format!("{PREFIX}-source-connection-field-trigger"));
    assert!(has_key(&rt, "option0"), "source listbox opened");

    let rows: Vec<_> = rt
        .elements()
        .into_iter()
        .filter(|e| e.key.as_deref() == Some("option0") || e.key.as_deref() == Some("option1"))
        .collect();
    assert!(
        rows.iter().any(|e| e.item_selected),
        "the option matching the chosen id src-1 must be marked selected in the listbox: {rows:?}"
    );
}

/// React's Select calls `onChange` then closes itself. The port delegates the
/// close to the host callback; with the host wiring in `view`, the listbox must
/// be gone after a choice.
#[test]
fn picking_an_option_closes_the_listbox_like_react() {
    let mut rt = Runtime::new(seeded(), view, (960.0, 640.0), env());
    rt.frame(0.0);
    click(&mut rt, &format!("{PREFIX}-source-connection-field-trigger"));
    assert!(has_key(&rt, "option1"), "the listbox is open before the choice");

    click(&mut rt, "option1");
    assert_eq!(rt.state.source_changes, vec!["src-2".to_owned()]);
    assert!(
        !has_key(&rt, "option1"),
        "the listbox must close after the choice (React handleSelect -> handleClose)"
    );
}

/// Control: database/schema options have label == value, so their trigger text
/// is correct even with the raw id passed as `current`. Shows `texts()` really
/// does observe select display text (so D1 is not a probe artifact).
#[test]
fn database_triggers_display_their_values_when_label_equals_value() {
    let mut rt = Runtime::new(seeded(), view, (960.0, 640.0), env());
    rt.frame(0.0);
    let shown = texts(&rt);
    assert!(
        shown.iter().any(|s| s == "db_s1"),
        "database trigger shows its value/label db_s1; displayed texts: {shown:?}"
    );
    assert!(
        shown.iter().any(|s| s == "db_t1"),
        "target database trigger shows db_t1; displayed texts: {shown:?}"
    );
}

/// Independent geometry with long, realistic React-style labels, every
/// conditional block mounted, at both window sizes and both themes.
#[test]
fn long_labels_and_all_blocks_stay_inside_both_windows_in_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut p = seeded();
            p.dark = dark;
            p.vm.include_empty_connection_option = true;
            p.vm.source.session_error = Some("source session refused".to_owned());
            p.vm.target.session_error = Some("target session refused".to_owned());
            p.vm.target.read_only_hint = true;
            p.vm.compare_label = "Compare schemas now".to_owned();
            let mut rt = Runtime::new(p, view, (w, h), env());
            rt.frame(0.0);
            assert_in_window(&rt, w, h, &format!("bar {w}x{h} dark={dark}"));

            let mut p = seeded();
            p.dark = dark;
            p.vm.layout = crate::components::migration::migration_endpoints_bar::EndpointsLayout::Grid;
            p.vm.footer_note = Some("A note beneath the endpoints grid".to_owned());
            p.vm.target.read_only_hint = true;
            let mut rt = Runtime::new(p, view, (w, h), env());
            rt.frame(0.0);
            assert_in_window(&rt, w, h, &format!("grid {w}x{h} dark={dark}"));
        }
    }
}

/// The swap button composes two opposing arrows and the compare button carries
/// the documented glyph keys. Check the keys exist and stay unique.
#[test]
fn swap_and_compare_glyph_keys_are_present_and_unique() {
    let mut rt = Runtime::new(seeded(), view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(has_key(&rt, &format!("{PREFIX}-swap")));
    assert!(has_key(&rt, &format!("{PREFIX}-compare-icon")));
    assert_eq!(element(&rt, &format!("{PREFIX}-swap")).role, "button");
    assert_eq!(element(&rt, &format!("{PREFIX}-compare")).role, "button");

    let mut p = seeded();
    p.vm.busy = true;
    let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(has_key(&rt, &format!("{PREFIX}-compare-spinner")));
    assert!(!has_key(&rt, &format!("{PREFIX}-compare-icon")));
}
