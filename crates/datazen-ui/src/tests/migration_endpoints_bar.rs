//! T010: MigrationEndpointsBar — the controlled source/target endpoint bar.
//!
//! Probes the rendered element tree (keys, roles, labels, i18n text, conditional
//! blocks, disabled state, busy spinner), drives the real kit Selects through a
//! pointer journey, then checks key stability and two-theme/two-size geometry.
//! Every assertion observes the runtime tree, not the component's own report.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::migration::migration_endpoints_bar::{
    resolve_keys, resolve_prefix, EndpointOption, EndpointsBarVm, EndpointsLayout,
    MigrationEndpointsBar, MigrationI18nPrefix, SelectSlot,
};
use crate::i18n::{has, t};

const PREFIX: &str = "data-sync";

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

/// Host state: the VM snapshot plus every action counter. Callbacks write the
/// values back into the VM, exactly as a host view model would.
#[derive(Clone)]
struct Probe {
    vm: EndpointsBarVm,
    dark: bool,
    source_changes: Vec<String>,
    target_changes: Vec<String>,
    source_db_changes: Vec<String>,
    target_db_changes: Vec<String>,
    source_schema_changes: Vec<String>,
    target_schema_changes: Vec<String>,
    swaps: usize,
    refreshes: usize,
    toggles: usize,
    closes: usize,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            vm: EndpointsBarVm::default(),
            dark: true,
            source_changes: Vec::new(),
            target_changes: Vec::new(),
            source_db_changes: Vec::new(),
            target_db_changes: Vec::new(),
            source_schema_changes: Vec::new(),
            target_schema_changes: Vec::new(),
            swaps: 0,
            refreshes: 0,
            toggles: 0,
            closes: 0,
        }
    }
}

/// A bar with two real options per endpoint and both ids selected.
fn seeded() -> Probe {
    let mut p = Probe::default();
    p.vm.source.options = vec![
        EndpointOption::new("src-1", "Source One"),
        EndpointOption::new("src-2", "Source Two"),
    ];
    p.vm.target.options = vec![
        EndpointOption::new("dst-1", "Target One"),
        EndpointOption::new("dst-2", "Target Two"),
    ];
    p.vm.source.connection_id = "src-1".into();
    p.vm.target.connection_id = "dst-1".into();
    p.vm.source.databases = vec!["db_s1".into(), "db_s2".into()];
    p.vm.target.databases = vec!["db_t1".into(), "db_t2".into()];
    p.vm.source.database = "db_s1".into();
    p.vm.target.database = "db_t1".into();
    p.vm.compare_label = "Compare".into();
    p
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let bar = MigrationEndpointsBar::<Probe>::new(s.vm.clone())
        .on_source_change(|st: &mut Probe, v| {
            st.source_changes.push(v.clone());
            st.vm.source.connection_id = v;
            st.vm.open = None; // React closes the listbox on a choice
        })
        .on_target_change(|st: &mut Probe, v| {
            st.target_changes.push(v.clone());
            st.vm.target.connection_id = v;
            st.vm.open = None;
        })
        .on_source_database_change(|st: &mut Probe, v| {
            st.source_db_changes.push(v.clone());
            st.vm.source.database = v;
            st.vm.open = None;
        })
        .on_target_database_change(|st: &mut Probe, v| {
            st.target_db_changes.push(v.clone());
            st.vm.target.database = v;
            st.vm.open = None;
        })
        .on_source_schema_change(|st: &mut Probe, v| {
            st.source_schema_changes.push(v.clone());
            st.vm.source.schema = v;
            st.vm.open = None;
        })
        .on_target_schema_change(|st: &mut Probe, v| {
            st.target_schema_changes.push(v.clone());
            st.vm.target.schema = v;
            st.vm.open = None;
        })
        .on_swap(|st: &mut Probe| {
            st.swaps += 1;
            std::mem::swap(&mut st.vm.source.connection_id, &mut st.vm.target.connection_id);
            std::mem::swap(&mut st.vm.source.database, &mut st.vm.target.database);
        })
        .on_refresh(|st: &mut Probe| st.refreshes += 1)
        .on_toggle(|st: &mut Probe, slot: SelectSlot| {
            st.toggles += 1;
            st.vm.open = if st.vm.open == Some(slot) {
                None
            } else {
                Some(slot)
            };
        })
        .on_close(|st: &mut Probe| {
            st.closes += 1;
            st.vm.open = None;
        });
    col().w_full().h_full().child(bar.build(s.dark))
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn key_count<S>(rt: &Runtime<S>, key: &str) -> usize {
    rt.elements()
        .iter()
        .filter(|e| e.key.as_deref() == Some(key))
        .count()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    // Read the centre from the element tree: overlay rows (kit listboxes) can
    // exist in the scene a frame before `find` reports them as drawn.
    let r = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
        .rect;
    let (x, y) = (r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0); // let the overlay layer settle for the next probe
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements()
        .iter()
        .filter_map(|e| e.text.clone())
        .collect()
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

#[test]
fn i18n_keys_from_the_react_source_all_resolve_and_render_localized_text() {
    // The eleven keys the React source can ask for, in the order it declares them.
    for key in [
        "sync.source",
        "sync.target",
        "sync.selectSource",
        "sync.selectTarget",
        "common.database",
        "common.selectDatabase",
        "sync.schema",
        "sync.selectSchema",
        "sync.swapEndpoints",
        "transfer.readOnlyHint",
        "common.selectConnection",
        "transfer.source",
        "transfer.target",
    ] {
        assert!(has(key), "{key} must be in the zh-CN locale table");
        // A missing key echoes itself; a real entry yields different text
        // (e.g. "sync.schema" resolves to the ASCII "Schema", which is fine).
        assert_ne!(t(key), key, "{key} must not echo the key");
    }

    let mut rt = Runtime::new(seeded(), view, (960.0, 640.0), env());
    rt.frame(0.0);

    // The visible labels are the exact resolved keys, never raw key echoes.
    let known: Vec<&str> = vec![
        "sync.source",
        "sync.target",
        "common.database",
        "sync.selectSchema",
    ];
    let shown = texts(&rt);
    assert!(shown.iter().any(|s| s == t("sync.source")), "source label");
    assert!(shown.iter().any(|s| s == t("sync.target")), "target label");
    assert_eq!(
        element(&rt, &format!("{PREFIX}-source-label")).text.as_deref(),
        Some(t("sync.source")),
        "source column label is the i18n string"
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-target-label")).text.as_deref(),
        Some(t("sync.target")),
        "target column label is the i18n string"
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-source-database-label"))
            .text
            .as_deref(),
        Some(t("common.database")),
        "database label is the i18n string"
    );
    for key in known {
        assert!(!shown.iter().any(|s| s == key), "no rendered key echo: {key}");
    }
}

#[test]
fn resolved_keys_follow_the_react_prefix_and_empty_option_rules() {
    let mut vm = EndpointsBarVm::default();
    let keys = resolve_keys(&vm);
    assert_eq!(keys.i18n_prefix, MigrationI18nPrefix::Sync);
    assert_eq!(keys.source_label, "sync.source");
    assert_eq!(keys.target_label, "sync.target");
    assert_eq!(keys.source_placeholder, "sync.selectSource");
    assert_eq!(keys.target_placeholder, "sync.selectTarget");
    assert_eq!(keys.database_label, "common.database");
    assert_eq!(keys.select_database_placeholder, "common.selectDatabase");
    assert_eq!(keys.schema_label, "sync.schema");
    assert_eq!(keys.select_schema_placeholder, "sync.selectSchema");
    assert_eq!(keys.swap_tooltip, "sync.swapEndpoints");
    assert_eq!(keys.read_only_hint, "transfer.readOnlyHint");

    vm.include_empty_connection_option = true;
    let keys = resolve_keys(&vm);
    assert_eq!(keys.source_placeholder, "common.selectConnection");
    assert_eq!(keys.target_placeholder, "common.selectConnection");
    assert_eq!(keys.empty_connection_label, "common.selectConnection");

    vm.empty_connection_label_key = Some("sync.selectSource".to_owned());
    let keys = resolve_keys(&vm);
    assert_eq!(keys.source_placeholder, "sync.selectSource");

    vm.i18n_prefix = Some(MigrationI18nPrefix::Transfer);
    let keys = resolve_keys(&vm);
    assert_eq!(keys.source_label, "transfer.source");
    assert_eq!(keys.target_label, "transfer.target");

    assert_eq!(
        resolve_prefix("schema-diff-main", None),
        MigrationI18nPrefix::SchemaDiff
    );
    assert_eq!(
        resolve_prefix("data-transfer-wizard", None),
        MigrationI18nPrefix::Transfer
    );
    assert_eq!(resolve_prefix("sync-bar", None), MigrationI18nPrefix::Sync);
    assert_eq!(
        resolve_prefix("anything", Some(MigrationI18nPrefix::SchemaDiff)),
        MigrationI18nPrefix::SchemaDiff
    );
}

#[test]
fn source_and_target_selectors_render_and_a_choice_reaches_each_callback() {
    let mut rt = Runtime::new(seeded(), view, (960.0, 640.0), env());
    rt.frame(0.0);

    for side in ["source", "target"] {
        assert!(
            has_key(&rt, &format!("{PREFIX}-{side}")),
            "{side} column is mounted"
        );
        assert!(has_key(&rt, &format!("{PREFIX}-{side}-connection-field-trigger")));
        assert!(has_key(&rt, &format!("{PREFIX}-{side}-database-field-trigger")));
    }
    let source_trigger = element(&rt, &format!("{PREFIX}-source-connection-field-trigger"));
    assert_eq!(source_trigger.role, "combobox", "source is a real select");
    let target_trigger = element(&rt, &format!("{PREFIX}-target-connection-field-trigger"));
    assert_eq!(target_trigger.role, "combobox", "target is a real select");
    assert!(source_trigger.focusable, "source selector takes focus");
    assert!(target_trigger.focusable, "target selector takes focus");

    // Drive the real kit listbox: option0 is "Source One", option1 "Source Two".
    click(&mut rt, &format!("{PREFIX}-source-connection-field-trigger"));
    assert!(has_key(&rt, "option1"), "the source listbox opened");
    click(&mut rt, "option1");
    assert_eq!(
        rt.state.source_changes,
        vec!["src-2".to_owned()],
        "the source reports the option value, not its label"
    );
    assert_eq!(rt.state.vm.source.connection_id, "src-2");

    click(&mut rt, &format!("{PREFIX}-target-connection-field-trigger"));
    assert!(has_key(&rt, "option1"), "the target listbox opened");
    click(&mut rt, "option1");
    assert_eq!(
        rt.state.target_changes,
        vec!["dst-2".to_owned()],
        "the target reports the option value, not its label"
    );
    assert_eq!(rt.state.vm.target.connection_id, "dst-2");
}

#[test]
fn database_and_schema_selectors_report_values_and_manage_open_state() {
    let mut p = seeded();
    p.vm.source.schemas = vec!["public".into(), "audit".into()];
    let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
    rt.frame(0.0);

    assert!(has_key(&rt, &format!("{PREFIX}-source-schema")));
    assert!(!has_key(&rt, &format!("{PREFIX}-target-schema")));

    click(&mut rt, &format!("{PREFIX}-source-database-field-trigger"));
    assert!(rt.state.toggles >= 1, "a press routes the slot to the host");
    assert!(has_key(&rt, "option1"), "the database listbox opened");
    click(&mut rt, "option1");
    assert_eq!(
        rt.state.source_db_changes,
        vec!["db_s2".to_owned()],
        "a database choice reports its value"
    );
    assert_eq!(rt.state.vm.source.database, "db_s2");

    click(&mut rt, &format!("{PREFIX}-source-schema-field-trigger"));
    assert!(has_key(&rt, "option1"), "the schema listbox opened");
    click(&mut rt, "option1");
    assert_eq!(
        rt.state.source_schema_changes,
        vec!["audit".to_owned()],
        "a schema choice reports its value"
    );
    assert_eq!(rt.state.vm.source.schema, "audit");
}

#[test]
fn swap_swaps_the_endpoints_and_fires_once() {
    let mut rt = Runtime::new(seeded(), view, (960.0, 640.0), env());
    rt.frame(0.0);
    let swap = element(&rt, &format!("{PREFIX}-swap"));
    assert_eq!(swap.role, "button", "swap is a real button");
    assert!(swap.focusable, "swap is enabled when both ids are set");
    assert_eq!(swap.help.as_deref(), Some(t("sync.swapEndpoints")));

    click(&mut rt, &format!("{PREFIX}-swap"));
    assert_eq!(rt.state.swaps, 1, "swap fires exactly once");
    assert_eq!(rt.state.vm.source.connection_id, "dst-1");
    assert_eq!(rt.state.vm.target.connection_id, "src-1");

    // React disables swap while busy or when neither endpoint is selected.
    let mut p = seeded();
    p.vm.source.connection_id.clear();
    p.vm.target.connection_id.clear();
    let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(
        !element(&rt, &format!("{PREFIX}-swap")).focusable,
        "swap is disabled with no endpoints selected"
    );
    click(&mut rt, &format!("{PREFIX}-swap"));
    assert_eq!(rt.state.swaps, 0, "a disabled swap cannot fire");

    let mut p = seeded();
    p.vm.busy = true;
    let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(
        !element(&rt, &format!("{PREFIX}-swap")).focusable,
        "swap is disabled while busy"
    );
}

#[test]
fn refresh_fires_once_and_busy_swaps_the_refresh_glyph_for_a_spinner() {
    let mut rt = Runtime::new(seeded(), view, (960.0, 640.0), env());
    rt.frame(0.0);
    let compare = element(&rt, &format!("{PREFIX}-compare"));
    assert_eq!(compare.role, "button", "compare/refresh is a real button");
    assert!(has_key(&rt, &format!("{PREFIX}-compare-icon")));
    assert!(!has_key(&rt, &format!("{PREFIX}-compare-spinner")));
    assert!(
        texts(&rt).iter().any(|s| s == "Compare"),
        "the host compare label is rendered"
    );

    click(&mut rt, &format!("{PREFIX}-compare"));
    assert_eq!(rt.state.refreshes, 1, "refresh/compare fires exactly once");
    click(&mut rt, &format!("{PREFIX}-compare"));
    assert_eq!(rt.state.refreshes, 2, "a second press still reaches the host");

    let mut p = seeded();
    p.vm.busy = true;
    let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(
        has_key(&rt, &format!("{PREFIX}-compare-spinner")),
        "busy renders the keyed spinner"
    );
    assert!(
        !has_key(&rt, &format!("{PREFIX}-compare-icon")),
        "the refresh glyph is replaced while busy"
    );
    assert!(
        !element(&rt, &format!("{PREFIX}-compare")).focusable,
        "the busy compare button is disabled"
    );

    let mut p = seeded();
    p.vm.compare_disabled = true;
    let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(
        !element(&rt, &format!("{PREFIX}-compare")).focusable,
        "compareDisabled disables the button without a spinner"
    );
}

#[test]
fn session_errors_and_read_only_hints_follow_the_react_conditions() {
    let mut p = seeded();
    p.vm.source.session_error = Some("source refused".to_owned());
    p.vm.target.session_error = Some("target refused".to_owned());
    p.vm.target.read_only_hint = true;
    let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert_eq!(
        element(&rt, &format!("{PREFIX}-source-session-error"))
            .text
            .as_deref(),
        Some("source refused"),
        "source session error is the host message"
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-target-session-error"))
            .text
            .as_deref(),
        Some("target refused"),
        "target session error is the host message"
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-target-read-only-hint"))
            .text
            .as_deref(),
        Some(t("transfer.readOnlyHint")),
        "the read-only hint uses the i18n string"
    );
    assert!(
        !has_key(&rt, &format!("{PREFIX}-source-read-only-hint")),
        "the source column never shows the read-only hint"
    );

    // Without values, none of the three conditional blocks is mounted.
    let mut rt = Runtime::new(seeded(), view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(!has_key(&rt, &format!("{PREFIX}-source-session-error")));
    assert!(!has_key(&rt, &format!("{PREFIX}-target-session-error")));
    assert!(!has_key(&rt, &format!("{PREFIX}-target-read-only-hint")));
}

#[test]
fn grid_layout_and_hide_until_connected_match_the_react_conditions() {
    let mut p = seeded();
    p.vm.hide_database_until_connected = true;
    p.vm.source.connection_id.clear();
    p.vm.layout = EndpointsLayout::Grid;
    p.vm.footer_note = Some("footer note".to_owned());
    p.vm.action_note = Some("action note".to_owned());
    let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
    rt.frame(0.0);

    assert!(
        !has_key(&rt, &format!("{PREFIX}-source-database")),
        "grid hides the database block until a connection is chosen"
    );
    assert!(
        has_key(&rt, &format!("{PREFIX}-target-database")),
        "a connected target still shows its database block"
    );
    assert!(
        !has_key(&rt, &format!("{PREFIX}-source-database-label")),
        "grid layout hides the database label"
    );
    assert!(!has_key(&rt, &format!("{PREFIX}-swap")), "grid hides swap");
    assert!(
        !has_key(&rt, &format!("{PREFIX}-compare")),
        "grid hides compare"
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-footer-note"))
            .text
            .as_deref(),
        Some("footer note"),
        "grid uses footerNote over actionNote"
    );
    assert!(!has_key(&rt, &format!("{PREFIX}-action-note")));

    // Once connected, the source database block appears (still without a label).
    let mut rt = Runtime::new(
        {
            let mut p = seeded();
            p.vm.hide_database_until_connected = true;
            p.vm.layout = EndpointsLayout::Grid;
            p
        },
        view,
        (960.0, 640.0),
        env(),
    );
    rt.frame(0.0);
    assert!(has_key(&rt, &format!("{PREFIX}-source-database")));
    assert!(!has_key(&rt, &format!("{PREFIX}-source-database-label")));
}

#[test]
fn bar_layout_places_action_note_above_compare() {
    let mut p = seeded();
    p.vm.action_note = Some("action note".to_owned());
    let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
    rt.frame(0.0);
    let note = element(&rt, &format!("{PREFIX}-action-note"));
    let compare = element(&rt, &format!("{PREFIX}-compare"));
    assert!(
        note.rect[1] < compare.rect[1],
        "action note is above compare in bar layout"
    );
}

#[test]
fn component_owned_keys_are_stable_and_never_duplicated() {
    let owned = [
        format!("{PREFIX}"),
        format!("{PREFIX}-source"),
        format!("{PREFIX}-source-label"),
        format!("{PREFIX}-source-connection-field-trigger"),
        format!("{PREFIX}-source-database"),
        format!("{PREFIX}-source-database-label"),
        format!("{PREFIX}-source-database-field-trigger"),
        format!("{PREFIX}-target"),
        format!("{PREFIX}-target-label"),
        format!("{PREFIX}-target-connection-field-trigger"),
        format!("{PREFIX}-target-database"),
        format!("{PREFIX}-target-database-label"),
        format!("{PREFIX}-target-database-field-trigger"),
        format!("{PREFIX}-swap"),
        format!("{PREFIX}-compare-col"),
        format!("{PREFIX}-compare"),
        format!("{PREFIX}-compare-icon"),
    ];

    for dark in [true, false] {
        let mut p = seeded();
        p.dark = dark;
        let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
        rt.frame(0.0);
        for key in &owned {
            assert_eq!(key_count(&rt, key), 1, "owned key appears once: {key}");
        }
        // The runtime suffixes later duplicate keys with #dupN; none of our
        // keys may be renamed that way. kit-internal `value`/`chevron` are out
        // of scope (the same carve-out the other ports use).
        for e in rt.elements() {
            if let Some(k) = e.key.as_deref() {
                assert!(
                    !k.contains("#dup"),
                    "a component key was duplicated: {k}"
                );
            }
        }
    }
}

#[test]
fn no_horizontal_overflow_at_both_sizes_in_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut p = seeded();
            p.dark = dark;
            p.vm.busy = false;
            p.vm.include_empty_connection_option = true;
            p.vm.source.session_error = Some("source refused".to_owned());
            p.vm.target.session_error = Some("target refused".to_owned());
            p.vm.target.read_only_hint = true;
            p.vm.source.schemas = vec!["public".into(), "audit".into()];
            p.vm.target.schemas = vec!["core".into()];
            p.vm.compare_label = "Compare".into();
            let mut rt = Runtime::new(p, view, (w, h), env());
            rt.frame(0.0);
            assert_in_window(
                &rt,
                w,
                h,
                &format!("bar {w}x{h} dark={dark} with every block mounted"),
            );

            let mut p = seeded();
            p.dark = dark;
            p.vm.layout = EndpointsLayout::Grid;
            p.vm.footer_note = Some("A short note under the grid".to_owned());
            p.vm.target.read_only_hint = true;
            let mut rt = Runtime::new(p, view, (w, h), env());
            rt.frame(0.0);
            assert_in_window(&rt, w, h, &format!("grid {w}x{h} dark={dark}"));
        }
    }
}
