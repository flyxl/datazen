//! T100: the query error panel and the query context selectors.
//!
//! The error panel is exercised through a host state that records the
//! copy feedback deadline and every action; the selectors through a host
//! state carrying the listbox open flag and the choose log.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};
use rune_kit::root;

use datazen_ui::components::query::{
    level_label_key, path_hierarchy_placeholder_segments, PathHierarchySegment,
    QueryContextSelectors, QueryContextSelectorsState, QueryErrorPanel, COPIED_FEEDBACK_MS,
};
use datazen_ui::i18n::{has, t};
use datazen_ui::ui::kit::kit;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

// ---- the error panel ---------------------------------------------------------

#[derive(Default)]
struct ErrorProbe {
    copied_until: f64,
    now: f64,
    copied_calls: Vec<String>,
    diagnose: u32,
    explain: u32,
    fix_sql: u32,
    retry: u32,
    ask_in_chat: u32,
}

fn error_view(state: &ErrorProbe, cx: &mut Cx<ErrorProbe>) -> El {
    let dark = cx.system_dark();
    let panel = QueryErrorPanel::<ErrorProbe>::new("syntax error at line 3 near 'FROM'")
        .copied(state.copied_until > state.now)
        .on_diagnose(|s: &mut ErrorProbe| s.diagnose += 1)
        .on_explain(|s: &mut ErrorProbe| s.explain += 1)
        .on_fix_sql(|s: &mut ErrorProbe| s.fix_sql += 1)
        .on_retry(|s: &mut ErrorProbe| s.retry += 1)
        .on_ask_in_chat(|s: &mut ErrorProbe| s.ask_in_chat += 1)
        .on_copy_tx(|s: &mut ErrorProbe, tx: &mut rune::Tx| {
            s.copied_calls.push(format!("clock={}", tx.clock()));
            s.copied_until = tx.clock() + COPIED_FEEDBACK_MS as f64 / 1000.0;
        });
    col()
        .key("page")
        .w_full()
        .h_full()
        .padding(24.0)
        .child(panel.build(dark))
}

fn error_runtime(probe: ErrorProbe, size: (f32, f32), dark: bool) -> Runtime<ErrorProbe> {
    let mut rt = Runtime::new(probe, error_view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element<S: 'static>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key}: is laid out"))
}

fn has_element<S: 'static>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click<S: 'static>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key}: is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
}

#[test]
fn the_error_panel_renders_the_react_tree_and_geometry() {
    assert!(has("common.executionFailed"), "the pack has the header key");
    assert_eq!(t("common.executionFailed"), "执行失败");
    for dark in [true, false] {
        let rt = error_runtime(ErrorProbe::default(), DESIGN, dark);
        let panel = element(&rt, "query-error-panel");
        let k = kit(dark);
        // React: `rounded-md border border-danger/20 bg-danger/10 px-4 py-3`.
        // `ElementInfo` carries no border fields (the filter bar hit the
        // same wall and documents it); the wash, radius and padding are
        // the observable halves.
        assert_eq!(panel.radius, k.radius.sm, "rounded-md");
        assert_eq!(panel.fill, k.alpha(k.c.danger.fg, 0.1), "bg-danger/10");
        let header = element(&rt, "query-error-header");
        assert!((header.rect[0] - (panel.rect[0] + 16.0)).abs() <= 0.5, "px-4: {:?}", header.rect);
        assert!((header.rect[1] - (panel.rect[1] + 12.0)).abs() <= 0.5, "py-3: {:?}", header.rect);
        let title = element(&rt, "query-error-title");
        assert_eq!(title.text.as_deref(), Some("执行失败"), "the zh-CN header");
        assert_eq!(title.text_color, Some(k.c.danger.fg), "text-danger");
        // The copy control sits at the header's right edge (flex-1 label).
        let copy = element(&rt, "query-copy-error");
        assert_eq!(copy.label.as_deref(), Some(t("common.copy")), "the copy label");
        let message = element(&rt, "query-error-message");
        assert_eq!(
            message.text.as_deref(),
            Some("syntax error at line 3 near 'FROM'"),
            "the message renders verbatim"
        );
        assert_eq!(message.text_color, Some(k.c.danger.fg), "text-danger on the pre");
        // `font-mono` is not an ElementInfo field (no test channel for the
        // font family — the same boundary the code-block port documented);
        // the wrapping pre's width is the panel's content width.
        assert!((message.rect[0] - (panel.rect[0] + 16.0)).abs() <= 0.5, "the pre spans the padded width");
        assert!((message.rect[1] - (header.rect[1] + header.rect[3] + 8.0)).abs() <= 0.5, "mt-2");
        // No callbacks armed in the default probe... but the view arms all
        // of them, so the action row exists with every button.
        assert!(has_element(&rt, "query-error-actions"));
        for key in [
            "query-explain-error",
            "query-fix-sql",
            "query-retry",
            "query-ask-in-chat",
        ] {
            let button = element(&rt, key);
            assert_eq!(button.role, "button", "{key} is a button");
            let label = rt
                .elements()
                .into_iter()
                .find(|e| e.path == format!("{}/label", button.path) && !e.exiting)
                .unwrap_or_else(|| panic!("{}/label is laid out", button.path));
            assert_eq!(label.text_color, Some(k.c.text_accent), "React text-accent on {key}");
        }
        assert_eq!(
            element(&rt, "query-explain-error").label.as_deref(),
            Some(t("query.explainError")),
            "onExplain wins over onDiagnose: the lightbulb label"
        );
    }
}

#[test]
fn the_explain_fallback_prefers_explain_and_shows_diagnose_without_it() {
    // With only onDiagnose armed, the row shows the stethoscope label.
    let view = |state: &ErrorProbe, cx: &mut Cx<ErrorProbe>| -> El {
        let dark = cx.system_dark();
        let panel = QueryErrorPanel::<ErrorProbe>::new("boom")
            .on_diagnose(|s: &mut ErrorProbe| s.diagnose += 1);
        col().key("page").w_full().h_full().padding(24.0).child(panel.build(dark))
    };
    let mut rt = Runtime::new(ErrorProbe::default(), view, DESIGN, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    assert_eq!(
        element(&rt, "query-explain-error").label.as_deref(),
        Some(t("diagnosis.diagnose")),
        "React's explain fallback shows the diagnose label"
    );
    assert!(has_element(&rt, "query-error-actions"), "the action row mounts");

    // With no actions at all, the row does not mount.
    let bare = |state: &ErrorProbe, cx: &mut Cx<ErrorProbe>| -> El {
        let dark = cx.system_dark();
        let panel = QueryErrorPanel::<ErrorProbe>::new("boom");
        col().key("page").w_full().h_full().padding(24.0).child(panel.build(dark))
    };
    let mut rt = Runtime::new(ErrorProbe::default(), bare, DESIGN, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    assert!(!has_element(&rt, "query-error-actions"), "no action row without callbacks");
    assert!(has_element(&rt, "query-copy-error"), "the copy control always mounts");
}

#[test]
fn pressing_copy_writes_the_clipboard_and_records_the_deadline() {
    let mut rt = error_runtime(ErrorProbe::default(), DESIGN, true);
    click(&mut rt, "query-copy-error");
    assert_eq!(rt.state.copied_calls.len(), 1, "onCopy ran once");
    assert_eq!(
        rt.state.copied_until,
        rt.state.copied_calls[0].trim_start_matches("clock=").parse::<f64>().unwrap()
            + 1.5,
        "the caller recorded copied_until = clock + 1500ms"
    );
    // The control now shows the copied label (the feedback state the
    // caller computed on the next frame).
    rt.state.now = rt.state.copied_until - 0.1;
    rt.settle_all();
    rt.frame(0.0);
    assert_eq!(
        element(&rt, "query-copy-error").label.as_deref(),
        Some(t("common.copied")),
        "copied flips the label"
    );
}

#[test]
fn pressing_each_action_fires_only_its_callback() {
    let mut rt = error_runtime(ErrorProbe::default(), DESIGN, false);
    click(&mut rt, "query-fix-sql");
    assert_eq!(rt.state.fix_sql, 1);
    assert_eq!((rt.state.explain, rt.state.diagnose, rt.state.retry, rt.state.ask_in_chat), (0, 0, 0, 0));
    click(&mut rt, "query-retry");
    assert_eq!(rt.state.retry, 1);
    click(&mut rt, "query-ask-in-chat");
    assert_eq!(rt.state.ask_in_chat, 1);
    click(&mut rt, "query-explain-error");
    assert_eq!(rt.state.explain, 1, "onExplain wins the explain row");
    assert_eq!(rt.state.diagnose, 0, "the diagnose fallback never fires while explain exists");
    assert_eq!(rt.state.copied_calls.len(), 0, "no copy ran");
}

// ---- the context selectors ---------------------------------------------------

#[derive(Default)]
struct SelectorsProbe {
    selectors: QueryContextSelectorsState,
    selected: Vec<(usize, String)>,
    /// The caller-computed segments the path view renders (a fresh probe
    /// carries the hive namespace; the placeholder test swaps them).
    segments: Vec<PathHierarchySegment>,
    is_multi_db: bool,
    databases: Vec<String>,
    current_database: Option<String>,
    context_schema: Option<String>,
}

impl SelectorsProbe {
    /// A path-hierarchy namespace: `558` (label, single root) →
    /// `hive`-catalog / `snap`-schema multi-option levels with a chosen
    /// path `558/hive`, plus a trailing empty-options level.
    fn hive() -> Self {
        Self {
            segments: vec![
                PathHierarchySegment::Label { name: "558".to_string() },
                PathHierarchySegment::Select {
                    level_index: 0,
                    options: vec!["hive".to_string(), "impala".to_string()],
                    value: "hive".to_string(),
                },
                PathHierarchySegment::Select {
                    level_index: 1,
                    options: vec!["snap".to_string(), "tmp".to_string()],
                    value: String::new(),
                },
                PathHierarchySegment::Select {
                    level_index: 2,
                    options: Vec::new(),
                    value: String::new(),
                },
            ],
            ..Default::default()
        }
    }
}

fn flat_view(state: &SelectorsProbe, cx: &mut Cx<SelectorsProbe>) -> El {
    let dark = cx.system_dark();
    let mut layer = Vec::new();
    let selectors = QueryContextSelectors::<SelectorsProbe>::new(
        state.is_multi_db,
        &state.databases,
        state.current_database.as_deref(),
        state.context_schema.as_deref(),
    )
    .flat_state(&state.selectors)
    .host_mut(|s: &mut SelectorsProbe| &mut s.selectors)
    .on_select_level(|s: &mut SelectorsProbe, index, value| {
        s.selected.push((index, value.to_string()));
    })
    .build(dark, &mut layer);
    root(
        col()
            .key("page")
            .w_full()
            .h_full()
            .padding(24.0)
            .child(selectors.unwrap_or_else(|| col().key("empty").w_full().h(4.0))),
        layer,
    )
}

fn path_view(state: &SelectorsProbe, cx: &mut Cx<SelectorsProbe>) -> El {
    let dark = cx.system_dark();
    let mut layer = Vec::new();
    let selectors = QueryContextSelectors::<SelectorsProbe>::path_hierarchy(
        &state.segments,
        &state.selectors,
    )
    .host_mut(|s: &mut SelectorsProbe| &mut s.selectors)
    .on_select_level(|s: &mut SelectorsProbe, index, value| {
        s.selected.push((index, value.to_string()));
    })
    .build(dark, &mut layer)
    .expect("the path form always mounts");
    root(
        col().key("page").w_full().h_full().padding(24.0).child(selectors),
        layer,
    )
}

#[test]
fn the_flat_form_renders_database_select_and_schema_label() {
    let probe = SelectorsProbe {
        is_multi_db: true,
        databases: vec!["dev".to_string(), "prod".to_string(), "analytics".to_string()],
        current_database: Some("dev".to_string()),
        context_schema: Some("public".to_string()),
        ..Default::default()
    };
    let mut rt = Runtime::new(probe, flat_view, DESIGN, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    assert!(has_element(&rt, "query-context-selectors"), "the shell mounts");
    let k = kit(true);
    assert!(has_element(&rt, "query-context-database-trigger"), "the database select mounts");
    assert!(has_element(&rt, "query-context-schema"), "the schema label mounts");
    let schema = element(&rt, "query-context-schema");
    assert_eq!(schema.text.as_deref(), Some("/ public"), "React renders `/ {{schema}}`");
    assert_eq!(schema.text_color, Some(k.c.text_muted), "text-fg-muted");
    assert!(schema.rect[2] <= 140.0, "max-w-[140px] caps the label");
    // The database glyph leads the shell, 14pt (h-3.5 w-3.5).
    let glyph = rt
        .elements()
        .into_iter()
        .find(|e| e.path.ends_with("query-context-selectors/database-icon") && !e.exiting)
        .expect("the database glyph is laid out");
    assert_eq!(glyph.rect[2], 14.0, "h-3.5 w-3.5");
    assert_eq!(glyph.text_color, Some(k.c.text_muted), "text-fg-muted");
}

#[test]
fn the_flat_null_cases_render_nothing() {
    // Multi-db with no databases and no schema: React returns null.
    let probe = SelectorsProbe { is_multi_db: true, ..Default::default() };
    let mut rt = Runtime::new(probe, flat_view, MIN, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    assert!(!has_element(&rt, "query-context-selectors"), "no shell without multi-db databases or schema");
    assert!(has_element(&rt, "empty"), "the caller's null-arm placeholder shows");

    // A schema alone still mounts the shell with the label.
    let probe = SelectorsProbe {
        is_multi_db: false,
        context_schema: Some("app".to_string()),
        ..Default::default()
    };
    let mut rt = Runtime::new(probe, flat_view, MIN, Env { system_dark: false, scale: 1.0 });
    rt.frame(0.0);
    assert!(has_element(&rt, "query-context-selectors"));
    assert!(!has_element(&rt, "query-context-database-trigger"), "no database select: isMultiDb false");
    assert!(has_element(&rt, "query-context-schema"), "the schema label mounts");
}

#[test]
fn the_path_form_renders_labels_selects_and_separators() {
    let mut rt = Runtime::new(SelectorsProbe::hive(), path_view, DESIGN, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    assert!(has_element(&rt, "query-context-selectors"), "the shell mounts");
    // The fixed root label: `558`.
    let label = element(&rt, "query-context-path-label-0");
    assert_eq!(label.text.as_deref(), Some("558"), "single-option levels become labels");
    assert!(label.rect[2] <= 72.0, "max-w-[4.5rem] caps the label");
    // The selects at their React trigger keys with the hierarchy labels.
    assert_eq!(level_label_key(0, true), "query.catalog");
    assert_eq!(level_label_key(1, true), "query.schema");
    assert_eq!(level_label_key(2, true), "query.schema");
    assert_eq!(level_label_key(0, false), "query.database");
    assert_eq!(level_label_key(1, false), "query.catalog");
    assert_eq!(level_label_key(3, false), "query.schema");
    for key in ["query-context-level-0-trigger", "query-context-level-1-trigger"] {
        assert!(has_element(&rt, key), "{key} mounts");
    }
    // The trailing empty level is disabled (options empty).
    let empty = element(&rt, "query-context-level-2-trigger");
    assert!(!empty.focusable, "a select with no options is disabled");
    // The separators between all four segments.
    for index in 1..4 {
        let sep = element(&rt, &format!("query-context-separator-{index}"));
        assert_eq!(sep.text.as_deref(), Some("/"), "separator {index}");
    }
}

#[test]
fn the_placeholder_segments_render_two_disabled_selects() {
    let placeholder = path_hierarchy_placeholder_segments();
    assert_eq!(placeholder.len(), 2);
    let probe = SelectorsProbe { segments: placeholder, ..Default::default() };
    let mut rt = Runtime::new(probe, path_view, MIN, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    for key in ["query-context-level-0-trigger", "query-context-level-1-trigger"] {
        let select = element(&rt, key);
        assert!(!select.focusable, "{key} is disabled with no options");
    }
}

#[test]
fn choosing_from_a_level_select_delivers_the_level_and_value() {
    // Open level 0's listbox and choose `impala`.
    let mut rt = Runtime::new(SelectorsProbe::hive(), path_view, DESIGN, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    rt.state.selectors.open_level = Some(0);
    rt.settle_all();
    rt.frame(0.0);
    // The kit select renders its listbox into the layer while open; the
    // option rows carry the `option` role with the option label as their
    // text child. Find the `impala` row by its role and press its centre.
    let option = rt
        .elements()
        .into_iter()
        .find(|e| {
            e.role == "option"
                && e.path
                    .split('/')
                    .last()
                    .is_some_and(|last| last.starts_with("item"))
                && rt
                    .elements()
                    .iter()
                    .any(|t| t.path == format!("{}/label", e.path) && t.text.as_deref() == Some("impala"))
        })
        .or_else(|| {
            // Fallback: the label text element itself, when the option
            // row's child key differs.
            rt.elements().into_iter().find(|e| e.text.as_deref() == Some("impala"))
        })
        .expect("the impala option is laid out");
    let (x, y) = (option.rect[0] + option.rect[2] / 2.0, option.rect[1] + option.rect[3] / 2.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
    assert_eq!(
        rt.state.selected,
        vec![(0, "impala".to_string())],
        "onSelectLevel(0, 'impala') ran"
    );
    assert_eq!(rt.state.selectors.open_level, None, "the choose closed the listbox");
    assert_eq!(rt.state.selectors.focus_level, Some(1), "the pending focus level is 0 + 1");
}
