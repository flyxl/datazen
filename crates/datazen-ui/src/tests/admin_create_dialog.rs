//! T013: `AdminCreateDialog` — the create-database / create-schema dialog.
//!
//! Pure-logic tests call the state machine directly; runtime tests render
//! the dialog in a probe `Runtime` and read back the laid-out element tree —
//! never the component's own report of itself. Geometry is asserted on
//! `rt.elements()` rects, which come from the layout engine, not from the
//! code under test; the zh-CN strings are pinned against the literals in
//! `packages/frontend/src/locales/zh-CN/core.ts` and `schema.ts`.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use crate::components::admin::{
    AdminCreateDialog, AdminCreateDialogConfig, AdminCreateDialogState, AdminSchemaField,
};
use crate::i18n::t;

// Literal zh-CN text from packages/frontend/src/locales/zh-CN/core.ts and
// schema.ts — pinned here, not through `t()`.
const TITLE_DB: &str = "创建数据库";
const TITLE_SCHEMA: &str = "创建 Schema";
const NAME_LABEL_DB: &str = "数据库名称";
const ENCODING_LABEL: &str = "字符编码";
const OWNER_LABEL: &str = "所有者";
const CANCEL: &str = "取消";
const COPIED: &str = "已复制";

fn db_config() -> AdminCreateDialogConfig {
    AdminCreateDialogConfig::new(
        "session-1",
        "create_database",
        "common.createDatabase",
        "createDb.name",
        "my_database",
    )
    .field_label("encoding", ENCODING_LABEL)
    .field_label("owner", OWNER_LABEL)
}

fn schema_config() -> AdminCreateDialogConfig {
    AdminCreateDialogConfig::new(
        "session-1",
        "create_schema",
        "common.createSchema",
        "createSchema.name",
        "my_schema",
    )
    .field_label("owner", OWNER_LABEL)
    .database("app")
}

fn db_definition() -> Vec<AdminSchemaField> {
    let mut encoding = AdminSchemaField::string_field("encoding");
    encoding.example = Some("UTF8".to_string());
    let mut owner = AdminSchemaField::string_field("owner");
    owner.title = Some("Owner".to_string());
    vec![
        AdminSchemaField::string_field("name"),
        encoding,
        owner,
        AdminSchemaField {
            name: "template".to_string(),
            schema_type: Some("boolean".to_string()),
            ..Default::default()
        },
    ]
}

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

struct Probe {
    dialog: AdminCreateDialogState,
    dark: bool,
}

impl Probe {
    fn new(config: AdminCreateDialogConfig) -> Self {
        Self {
            dialog: AdminCreateDialogState::new(config),
            dark: true,
        }
    }

    /// An open dialog with the `create_database` definition loaded.
    fn open_db() -> Self {
        let mut probe = Self::new(db_config());
        probe.dialog.set_definition(db_definition());
        probe.dialog.open_dialog();
        probe
    }
}

fn host_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    AdminCreateDialog::<Probe>::new(&state.dialog)
        .host_mut(|s: &mut Probe| &mut s.dialog)
        .build(state.dark, &mut layer);
    rune_kit::root(
        col()
            .key("probe-page")
            .w_full()
            .h_full()
            .child(col().key("probe-content").w_full().h_full()),
        layer,
    )
}

/// The same dialog with no accessor: the children render unwired, covering
/// the `None` arms of every button and field.
fn bare_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    AdminCreateDialog::<Probe>::new(&state.dialog).build(state.dark, &mut layer);
    rune_kit::root(
        col()
            .key("probe-page")
            .w_full()
            .h_full()
            .child(col().key("probe-content").w_full().h_full()),
        layer,
    )
}

fn runtime(probe: Probe, w: f32, h: f32) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, host_view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

fn runtime_bare(probe: Probe, w: f32, h: f32) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, bare_view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
}

fn focused_key<S>(rt: &Runtime<S>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key)
}

/// The focused element's path. A text field's caret lives on its inner
/// `input` child (`.../admin-create-name-input/value/input`), so focus on
/// a field is asserted by path, not by the field's own key.
fn focused_path<S>(rt: &Runtime<S>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .map(|e| e.path)
}

/// Tab through the focus order until `key` is focused (at most 60 stops),
/// so keyboard tests do not depend on the exact tab order.
fn focus_by_tab<S>(rt: &mut Runtime<S>, key: &str) {
    for _ in 0..60 {
        if focused_key(rt).as_deref() == Some(key) {
            return;
        }
        press(rt, Key::Tab);
    }
    panic!("{key} never took focus after 60 Tabs");
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn element_text<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).text.unwrap_or_default()
}

fn any_text<S>(rt: &Runtime<S>, text: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.text.as_deref() == Some(text))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

/// Every laid-out element stays inside the window horizontally.
/// `rect` is `[x, y, w, h]`, so the right edge is `r[0] + r[2]`.
fn assert_no_horizontal_overflow<S>(rt: &Runtime<S>, w: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        if e.exiting {
            continue;
        }
        let r = e.rect;
        if r[0] < -0.5 || r[0] + r[2] > w + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) horizontally out of window {w}: {outside:?}",
        outside.len()
    );
}

fn rect_contains(outer: [f32; 4], inner: [f32; 4]) -> bool {
    inner[0] >= outer[0] - 0.5
        && inner[1] >= outer[1] - 0.5
        && inner[0] + inner[2] <= outer[0] + outer[2] + 0.5
        && inner[1] + inner[3] <= outer[1] + outer[3] + 0.5
}

// ---- i18n keys ------------------------------------------------------------

#[test]
fn the_keys_the_dialog_asks_for_exist() {
    assert_eq!(t("common.createDatabase"), TITLE_DB);
    assert_eq!(t("common.createSchema"), TITLE_SCHEMA);
    assert_eq!(t("createDb.name"), NAME_LABEL_DB);
    assert_eq!(t("createDb.encoding"), ENCODING_LABEL);
    assert_eq!(t("createDb.owner"), OWNER_LABEL);
    assert_eq!(t("createSchema.name"), "Schema 名称");
    assert_eq!(t("common.cancel"), CANCEL);
    assert_eq!(t("common.copy"), "复制");
    assert_eq!(t("common.copied"), COPIED);
}

// ---- schema-field model ---------------------------------------------------

#[test]
fn field_label_prefers_title_then_description_then_name() {
    let mut titled = AdminSchemaField::string_field("owner");
    titled.title = Some("Owner".to_string());
    titled.description = Some("The owner".to_string());
    assert_eq!(titled.label(), "Owner");
    let mut described = AdminSchemaField::string_field("owner");
    described.description = Some("The owner".to_string());
    assert_eq!(described.label(), "The owner");
    assert_eq!(AdminSchemaField::string_field("owner").label(), "owner");
    let mut blank = AdminSchemaField::string_field("owner");
    blank.title = Some(String::new());
    assert_eq!(blank.label(), "owner");
}

#[test]
fn field_placeholder_prefers_the_first_example_then_example() {
    let mut field = AdminSchemaField::string_field("encoding");
    field.example = Some("UTF8".to_string());
    field.examples = vec!["LATIN1".to_string(), "UTF8".to_string()];
    assert_eq!(field.placeholder(), Some("LATIN1"));
    field.examples.clear();
    assert_eq!(field.placeholder(), Some("UTF8"));
    assert_eq!(AdminSchemaField::string_field("x").placeholder(), None);
}

#[test]
fn only_string_fields_render_an_input() {
    assert!(AdminSchemaField::string_field("a").renders_input());
    assert!(AdminSchemaField::default().renders_input());
    let mut number = AdminSchemaField::string_field("n");
    number.schema_type = Some("number".to_string());
    assert!(!number.renders_input());
    let mut boolean = AdminSchemaField::string_field("b");
    boolean.schema_type = Some("boolean".to_string());
    assert!(!boolean.renders_input());
}

// ---- state machine ----------------------------------------------------------

#[test]
fn create_records_the_exact_execute_request() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    dialog.set_definition(db_definition());
    dialog.open_dialog();
    dialog.set_name("  app_db  ");
    dialog.set_optional("encoding", "  UTF8 ");
    dialog.set_optional("owner", "   ");
    dialog.create();
    assert!(dialog.running());
    assert_eq!(dialog.error(), None);
    assert_eq!(dialog.execute_requests().len(), 1);
    let request = &dialog.execute_requests()[0];
    assert_eq!(request.db_session_id, "session-1");
    assert_eq!(request.command, "create_database");
    assert_eq!(
        request.input,
        vec![
            ("name".to_string(), "app_db".to_string()),
            ("encoding".to_string(), "UTF8".to_string()),
        ],
        "the name is trimmed, blank optionals are dropped, the rest are trimmed"
    );
    assert_eq!(request.database, None);
}

#[test]
fn create_carries_the_database_pin_for_create_schema() {
    let mut dialog = AdminCreateDialogState::new(schema_config());
    dialog.set_definition(vec![AdminSchemaField::string_field("owner")]);
    dialog.open_dialog();
    dialog.set_name("myschema");
    dialog.create();
    assert_eq!(
        dialog.execute_requests()[0].database,
        Some("app".to_string())
    );
}

#[test]
fn create_is_a_no_op_on_a_blank_name() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    dialog.open_dialog();
    dialog.set_name("   ");
    dialog.create();
    assert!(!dialog.running());
    assert!(dialog.execute_requests().is_empty());
    assert!(!dialog.can_create());
}

#[test]
fn create_is_a_no_op_while_one_is_in_flight() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    dialog.open_dialog();
    dialog.set_name("app_db");
    dialog.create();
    dialog.set_name("other_db");
    dialog.create();
    assert_eq!(
        dialog.execute_requests().len(),
        1,
        "a second create while running must not fire a second execute"
    );
}

#[test]
fn execute_succeeded_resets_closes_and_records_the_created_name() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    dialog.set_definition(db_definition());
    dialog.open_dialog();
    dialog.set_name("app_db");
    dialog.set_optional("encoding", "UTF8");
    dialog.create();
    dialog.execute_succeeded();
    assert!(!dialog.is_open());
    assert!(!dialog.running());
    assert_eq!(dialog.name_text(), "");
    assert_eq!(dialog.optional_value("encoding"), "");
    assert_eq!(dialog.created_names(), ["app_db"]);
}

#[test]
fn execute_failed_keeps_the_form_and_shows_the_error() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    dialog.open_dialog();
    dialog.set_name("app_db");
    dialog.create();
    dialog.execute_failed("duplicate database");
    assert!(dialog.is_open());
    assert!(!dialog.running());
    assert_eq!(dialog.error(), Some("duplicate database"));
    assert_eq!(dialog.name_text(), "app_db");
    assert!(dialog.can_create(), "the user can retry after a failure");
}

#[test]
fn close_resets_the_form_but_open_does_not() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    dialog.set_definition(db_definition());
    dialog.open_dialog();
    dialog.set_name("app_db");
    dialog.set_optional("encoding", "UTF8");
    dialog.execute_failed("boom");
    dialog.close();
    assert!(!dialog.is_open());
    assert_eq!(dialog.name_text(), "");
    assert_eq!(dialog.optional_value("encoding"), "");
    assert_eq!(dialog.error(), None);
    dialog.open_dialog();
    dialog.set_name("kept");
    dialog.open_dialog();
    assert_eq!(dialog.name_text(), "kept", "reopening keeps the draft");
}

#[test]
fn set_definition_keeps_edits_for_surviving_fields() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    dialog.set_definition(db_definition());
    dialog.set_optional("encoding", "UTF8");
    dialog.set_optional("owner", "alice");
    let mut next = db_definition();
    next.retain(|field| field.name != "owner");
    let mut extra = AdminSchemaField::string_field("collate");
    extra.example = Some("C".to_string());
    next.push(extra);
    dialog.set_definition(next);
    assert_eq!(dialog.optional_value("encoding"), "UTF8");
    assert_eq!(dialog.optional_value("owner"), "");
    assert_eq!(dialog.optional_value("collate"), "");
    assert_eq!(
        dialog
            .visible_fields()
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        ["encoding", "collate"],
        "`name` is excluded and non-string fields never render"
    );
}

#[test]
fn the_field_label_override_wins_over_the_schema_title() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    dialog.set_definition(db_definition());
    let owner = dialog
        .visible_fields()
        .into_iter()
        .find(|field| field.name == "owner")
        .unwrap_or_else(|| panic!("owner is visible"));
    assert_eq!(dialog.field_label(owner), OWNER_LABEL);
    let encoding = dialog
        .visible_fields()
        .into_iter()
        .find(|field| field.name == "encoding")
        .unwrap_or_else(|| panic!("encoding is visible"));
    assert_eq!(dialog.field_label(encoding), ENCODING_LABEL);
}

// ---- runtime render ---------------------------------------------------------

#[test]
fn a_closed_dialog_mounts_nothing() {
    let rt = runtime(Probe::open_db(), 1440.0, 900.0);
    assert!(has_key(&rt, "admin-create-dialog"));
    let mut closed = Probe::new(db_config());
    closed.dialog.set_definition(db_definition());
    let rt = runtime(closed, 1440.0, 900.0);
    assert!(!has_key(&rt, "admin-create-dialog"));
    assert!(!has_key(&rt, "admin-create-name-input"));
    assert!(!has_key(&rt, "admin-create-submit"));
}

#[test]
fn an_unwired_dialog_mounts_nothing() {
    let rt = runtime_bare(Probe::open_db(), 1440.0, 900.0);
    assert!(!has_key(&rt, "admin-create-dialog"));
    assert!(!has_key(&rt, "admin-create-name-input"));
}

#[test]
fn open_dialog_renders_the_title_name_label_fields_and_footer() {
    let rt = runtime(Probe::open_db(), 1440.0, 900.0);
    assert_eq!(element_text(&rt, "admin-create-dialog-title"), TITLE_DB);
    assert_eq!(element_text(&rt, "admin-create-name-label"), NAME_LABEL_DB);
    assert_eq!(
        element_text(&rt, "admin-create-field-encoding-label"),
        ENCODING_LABEL
    );
    assert_eq!(
        element_text(&rt, "admin-create-field-owner-label"),
        OWNER_LABEL
    );
    assert!(has_key(&rt, "admin-create-name-input"));
    assert!(has_key(&rt, "admin-create-field-encoding"));
    assert!(has_key(&rt, "admin-create-field-owner"));
    assert!(
        !has_key(&rt, "admin-create-field-template"),
        "non-string fields render nothing"
    );
    assert!(
        !has_key(&rt, "admin-create-field-name"),
        "`name` is excluded from the schema fields"
    );
    assert!(any_text(&rt, CANCEL));
    assert!(any_text(&rt, TITLE_DB));
    assert!(!has_key(&rt, "admin-create-error"));
    assert!(!has_key(&rt, "admin-create-submit-spinner"));
    assert_no_horizontal_overflow(&rt, 1440.0, "dialog 1440x900");
}

#[test]
fn the_schema_dialog_uses_its_own_title_and_labels() {
    let mut probe = Probe::new(schema_config());
    probe
        .dialog
        .set_definition(vec![AdminSchemaField::string_field("owner")]);
    probe.dialog.open_dialog();
    let rt = runtime(probe, 1440.0, 900.0);
    assert_eq!(element_text(&rt, "admin-create-dialog-title"), TITLE_SCHEMA);
    assert_eq!(element_text(&rt, "admin-create-name-label"), "Schema 名称");
    assert_eq!(
        element_text(&rt, "admin-create-field-owner-label"),
        OWNER_LABEL
    );
}

/// Frame until the name input holds focus (at most 10 frames): the layout
/// hook fires after the first layout and the focus lands on a following
/// frame, so the count is timing, not behaviour.
fn settle_focus_on_name_input(rt: &mut Runtime<Probe>) {
    for _ in 0..10 {
        if focused_path(rt)
            .as_deref()
            .is_some_and(|path| path.contains("admin-create-name-input"))
        {
            return;
        }
        rt.frame(0.0);
    }
    panic!(
        "the name input never took focus, got focused path {:?}",
        focused_path(rt)
    );
}

#[test]
fn the_name_input_takes_focus_when_the_dialog_opens() {
    let mut rt = runtime(Probe::open_db(), 1440.0, 900.0);
    settle_focus_on_name_input(&mut rt);
}

#[test]
fn reopening_refocuses_the_name_input() {
    let mut rt = runtime(Probe::open_db(), 1440.0, 900.0);
    settle_focus_on_name_input(&mut rt);
    click(&mut rt, "admin-create-cancel");
    assert!(!rt.state.dialog.is_open());
    rt.state.dialog.open_dialog();
    // The open happened outside a handler, so the runtime needs an
    // explicit invalidate before the next frame re-runs the view.
    rt.invalidate();
    settle_focus_on_name_input(&mut rt);
}

#[test]
fn a_later_layout_does_not_steal_the_focus_back() {
    let mut rt = runtime(Probe::open_db(), 1440.0, 900.0);
    focus_by_tab(&mut rt, "admin-create-cancel");
    rt.frame(0.0);
    rt.frame(0.0);
    assert_eq!(
        focused_key(&rt).as_deref(),
        Some("admin-create-cancel"),
        "the focus request is one-shot"
    );
}

#[test]
fn typing_a_name_and_pressing_create_records_the_request() {
    let mut rt = runtime(Probe::open_db(), 1440.0, 900.0);
    click(&mut rt, "admin-create-name-input");
    rt.paste("app_db");
    rt.settle_all();
    rt.frame(0.0);
    assert_eq!(rt.state.dialog.name_text(), "app_db");
    click(&mut rt, "admin-create-submit");
    assert_eq!(rt.state.dialog.execute_requests().len(), 1);
    assert_eq!(
        rt.state.dialog.execute_requests()[0].input,
        vec![("name".to_string(), "app_db".to_string())]
    );
    assert!(rt.state.dialog.running());
    assert!(
        has_key(&rt, "admin-create-submit-spinner"),
        "the running button shows its spinner"
    );
}

#[test]
fn pressing_create_with_a_blank_name_records_nothing() {
    let mut rt = runtime(Probe::open_db(), 1440.0, 900.0);
    click(&mut rt, "admin-create-submit");
    assert!(
        rt.state.dialog.execute_requests().is_empty(),
        "the submit button is disabled while the name is blank"
    );
    click(&mut rt, "admin-create-name-input");
    rt.paste("   ");
    rt.settle_all();
    rt.frame(0.0);
    click(&mut rt, "admin-create-submit");
    assert!(rt.state.dialog.execute_requests().is_empty());
}

#[test]
fn enter_in_the_name_field_submits() {
    let mut rt = runtime(Probe::open_db(), 1440.0, 900.0);
    click(&mut rt, "admin-create-name-input");
    rt.paste("app_db");
    rt.settle_all();
    rt.frame(0.0);
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.dialog.execute_requests().len(), 1);
}

#[test]
fn optional_field_values_flow_into_the_request() {
    let mut rt = runtime(Probe::open_db(), 1440.0, 900.0);
    click(&mut rt, "admin-create-name-input");
    rt.paste("app_db");
    click(&mut rt, "admin-create-field-encoding");
    rt.paste("UTF8");
    rt.settle_all();
    rt.frame(0.0);
    click(&mut rt, "admin-create-submit");
    assert_eq!(
        rt.state.dialog.execute_requests()[0].input,
        vec![
            ("name".to_string(), "app_db".to_string()),
            ("encoding".to_string(), "UTF8".to_string()),
        ]
    );
}

#[test]
fn cancel_closes_and_resets() {
    let mut rt = runtime(Probe::open_db(), 1440.0, 900.0);
    click(&mut rt, "admin-create-name-input");
    rt.paste("app_db");
    rt.settle_all();
    rt.frame(0.0);
    click(&mut rt, "admin-create-cancel");
    assert!(!rt.state.dialog.is_open());
    assert_eq!(rt.state.dialog.name_text(), "");
    assert!(!has_key(&rt, "admin-create-dialog"));
}

#[test]
fn tab_reaches_cancel_and_enter_closes() {
    let mut rt = runtime(Probe::open_db(), 1440.0, 900.0);
    focus_by_tab(&mut rt, "admin-create-cancel");
    press(&mut rt, Key::Enter);
    assert!(!rt.state.dialog.is_open());
}

#[test]
fn a_failed_execute_shows_the_copyable_error() {
    let mut probe = Probe::open_db();
    probe.dialog.set_name("app_db");
    probe.dialog.create();
    probe
        .dialog
        .execute_failed("database \"app_db\" already exists");
    let rt = runtime(probe, 1440.0, 900.0);
    assert!(has_key(&rt, "admin-create-error"));
    assert_eq!(
        element_text(&rt, "admin-create-error-text"),
        "database \"app_db\" already exists"
    );
    assert!(has_key(&rt, "admin-create-error-copy"));
    assert_no_horizontal_overflow(&rt, 1440.0, "error dialog 1440x900");
}

#[test]
fn copying_the_error_flips_the_copied_feedback() {
    let mut probe = Probe::open_db();
    probe.dialog.set_name("app_db");
    probe.dialog.create();
    probe.dialog.execute_failed("boom");
    let mut rt = runtime(probe, 1440.0, 900.0);
    click(&mut rt, "admin-create-error-copy");
    assert!(rt.state.dialog.error_copied());
    assert!(any_text(&rt, COPIED));
}

// ---- geometry -----------------------------------------------------------------

#[test]
fn the_dialog_fits_the_minimum_window_with_its_actions_inside() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let rt = runtime(Probe::open_db(), w, h);
        let panel = element(&rt, "admin-create-dialog").rect;
        assert!(
            panel[0] >= -0.5 && panel[0] + panel[2] <= w + 0.5,
            "panel [{panel:?}] fits {w}px wide"
        );
        assert!(
            panel[1] >= -0.5 && panel[1] + panel[3] <= h + 0.5,
            "panel [{panel:?}] fits {h}px tall"
        );
        assert!(
            (panel[2] - 448.0).abs() <= 0.5,
            "panel is React's max-w-md (448pt), got width {}",
            panel[2]
        );
        for key in [
            "admin-create-name-input",
            "admin-create-field-encoding",
            "admin-create-cancel",
            "admin-create-submit",
        ] {
            let inner = element(&rt, key).rect;
            assert!(
                rect_contains(panel, inner),
                "{key} {inner:?} sits inside the panel {panel:?} at {w}x{h}"
            );
        }
        assert_no_horizontal_overflow(&rt, w, "dialog minimum window");
    }
}

#[test]
fn the_dialog_renders_the_same_in_the_light_theme() {
    let mut probe = Probe::open_db();
    probe.dark = false;
    let rt = runtime(probe, 960.0, 640.0);
    assert_eq!(element_text(&rt, "admin-create-dialog-title"), TITLE_DB);
    assert!(has_key(&rt, "admin-create-submit"));
    assert_no_horizontal_overflow(&rt, 960.0, "dialog light 960x640");
}

#[test]
fn optional_edit_creates_an_empty_edit_for_an_unknown_field() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    assert_eq!(dialog.optional_value("fresh"), "");
    dialog.set_optional("fresh", "v");
    assert_eq!(dialog.optional_value("fresh"), "v");
}

#[test]
fn field_label_falls_back_to_the_schema_title_without_an_override() {
    let config = AdminCreateDialogConfig::new(
        "session-1",
        "create_database",
        "common.createDatabase",
        "createDb.name",
        "my_database",
    );
    let mut dialog = AdminCreateDialogState::new(config);
    dialog.set_definition(db_definition());
    let owner = dialog
        .visible_fields()
        .into_iter()
        .find(|field| field.name == "owner")
        .unwrap_or_else(|| panic!("owner is visible"));
    assert_eq!(dialog.field_label(owner), "Owner");
}

#[test]
fn clicking_the_header_close_button_closes_and_resets() {
    let mut rt = runtime(Probe::open_db(), 1440.0, 900.0);
    click(&mut rt, "admin-create-name-input");
    rt.paste("app_db");
    rt.settle_all();
    rt.frame(0.0);
    click(&mut rt, "admin-create-dialog-close");
    assert!(!rt.state.dialog.is_open());
    assert_eq!(rt.state.dialog.name_text(), "");
}

#[test]
fn tab_reaches_submit_and_enter_submits() {
    let mut rt = runtime(Probe::open_db(), 1440.0, 900.0);
    click(&mut rt, "admin-create-name-input");
    rt.paste("app_db");
    rt.settle_all();
    rt.frame(0.0);
    focus_by_tab(&mut rt, "admin-create-submit");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.dialog.execute_requests().len(), 1);
}

#[test]
fn a_non_enter_key_on_the_footer_buttons_does_nothing() {
    let mut rt = runtime(Probe::open_db(), 1440.0, 900.0);
    click(&mut rt, "admin-create-name-input");
    rt.paste("app_db");
    rt.settle_all();
    rt.frame(0.0);
    focus_by_tab(&mut rt, "admin-create-submit");
    press(&mut rt, Key::Down);
    assert!(
        rt.state.dialog.execute_requests().is_empty(),
        "only Enter activates the submit button"
    );
    assert!(rt.state.dialog.is_open());
    focus_by_tab(&mut rt, "admin-create-cancel");
    press(&mut rt, Key::Down);
    assert!(rt.state.dialog.is_open(), "only Enter activates Cancel");
}
