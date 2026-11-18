//! Independent retest of the T010 MigrationEndpointsBar D1/D2 fix.
//!
//! Every assertion reads the runtime element tree (texts, keys, item_selected),
//! never the component's own reported state. The host `Probe` mirrors a real
//! view model: callbacks write values back and own the single open slot.

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
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            vm: EndpointsBarVm::default(),
            dark: true,
            source_changes: Vec::new(),
            target_changes: Vec::new(),
        }
    }
}

/// Source/target ids are opaque values; their labels deliberately differ.
/// This is React's `{ value: c.id, label: c.name + " (" + type + ")" }`.
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
    p
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let bar = MigrationEndpointsBar::<Probe>::new(s.vm.clone())
        .on_source_change(|st: &mut Probe, v| {
            st.source_changes.push(v.clone());
            st.vm.source.connection_id = v;
            st.vm.open = None; // React's Select closes on a choice.
        })
        .on_target_change(|st: &mut Probe, v| {
            st.target_changes.push(v.clone());
            st.vm.target.connection_id = v;
            st.vm.open = None;
        })
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
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
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

/// D1, channel 1: the closed triggers resolve `value -> label` and render the
/// LABEL, never the opaque id. Both source and target must do this.
#[test]
fn both_source_and_target_triggers_resolve_values_to_labels() {
    let mut rt = Runtime::new(seeded(), view, (960.0, 640.0), env());
    rt.frame(0.0);
    let shown = texts(&rt);
    assert!(
        shown.iter().any(|s| s == "Source One (Postgres)"),
        "source trigger must show the option LABEL; texts: {shown:?}"
    );
    assert!(
        shown.iter().any(|s| s == "Target One (SQLite)"),
        "target trigger must show the option LABEL; texts: {shown:?}"
    );
    assert!(
        !shown.iter().any(|s| s == "src-1" || s == "dst-1"),
        "no opaque id may leak into visible trigger text; texts: {shown:?}"
    );
}

/// D1, channel 2: with each listbox open, the row matching the chosen id must
/// carry `item_selected = true` (kit compares `current` to the option LABEL).
#[test]
fn the_chosen_row_is_selected_in_both_open_listboxes() {
    for (slot, what) in [
        (SelectSlot::SourceConnection, "source"),
        (SelectSlot::TargetConnection, "target"),
    ] {
        let mut p = seeded();
        p.vm.open = Some(slot);
        let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
        rt.frame(0.0);
        assert!(has_key(&rt, "option0"), "{what} listbox opened");
        assert!(has_key(&rt, "option1"), "{what} listbox has both rows");

        let rows: Vec<_> = rt
            .elements()
            .into_iter()
            .filter(|e| e.key.as_deref() == Some("option0") || e.key.as_deref() == Some("option1"))
            .collect();
        // The chosen id (src-1 / dst-1) is each column's FIRST option -> option0.
        assert!(
            element(&rt, "option0").item_selected,
            "{what}: the row matching the chosen id must be marked selected: {rows:?}"
        );
        assert!(
            !element(&rt, "option1").item_selected,
            "{what}: the unchosen row must not be selected: {rows:?}"
        );
    }
}

/// An unknown or empty current id resolves to `None` and falls back to the
/// placeholder: no panic, no raw id leak, and a render identical to the empty
/// state, with no option marked selected.
#[test]
fn unknown_and_empty_current_fall_back_to_the_placeholder_without_panic() {
    // Baseline: both current ids empty -> placeholder state.
    let mut base = seeded();
    base.vm.source.connection_id = String::new();
    base.vm.target.connection_id = String::new();
    let mut brt = Runtime::new(base, view, (960.0, 640.0), env());
    brt.frame(0.0); // must not panic
    let base_shown = texts(&brt);

    for (source_id, target_id) in [("does-not-exist", ""), ("", "does-not-exist")] {
        let mut p = seeded();
        p.vm.source.connection_id = source_id.into();
        p.vm.target.connection_id = target_id.into();
        p.vm.open = Some(SelectSlot::SourceConnection);
        let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
        rt.frame(0.0); // must not panic

        let shown = texts(&rt);
        assert!(
            !shown.iter().any(|s| s == "does-not-exist"),
            "the unknown id must not be rendered; texts: {shown:?}"
        );
        // The only state difference from `base` is the id value (and listbox
        // openness, which adds option labels to both sides equally). Compare the
        // trigger-relevant texts: neither the unknown nor the empty id may show.
        assert!(
            shown.iter().any(|s| s == "sync.selectSource" || s == crate::i18n::t("sync.selectSource")),
            "the source trigger must fall back to its placeholder; texts: {shown:?}"
        );
        assert!(
            shown.iter().any(|s| s == "sync.selectTarget" || s == crate::i18n::t("sync.selectTarget")),
            "the target trigger must fall back to its placeholder; texts: {shown:?}"
        );
        assert!(has_key(&rt, "option0"), "source listbox opened");
        assert!(
            !element(&rt, "option0").item_selected && !element(&rt, "option1").item_selected,
            "no row may be selected when the current id is unknown/empty"
        );
    }

    // The empty-state baseline itself shows placeholders, not the ids.
    assert!(
        base_shown.iter().any(|s| s == "sync.selectSource" || s == crate::i18n::t("sync.selectSource")),
        "empty source id shows the placeholder; texts: {base_shown:?}"
    );
}

/// D1 fix must not break the label -> VALUE mapping on `on_choose`: kit answers
/// with the label, and the component must report the option's opaque value.
#[test]
fn on_choose_reports_the_value_not_the_label() {
    let mut rt = Runtime::new(seeded(), view, (960.0, 640.0), env());
    rt.frame(0.0);
    click(&mut rt, &format!("{PREFIX}-source-connection-field-trigger"));
    assert!(has_key(&rt, "option1"), "source listbox opened");
    click(&mut rt, "option1"); // label "Source Two (MySQL)", value "src-2"
    assert_eq!(
        rt.state.source_changes,
        vec!["src-2".to_owned()],
        "on_choose must report the option VALUE, not its label"
    );
    assert!(
        !rt.state.source_changes.iter().any(|v| v == "Source Two (MySQL)"),
        "the label must not leak through on_choose"
    );
}
