//! Independent audit for T029 history components (b45dc0455).
//! Focus: source discipline/i18n tokens, rendered i18n text, key structure,
//! bounds, and prop-driven copy feedback. (Reuse/structure 1:1 issues are
//! documented in the audit report; this file records executable checks.)

use rune::{col, Cx, El, Env, Runtime, TextEdit, Tx};

use crate::components::history::{
    HistoryConnectionOption, HistoryEntryCard, HistoryEntryVm, HistoryFilterBar, HistoryFilterOpen,
    HistoryQueryState,
};
use crate::i18n::{self, t};
use crate::icons::{ICON_CHECK, ICON_COPY, ICON_EXTERNAL_LINK, ICON_STAR, ICON_TRASH};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
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

fn find_node<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    if v.get("key").and_then(|x| x.as_str()) == Some(key) {
        return Some(v);
    }
    if let Some(arr) = v.get("children").and_then(|x| x.as_array()) {
        for c in arr {
            if let Some(f) = find_node(c, key) {
                return Some(f);
            }
        }
    }
    None
}

fn node_name(v: &serde_json::Value) -> Option<String> {
    v.get("name")
        .or_else(|| v.get("label"))
        .and_then(|x| x.as_str())
        .map(str::to_owned)
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    for el in rt.elements() {
        let [x, y, ew, eh] = el.rect;
        assert!(
            x >= -0.5 && y >= -0.5 && x + ew <= w + 0.5 && y + eh <= h + 0.5,
            "{label}: element {:?} escaped window ({w}x{h}): rect={:?}",
            el.key,
            el.rect
        );
    }
}

fn sample_entry() -> HistoryEntryVm {
    HistoryEntryVm {
        id: "audit-h1".to_string(),
        sql: "SELECT * FROM audit_table WHERE id = 1;".to_string(),
        success: true,
        execution_time_ms: 42,
        executed_at: "2026-11-18 12:00:00".to_string(),
        database: Some("audit_db".to_string()),
        schema: Some("public".to_string()),
        rows_affected: Some(7),
        error_message: None,
    }
}

#[derive(Clone, Default)]
struct CardState {
    copied: bool,
    copy_calls: usize,
}

fn card_view(state: &CardState, _cx: &mut Cx<CardState>) -> El {
    col().w_full().h_full().padding(16.0).child(
        HistoryEntryCard::<CardState>::new(sample_entry(), "AuditConn")
            .copied(state.copied)
            .can_open(true)
            .on_copy(|s: &mut CardState, _id, _tx: &mut Tx| {
                s.copy_calls += 1;
                s.copied = true;
            })
            .build(true),
    )
}

#[test]
fn audit_card_renders_i18n_labels_and_keys() {
    let mut rt = Runtime::new(CardState::default(), card_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    for key in [
        "history-entry-card-audit-h1",
        "history-entry-card-audit-h1-header",
        "history-entry-card-audit-h1-meta",
        "history-entry-card-audit-h1-select",
        "history-entry-card-audit-h1-status",
        "history-entry-card-audit-h1-connection",
        "history-entry-card-audit-h1-database",
        "history-entry-card-audit-h1-schema",
        "history-entry-card-audit-h1-time",
        "history-entry-card-audit-h1-rows",
        "history-entry-card-audit-h1-executed-at",
        "history-entry-card-audit-h1-actions",
        "history-entry-card-audit-h1-action-copy",
        "history-entry-card-audit-h1-action-favorite",
        "history-entry-card-audit-h1-action-open",
        "history-entry-card-audit-h1-action-delete",
        "history-entry-card-audit-h1-sql",
    ] {
        assert!(has_key(&rt, key), "expected key present: {key}");
    }
    let tree = rt.accessibility_json();
    let copy = find_node(&tree, "history-entry-card-audit-h1-action-copy").expect("copy node");
    assert_eq!(
        node_name(copy).as_deref(),
        Some(t("query.historyCopy")),
        "copy action accessible name comes from i18n"
    );
    let open = find_node(&tree, "history-entry-card-audit-h1-action-open").expect("open node");
    assert_eq!(
        node_name(open).as_deref(),
        Some(t("query.historyOpen")),
        "open action accessible name comes from i18n"
    );
    let fav = find_node(&tree, "history-entry-card-audit-h1-action-favorite").expect("fav node");
    assert_eq!(
        node_name(fav).as_deref(),
        Some(t("query.historyFavorite")),
        "favorite action accessible name comes from i18n"
    );
    let del = find_node(&tree, "history-entry-card-audit-h1-action-delete").expect("delete node");
    assert_eq!(
        node_name(del).as_deref(),
        Some(t("query.historyDeleteTitle")),
        "delete action accessible name comes from i18n"
    );
}

#[test]
fn audit_card_copy_feedback_uses_copied_prop_and_i18n() {
    // not copied
    let mut rt = Runtime::new(CardState::default(), card_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    let tree = rt.accessibility_json();
    let copy = find_node(&tree, "history-entry-card-audit-h1-action-copy").expect("copy node");
    assert_eq!(node_name(copy).as_deref(), Some(t("query.historyCopy")));
    // copied state
    let st = CardState {
        copied: true,
        copy_calls: 0,
    };
    let mut rt = Runtime::new(st, card_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    let tree = rt.accessibility_json();
    let copy = find_node(&tree, "history-entry-card-audit-h1-action-copy").expect("copy node");
    assert_eq!(
        node_name(copy).as_deref(),
        Some(t("common.copied")),
        "copied prop drives i18n copied label"
    );
}

#[test]
fn audit_card_geometry_bounds_dark_light() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut rt = Runtime::new(CardState::default(), card_view, (w, h), env(dark));
            rt.frame(0.0);
            assert_in_window(&rt, w, h, &format!("audit card dark={dark} {w}x{h}"));
        }
    }
}

#[derive(Clone)]
struct FilterState {
    query: HistoryQueryState,
    search: TextEdit,
    open: HistoryFilterOpen,
}

impl Default for FilterState {
    fn default() -> Self {
        Self {
            query: HistoryQueryState {
                connection_id: "c1".to_string(),
                ..HistoryQueryState::default()
            },
            search: TextEdit::new(String::new()),
            open: HistoryFilterOpen::None,
        }
    }
}

fn filter_view(state: &FilterState, _cx: &mut Cx<FilterState>) -> El {
    let mut layer = Vec::new();
    let bar = HistoryFilterBar::<FilterState>::new(state.query.clone(), &state.search)
        .connections(vec![HistoryConnectionOption {
            id: "c1".to_string(),
            name: "Conn A".to_string(),
        }])
        .databases(vec!["db1".to_string()])
        .schemas(vec!["public".to_string()])
        .open(state.open)
        .on_search_edit(|s: &mut FilterState| &mut s.search)
        .on_set_connection(|s: &mut FilterState, id| {
            s.query.connection_id = id;
            s.query.database.clear();
            s.query.schema.clear();
        })
        .on_toggle_open(|s: &mut FilterState, which| s.open = which)
        .on_close(|s: &mut FilterState| s.open = HistoryFilterOpen::None)
        .on_reset(|s: &mut FilterState| s.query = HistoryQueryState::default())
        .build(true, &mut layer);
    let mut root = col().w_full().h_full().padding(16.0).child(bar);
    for l in layer {
        root = root.child(l);
    }
    root
}

#[test]
fn audit_filter_bar_conditional_sections_and_reset_i18n() {
    let mut rt = Runtime::new(FilterState::default(), filter_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    assert!(has_key(&rt, "history-filter-bar"));
    assert!(has_key(&rt, "history-filter-bar-search"));
    // connection selected + databases/schemas provided => visible
    assert!(has_key(&rt, "history-filter-bar-database-trigger"));
    assert!(has_key(&rt, "history-filter-bar-schema-trigger"));
    // default query for this state has connection c1 (differs from all-default reset target in some fields?),
    // but reset visibility depends on changed vs HistoryQueryState::default().
    // Here connection_id is c1 != "all" so reset should be present and labeled with i18n.
    assert!(
        has_key(&rt, "history-filter-bar-reset-filters"),
        "reset visible when state differs from default"
    );
    let tree = rt.accessibility_json();
    let reset = find_node(&tree, "history-filter-bar-reset-filters").expect("reset node");
    assert_eq!(
        node_name(reset).as_deref(),
        Some(t("common.reset")),
        "reset label comes from i18n"
    );
}

#[test]
fn audit_filter_bar_geometry_bounds_dark_light() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut rt = Runtime::new(FilterState::default(), filter_view, (w, h), env(dark));
            rt.frame(0.0);
            assert_in_window(&rt, w, h, &format!("audit filter dark={dark} {w}x{h}"));
        }
    }
}

#[test]
fn audit_component_sources_have_no_hardcoded_cjk() {
    let card = include_str!("../components/history/history_entry_card.rs");
    let bar = include_str!("../components/history/history_filter_bar.rs");
    let module = include_str!("../components/history/mod.rs");
    for (name, src) in [
        ("history_entry_card.rs", card),
        ("history_filter_bar.rs", bar),
        ("history/mod.rs", module),
    ] {
        for (i, line) in src.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            assert!(
                !code.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
                "{name}:{} has hardcoded CJK in code: {}",
                i + 1,
                line
            );
        }
    }
}

#[test]
fn audit_i18n_keys_resolve_to_expected_zh_cn_values() {
    // Spot-check keys used by components resolve to the zh-CN pack values.
    assert_eq!(t("query.historyCopy"), "复制");
    assert_eq!(t("common.copied"), "已复制");
    assert_eq!(t("query.historyOpen"), "打开");
    assert_eq!(t("query.historyFavorite"), "收藏");
    assert_eq!(t("query.historyDeleteTitle"), "删除此记录");
    assert_eq!(t("query.historyDefaultDb"), "默认库");
    assert_eq!(t("query.searchHistory"), "搜索历史…");
    assert_eq!(t("query.historyScopeAll"), "全部");
    assert_eq!(t("query.historyAllDatabases"), "全部数据库");
    assert_eq!(t("query.historyAllSchemas"), "全部 Schema");
    assert_eq!(t("query.historyStatusAll"), "全部");
    assert_eq!(t("query.historyStatusSuccess"), "成功");
    assert_eq!(t("query.historyStatusFailed"), "失败");
    assert_eq!(t("query.historyRangeAll"), "全部时间");
    assert_eq!(t("query.historyRangeToday"), "最近 24 小时");
    assert_eq!(t("query.historyRange7d"), "最近 7 天");
    assert_eq!(t("query.historyRange30d"), "最近 30 天");
    assert_eq!(t("query.historyRange90d"), "最近 90 天");
    assert_eq!(t("query.historySortRecent"), "最新优先");
    assert_eq!(t("query.historySortOldest"), "最早优先");
    assert_eq!(t("query.historySortSlowest"), "最慢优先");
    assert_eq!(t("common.reset"), "重置");
}



fn audit_entry(sql: &str) -> HistoryEntryVm {
    HistoryEntryVm {
        id: "h1".to_string(),
        success: true,
        database: Some("appdb".to_string()),
        schema: Some("public".to_string()),
        execution_time_ms: 42,
        rows_affected: Some(7),
        executed_at: "2026-11-17 10:00:00".to_string(),
        sql: sql.to_string(),
        error_message: None,
    }
}

fn card_long_sql_audit_view(_state: &CardState, _cx: &mut Cx<CardState>) -> El {
    let entry = audit_entry(&("select ".to_string() + &"very_long_identifier_".repeat(40) + " from t;"));
    col().w_full().h_full().padding(16.0).child(
        HistoryEntryCard::<CardState>::new(entry, "AuditConn")
            .copied(false)
            .can_open(true)
            .build(true),
    )
}

#[test]
fn audit_every_component_translation_key_exists() {
    for key in [
        "common.copied", "common.reset", "query.historyAllDatabases", "query.historyAllSchemas",
        "query.historyCopy", "query.historyDefaultDb", "query.historyDeleteTitle",
        "query.historyFavorite", "query.historyOpen", "query.historyRange30d",
        "query.historyRange7d", "query.historyRange90d", "query.historyRangeAll",
        "query.historyRangeToday", "query.historyRows", "query.historyScopeAll",
        "query.historySelectAll", "query.historySortOldest", "query.historySortRecent",
        "query.historySortSlowest", "query.historyStatusAll", "query.historyStatusFailed",
        "query.historyStatusSuccess", "query.searchHistory",
    ] {
        assert!(i18n::has(key), "translation key missing: {key}");
        assert_ne!(t(key), key, "translation key did not resolve: {key}");
    }
}

#[test]
fn audit_new_action_icons_are_twenty_four_view() {
    for icon in [&ICON_CHECK, &ICON_COPY, &ICON_EXTERNAL_LINK, &ICON_STAR, &ICON_TRASH] {
        assert_eq!(icon.view, 24.0, "{} must be a 24-view icon", icon.name);
        assert!(!icon.body.is_empty(), "{} must have a body", icon.name);
    }
}

#[test]
fn audit_filter_search_icon_sits_inside_the_text_field() {
    let mut rt = Runtime::new(FilterState::default(), filter_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    let search = element(&rt, "history-filter-bar-search");
    let field = element(&rt, "history-filter-bar-search-field");
    let icon = element(&rt, "history-filter-bar-search-icon");
    assert!(
        icon.rect[0] >= field.rect[0] - 0.5
            && icon.rect[1] >= field.rect[1] - 0.5
            && icon.rect[0] + icon.rect[2] <= field.rect[0] + field.rect[2] + 0.5
            && icon.rect[1] + icon.rect[3] <= field.rect[1] + field.rect[3] + 0.5,
        "search icon {:?} not inside field {:?} (container {:?})", icon.rect, field.rect, search.rect
    );
}

#[test]
fn audit_long_sql_is_one_wrapped_text_block() {
    let mut rt = Runtime::new(CardState::default(), card_long_sql_audit_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    let sql = element(&rt, "history-entry-card-h1-sql");
    assert!(sql.text.as_deref().unwrap_or("").len() > 400, "full SQL is in the text");
    assert!(sql.rect[3] > 24.0, "long SQL must wrap, got {:?}", sql.rect);
    assert!(sql.rect[2] <= 928.5, "SQL stays inside card width");
    let matching = rt.elements().iter().filter(|e| e.text.as_deref() == sql.text.as_deref()).count();
    assert_eq!(matching, 1, "SQL is a single text block, not nested cells");
}

#[test]
fn audit_history_action_controls_are_real_buttons() {
    let mut rt = Runtime::new(CardState::default(), card_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    for key in [
        "history-entry-card-audit-h1-action-copy", "history-entry-card-audit-h1-action-favorite",
        "history-entry-card-audit-h1-action-open", "history-entry-card-audit-h1-action-delete",
    ] {
        assert_eq!(element(&rt, key).role, "button", "{key} role");
    }
}

#[test]
fn audit_copy_action_reuses_the_copyable_primitive() {
    let src = include_str!("../components/history/history_entry_card.rs");
    assert!(
        src.contains("CopyButton as Copyable") && src.contains("Copyable::new"),
        "copy action must mount the shared CopyButton base (alias kept for the literal)"
    );
    assert!(src.contains("on_copy_tx"), "copy action does not use on_copy_tx");
}

#[test]
fn audit_filter_bar_open_overlays_stay_inside_both_windows() {
    for open in [
        HistoryFilterOpen::None, HistoryFilterOpen::Connection, HistoryFilterOpen::Database,
        HistoryFilterOpen::Schema, HistoryFilterOpen::Status, HistoryFilterOpen::Range,
        HistoryFilterOpen::Sort,
    ] {
        for dark in [true, false] {
            for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
                let state = FilterState { open, ..FilterState::default() };
                let mut rt = Runtime::new(state, filter_view, (w, h), env(dark));
                rt.frame(0.0);
                assert_in_window(&rt, w, h, &format!("filter open={open:?} dark={dark} {w}x{h}"));
            }
        }
    }
}



#[test]
fn audit_card_copy_press_copies_and_schedules_the_feedback_wake() {
    let clipboard = rune::clipboard::memory();
    let mut rt = Runtime::new(CardState::default(), card_view, (960.0, 640.0), env(true));
    rt.use_clipboard(&clipboard);
    rt.frame(0.0);
    let before = rt.clock.max(0.0);
    let (x, y) = rt.find("history-entry-card-audit-h1-action-copy").expect("copy action laid out");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(
        clipboard.text().as_deref().map(String::as_str),
        Some("SELECT * FROM audit_table WHERE id = 1;"),
        "the card copy action must copy the exact SQL through CopyButton"
    );
    assert_eq!(rt.state.copy_calls, 1, "on_copy hook reached");
    let deadline = rt.wake_deadline().expect("CopyButton must schedule the 1.5s feedback wake");
    let delta = deadline - before;
    assert!(
        delta > 1.4 && delta < 1.7,
        "feedback wake must be ~1500ms after the press: before={before} deadline={deadline}"
    );
}

#[test]
fn audit_filter_bar_wraps_at_960_and_not_at_1440() {
    let mut rt = Runtime::new(FilterState::default(), filter_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    let row = element(&rt, "history-filter-bar-row");
    let search = element(&rt, "history-filter-bar-search");
    let sort = element(&rt, "history-filter-bar-sort-trigger");
    let bar = element(&rt, "history-filter-bar");
    assert!(row.rect[3] > 40.0, "960 must wrap onto a second line: row={:?}", row.rect);
    assert!(
        sort.rect[1] >= search.rect[1] + search.rect[3],
        "sort must be on the wrapped second line: sort={:?} search={:?}",
        sort.rect,
        search.rect
    );
    assert!(
        bar.rect[1] + bar.rect[3] <= 640.0,
        "wrapped bar must stay inside the window: bar={:?}",
        bar.rect
    );

    let mut rt = Runtime::new(FilterState::default(), filter_view, (1440.0, 816.0), env(true));
    rt.frame(0.0);
    let row = element(&rt, "history-filter-bar-row");
    let search = element(&rt, "history-filter-bar-search");
    let sort = element(&rt, "history-filter-bar-sort-trigger");
    assert!(row.rect[3] <= 34.0, "1440 must stay one line: row={:?}", row.rect);
    assert!(
        (sort.rect[1] - search.rect[1]).abs() <= 2.0,
        "1440 sort sits on the same line: sort={:?} search={:?}",
        sort.rect,
        search.rect
    );
}

#[test]
fn audit_filter_bar_uses_the_react_metric_widths() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut rt = Runtime::new(FilterState::default(), filter_view, (w, h), env(dark));
            rt.frame(0.0);
            let search = element(&rt, "history-filter-bar-search");
            let field = element(&rt, "history-filter-bar-search-field");
            let icon = element(&rt, "history-filter-bar-search-icon");
            assert!(search.rect[2] >= 180.0, "{w}x{h} dark={dark}: search min width {search:?}");
            assert!((icon.rect[0] - field.rect[0] - 10.0).abs() <= 0.5,
                "{w}x{h} dark={dark}: icon inset {:?} field {:?}", icon.rect, field.rect);
            assert!((icon.rect[1] - field.rect[1] - 8.0).abs() <= 0.5,
                "{w}x{h} dark={dark}: icon top {:?} field {:?}", icon.rect, field.rect);
            for (key, width) in [
                ("history-filter-bar-connection-trigger", 128.0),
                ("history-filter-bar-database-trigger", 112.0),
                ("history-filter-bar-schema-trigger", 112.0),
                ("history-filter-bar-status-trigger", 112.0),
                ("history-filter-bar-range-trigger", 128.0),
                ("history-filter-bar-sort-trigger", 128.0),
            ] {
                let r = element(&rt, key).rect;
                assert!((r[2] - width).abs() <= 0.5,
                    "{w}x{h} dark={dark}: {key} width {} != React {width}", r[2]);
            }
        }
    }
}
