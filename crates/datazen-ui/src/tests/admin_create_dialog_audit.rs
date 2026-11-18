//! T013 independent audit: `AdminCreateDialog`.
//!
//! Fresh vectors against the React source
//! (`packages/frontend/src/components/admin/AdminCreateDialog.tsx`,
//! `AdminSchemaFields.tsx`, `lib/commandSchema.ts`, the two thin callers
//! and the zh-CN packs) — nothing here reuses the coder test file's
//! values. Runtime tests read `rt.elements()` rects from the layout
//! engine, never the component's own report of itself.
//!
//! Two behaviours pinned here diverge from React and are reported as
//! non-blocking findings (see the audit report):
//! * `audit_stale_optional_values_drop_with_the_definition` — React's
//!   `handleCreate` sends every `optionalValues` entry, including values
//!   for fields the definition no longer declares; Rune drops them with
//!   the definition. The Rune behaviour is arguably the fix, but it is a
//!   semantic change, so it is pinned, not assumed.
//! * `audit_error_copied_flag_survives_close_and_reopen` — React's
//!   `CopyableError` unmounts with the dialog and its "copied" state
//!   resets; Rune's `error_copied` survives `reset_form` / `close`, so a
//!   later error can render as already-copied. Cosmetic state leak.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use crate::components::admin::{
    AdminCreateDialog, AdminCreateDialogConfig, AdminCreateDialogState, AdminSchemaField,
};
use crate::i18n::t;

// Fresh literals, pinned against packages/frontend/src/locales/zh-CN/*.
const TITLE_DB: &str = "创建数据库";
const TITLE_SCHEMA: &str = "创建 Schema";
const CANCEL: &str = "取消";
const OWNER_ZH: &str = "所有者";

fn db_config() -> AdminCreateDialogConfig {
    AdminCreateDialogConfig::new(
        "session-9",
        "create_database",
        "common.createDatabase",
        "createDb.name",
        "my_database",
    )
    .field_label("encoding", "字符编码")
    .field_label("owner", OWNER_ZH)
}

fn schema_config() -> AdminCreateDialogConfig {
    AdminCreateDialogConfig::new(
        "session-9",
        "create_schema",
        "common.createSchema",
        "createSchema.name",
        "my_schema",
    )
    .field_label("owner", OWNER_ZH)
    .database("catalog_7")
}

/// Definition with different fields/values from the coder file: a
/// description-only field (label falls back to the description), an
/// `integer` field (never renders), and a field with no type at all.
fn audit_definition() -> Vec<AdminSchemaField> {
    let mut encoding = AdminSchemaField::string_field("encoding");
    encoding.examples = vec!["UTF8".to_string(), "SQL_ASCII".to_string()];
    let mut owner = AdminSchemaField::string_field("owner");
    owner.title = Some("Owner".to_string());
    owner.description = Some("The owner role".to_string());
    let mut locale = AdminSchemaField::string_field("lc_collate");
    locale.description = Some("排序规则".to_string());
    let mut size = AdminSchemaField::string_field("size_mb");
    size.schema_type = Some("integer".to_string());
    vec![
        AdminSchemaField::string_field("name"),
        encoding,
        owner,
        locale,
        AdminSchemaField::default(), // nameless, typeless field
        size,
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

    fn open_db() -> Self {
        let mut probe = Self::new(db_config());
        probe.dialog.set_definition(audit_definition());
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

fn runtime(probe: Probe, w: f32, h: f32) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, host_view, (w, h), env(dark));
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

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn element_text<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).text.unwrap_or_default()
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

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

/// Strict overlap (touching edges is fine): both axes must cross by more
/// than the 0.5pt layout tolerance.
fn rects_overlap(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] + 0.5 < b[0] + b[2] - 0.5
        && b[0] + 0.5 < a[0] + a[2] - 0.5
        && a[1] + 0.5 < b[1] + b[3] - 0.5
        && b[1] + 0.5 < a[1] + a[3] - 0.5
}

/// The text of a descendant of the element keyed `key` (button labels
/// live on a child text element, not on the button itself).
fn descendant_text<S>(rt: &Runtime<S>, key: &str, text: &str) -> bool {
    let parent = element(rt, key).path;
    let prefix = format!("{parent}/");
    rt.elements()
        .iter()
        .any(|e| e.path.starts_with(&prefix) && e.text.as_deref() == Some(text))
}

/// The accessible label of the inner `input` child of the field keyed
/// `key` — the kit names an input from its placeholder when the field
/// has no label, so the placeholder is observable here, not as text.
fn input_label<S>(rt: &Runtime<S>, key: &str) -> Option<String> {
    let parent = element(rt, key).path;
    let prefix = format!("{parent}/value/input");
    rt.elements()
        .into_iter()
        .find(|e| e.path == prefix)
        .and_then(|e| e.label)
}

// ---- i18n -------------------------------------------------------------------

#[test]
fn audit_keys_the_schema_caller_uses() {
    // The coder file pins createDb.* / createSchema.name; this one pins
    // the key only the CreateSchemaDialog caller passes.
    assert_eq!(t("createSchema.owner"), OWNER_ZH);
    assert_eq!(t("common.createSchema"), TITLE_SCHEMA);
}

// ---- state machine: fresh vectors ------------------------------------------

#[test]
fn audit_create_trims_and_keeps_inner_spaces() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    dialog.set_definition(audit_definition());
    dialog.open_dialog();
    dialog.set_name("  my db  ");
    dialog.set_optional("encoding", "  UTF 8  ");
    dialog.set_optional("owner", "  alice bob  ");
    dialog.create();
    assert_eq!(
        dialog.execute_requests()[0].input,
        vec![
            ("name".to_string(), "my db".to_string()),
            ("encoding".to_string(), "UTF 8".to_string()),
            ("owner".to_string(), "alice bob".to_string()),
        ],
        "trim the edges, keep the inside — React's name.trim() / value.trim()"
    );
}

#[test]
fn audit_create_with_no_definition_sends_only_the_name() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    dialog.open_dialog();
    dialog.set_name("solo_db");
    // A value typed for a field the (absent) definition never declared:
    // React would still send it (optionalValues is definition-blind),
    // Rune only sends visible fields.
    dialog.set_optional("ghost", "x");
    dialog.create();
    assert_eq!(
        dialog.execute_requests()[0].input,
        vec![("name".to_string(), "solo_db".to_string())]
    );
}

#[test]
fn audit_stale_optional_values_drop_with_the_definition() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    dialog.set_definition(audit_definition());
    dialog.open_dialog();
    dialog.set_name("fresh_db");
    dialog.set_optional("encoding", "UTF8");
    dialog.set_optional("owner", "alice");
    // The driver re-declares the command without `encoding`.
    let mut next = audit_definition();
    next.retain(|field| field.name != "encoding");
    dialog.set_definition(next);
    dialog.create();
    assert_eq!(
        dialog.execute_requests()[0].input,
        vec![
            ("name".to_string(), "fresh_db".to_string()),
            ("owner".to_string(), "alice".to_string()),
        ],
        "React would still send the stale encoding value; Rune drops it — pinned, see the file header"
    );
}

#[test]
fn audit_two_create_cycles_accumulate_outboxes() {
    let mut dialog = AdminCreateDialogState::new(schema_config());
    dialog.set_definition(vec![AdminSchemaField::string_field("owner")]);
    dialog.open_dialog();
    dialog.set_name("schema_one");
    dialog.create();
    dialog.execute_succeeded();
    dialog.open_dialog();
    dialog.set_name("schema_two");
    dialog.set_optional("owner", "bob");
    dialog.create();
    dialog.execute_succeeded();
    assert_eq!(dialog.created_names(), ["schema_one", "schema_two"]);
    assert_eq!(dialog.execute_requests().len(), 2);
    assert_eq!(
        dialog.execute_requests()[1].database,
        Some("catalog_7".to_string())
    );
    assert_eq!(
        dialog.execute_requests()[1].input,
        vec![
            ("name".to_string(), "schema_two".to_string()),
            ("owner".to_string(), "bob".to_string()),
        ]
    );
    assert!(!dialog.is_open());
}

#[test]
fn audit_open_epoch_bumps_only_on_open() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    assert_eq!(dialog.open_epoch(), 0);
    dialog.open_dialog();
    assert_eq!(dialog.open_epoch(), 1);
    dialog.close();
    assert_eq!(dialog.open_epoch(), 1, "closing does not bump the epoch");
    dialog.open_dialog();
    assert_eq!(dialog.open_epoch(), 2);
    dialog.open_dialog();
    assert_eq!(dialog.open_epoch(), 3, "opening twice bumps twice");
}

#[test]
fn audit_reset_form_leaves_a_flight_alone() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    dialog.open_dialog();
    dialog.set_name("in_flight");
    dialog.create();
    dialog.reset_form();
    assert!(dialog.running(), "a reset never cancels a flight");
    assert_eq!(dialog.execute_requests().len(), 1);
    assert_eq!(dialog.name_text(), "");
    assert_eq!(dialog.error(), None);
    dialog.execute_failed("late failure");
    assert!(!dialog.running());
    assert_eq!(dialog.error(), Some("late failure"));
}

#[test]
fn audit_close_while_running_then_failure_then_reopen() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    dialog.open_dialog();
    dialog.set_name("doomed_db");
    dialog.create();
    dialog.close();
    assert!(!dialog.is_open());
    assert!(dialog.running(), "closing does not cancel the flight");
    dialog.execute_failed("duplicate database");
    assert!(!dialog.is_open());
    assert_eq!(dialog.error(), Some("duplicate database"));
    // React parity: the late catch sets the error on the closed dialog,
    // so reopening shows it — the form was already reset by the close.
    dialog.open_dialog();
    assert_eq!(dialog.error(), Some("duplicate database"));
    assert_eq!(dialog.name_text(), "");
}

#[test]
fn audit_error_cleared_by_the_next_create_attempt() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    dialog.open_dialog();
    dialog.set_name("retry_db");
    dialog.create();
    dialog.execute_failed("first attempt failed");
    dialog.create();
    assert_eq!(dialog.error(), None, "handleCreate clears the error first");
    assert!(dialog.running());
    assert_eq!(dialog.execute_requests().len(), 2);
}

#[test]
fn audit_succeeded_records_the_trimmed_name() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    dialog.open_dialog();
    dialog.set_name("  spaced_db  ");
    dialog.create();
    dialog.execute_succeeded();
    assert_eq!(dialog.created_names(), ["spaced_db"]);
}

#[test]
fn audit_unicode_name_flows_through() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    dialog.set_definition(audit_definition());
    dialog.open_dialog();
    dialog.set_name("数据库_01");
    dialog.set_optional("lc_collate", "zh_CN.UTF-8");
    dialog.create();
    assert_eq!(
        dialog.execute_requests()[0].input,
        vec![
            ("name".to_string(), "数据库_01".to_string()),
            ("lc_collate".to_string(), "zh_CN.UTF-8".to_string()),
        ]
    );
}

#[test]
fn audit_set_definition_reorders_and_keeps_edits() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    dialog.set_definition(vec![
        AdminSchemaField::string_field("alpha"),
        AdminSchemaField::string_field("beta"),
    ]);
    dialog.set_optional("alpha", "A");
    dialog.set_optional("beta", "B");
    dialog.set_definition(vec![
        AdminSchemaField::string_field("beta"),
        AdminSchemaField::string_field("alpha"),
        AdminSchemaField::string_field("gamma"),
    ]);
    assert_eq!(
        dialog
            .visible_fields()
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        ["beta", "alpha", "gamma"],
        "visible order follows the new definition order"
    );
    assert_eq!(dialog.optional_value("alpha"), "A");
    assert_eq!(dialog.optional_value("beta"), "B");
    assert_eq!(dialog.optional_value("gamma"), "");
}

#[test]
fn audit_empty_string_label_override_wins() {
    // React: `labelOverride ?? fieldLabel(field)` — only null/undefined
    // fall through, so an empty-string override wins over the title.
    let config = AdminCreateDialogConfig::new(
        "session-9",
        "create_database",
        "common.createDatabase",
        "createDb.name",
        "my_database",
    )
    .field_label("owner", "");
    let mut dialog = AdminCreateDialogState::new(config);
    let mut owner = AdminSchemaField::string_field("owner");
    owner.title = Some("Owner".to_string());
    dialog.set_definition(vec![owner]);
    let field = dialog
        .visible_fields()
        .into_iter()
        .find(|field| field.name == "owner")
        .unwrap_or_else(|| panic!("owner is visible"));
    assert_eq!(dialog.field_label(field), "");
}

#[test]
fn audit_empty_first_example_wins_as_placeholder() {
    // React: `typeof examples[0] === 'string'` — "" is a string, so the
    // empty first example wins over `example`.
    let mut field = AdminSchemaField::string_field("encoding");
    field.example = Some("LATIN1".to_string());
    field.examples = vec![String::new(), "UTF8".to_string()];
    assert_eq!(field.placeholder(), Some(""));
}

#[test]
fn audit_blank_name_with_tabs_and_newlines_blocks_create() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    dialog.open_dialog();
    dialog.set_name(" \t\n ");
    assert!(!dialog.can_create());
    dialog.create();
    assert!(!dialog.running());
    assert!(dialog.execute_requests().is_empty());
}

#[test]
fn audit_error_copied_flag_survives_close_and_reopen() {
    // Pinned current behaviour, diverging from React: React's
    // CopyableError unmounts with the dialog so its copied state resets,
    // but Rune's flag survives reset_form / close — a later error renders
    // as already-copied without any copy press. Non-blocking; the fix is
    // clearing error_copied in reset_form.
    let mut dialog = AdminCreateDialogState::new(db_config());
    dialog.set_definition(audit_definition());
    dialog.open_dialog();
    dialog.set_name("copy_db");
    dialog.create();
    dialog.execute_failed("first boom");
    dialog.set_error_copied(true);
    dialog.close();
    dialog.open_dialog();
    dialog.set_name("copy_db");
    dialog.create();
    dialog.execute_failed("second boom");
    assert!(
        dialog.error_copied(),
        "the flag leaked across the close into the new error"
    );
}

#[test]
fn audit_focus_request_is_one_shot_at_the_state_level() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    assert!(!dialog.take_focus_name_request());
    dialog.open_dialog();
    assert!(
        dialog.take_focus_name_request(),
        "one open arms one request"
    );
    assert!(
        !dialog.take_focus_name_request(),
        "taking it twice yields false the second time"
    );
    dialog.open_dialog();
    assert!(dialog.take_focus_name_request(), "reopening arms it again");
}

#[test]
fn audit_open_consumes_the_focus_request_in_layout() {
    // The view's on_layout_tx hook consumes the one-shot request and
    // focuses the name input by key — so after the first frame the
    // request must already be spent, not still pending.
    let mut rt = runtime(Probe::open_db(), 1440.0, 900.0);
    rt.frame(0.0);
    assert!(
        !rt.state.dialog.take_focus_name_request(),
        "the layout hook consumed the focus request during the first frames"
    );
}

#[test]
fn audit_set_error_copied_round_trips_for_the_host_tick() {
    let mut dialog = AdminCreateDialogState::new(db_config());
    assert!(!dialog.error_copied());
    dialog.set_error_copied(true);
    assert!(dialog.error_copied());
    dialog.set_error_copied(false);
    assert!(!dialog.error_copied());
}

// ---- runtime render ---------------------------------------------------------

#[test]
fn audit_submit_and_cancel_buttons_carry_their_own_labels() {
    let rt = runtime(Probe::open_db(), 1440.0, 900.0);
    // React renders {title} as the primary button's label and
    // t('common.cancel') on the ghost button — pinned on the buttons
    // themselves (the label lives on a child text element).
    assert!(
        descendant_text(&rt, "admin-create-submit", TITLE_DB),
        "the primary button's label is the dialog title"
    );
    assert!(
        descendant_text(&rt, "admin-create-cancel", CANCEL),
        "the ghost button's label is common.cancel"
    );
}

#[test]
fn audit_name_and_schema_placeholders_are_visible() {
    let rt = runtime(Probe::open_db(), 1440.0, 900.0);
    // The kit names an input from its placeholder, so the placeholder is
    // observed as the inner input's accessible label while the field is
    // empty (its text is "").
    assert_eq!(
        input_label(&rt, "admin-create-name-input").as_deref(),
        Some("my_database"),
        "the name placeholder (the literal from CreateDatabaseDialog)"
    );
    assert_eq!(
        input_label(&rt, "admin-create-field-encoding").as_deref(),
        Some("UTF8"),
        "the encoding field's first example as its placeholder"
    );
}

#[test]
fn audit_enter_in_an_optional_field_does_not_submit() {
    // React's AdminSchemaFields Input has no onKeyDown — only the name
    // field submits on Enter.
    let mut rt = runtime(Probe::open_db(), 1440.0, 900.0);
    click(&mut rt, "admin-create-name-input");
    rt.paste("typed_db");
    click(&mut rt, "admin-create-field-encoding");
    rt.paste("UTF8");
    rt.settle_all();
    rt.frame(0.0);
    press(&mut rt, Key::Enter);
    assert!(
        rt.state.dialog.execute_requests().is_empty(),
        "Enter in an optional field must not create"
    );
    assert!(rt.state.dialog.is_open());
    click(&mut rt, "admin-create-submit");
    assert_eq!(rt.state.dialog.execute_requests().len(), 1);
}

#[test]
fn audit_sibling_rows_do_not_overlap_dark_and_light() {
    // Geometry from rt.elements() rects (layout engine output), at both
    // required sizes in both themes: the stacked label/input rows and the
    // footer buttons must not overlap each other, and everything must sit
    // inside the 448pt panel inside the window.
    const KEYS: [&str; 8] = [
        "admin-create-name-label",
        "admin-create-name-input",
        "admin-create-field-encoding-label",
        "admin-create-field-encoding",
        "admin-create-field-lc_collate-label",
        "admin-create-field-lc_collate",
        "admin-create-cancel",
        "admin-create-submit",
    ];
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut probe = Probe::open_db();
            probe.dark = dark;
            let rt = runtime(probe, w, h);
            let panel = element(&rt, "admin-create-dialog").rect;
            assert!(
                (panel[2] - 448.0).abs() <= 0.5,
                "panel is React's max-w-md (448pt), got {} at {w}x{h} dark={dark}",
                panel[2]
            );
            assert!(
                rect_contains([0.0, 0.0, w, h], panel),
                "panel {panel:?} fits {w}x{h} dark={dark}"
            );
            let mut rects: Vec<(&str, [f32; 4])> = Vec::new();
            for key in KEYS {
                let rect = element(&rt, key).rect;
                assert!(
                    rect_contains(panel, rect),
                    "{key} {rect:?} sits inside the panel {panel:?} at {w}x{h} dark={dark}"
                );
                rects.push((key, rect));
            }
            // The Dialog base's scrim covers the whole window (React's
            // bg-black/50 backdrop).
            let scrim = element(&rt, "admin-create-dialog-scrim").rect;
            assert!(
                (scrim[0] - 0.0).abs() <= 0.5
                    && (scrim[1] - 0.0).abs() <= 0.5
                    && (scrim[0] + scrim[2] - w).abs() <= 0.5
                    && (scrim[1] + scrim[3] - h).abs() <= 0.5,
                "scrim {scrim:?} covers the {w}x{h} window dark={dark}"
            );
            for i in 0..rects.len() {
                for j in (i + 1)..rects.len() {
                    assert!(
                        !rects_overlap(rects[i].1, rects[j].1),
                        "{} {:?} overlaps {} {:?} at {w}x{h} dark={dark}",
                        rects[i].0,
                        rects[i].1,
                        rects[j].0,
                        rects[j].1
                    );
                }
            }
            assert_no_horizontal_overflow(&rt, w, "audit overlap");
        }
    }
}

#[test]
fn audit_long_schema_list_keeps_footer_inside_the_window() {
    // 12 optional fields at the minimum window: the 400pt body cap must
    // scroll the list instead of pushing the footer out.
    let mut fields = vec![AdminSchemaField::string_field("name")];
    for i in 0..12 {
        fields.push(AdminSchemaField::string_field(format!("opt{i:02}")));
    }
    for dark in [true, false] {
        let mut probe = Probe::new(db_config());
        probe.dark = dark;
        probe.dialog.set_definition(fields.clone());
        probe.dialog.open_dialog();
        let rt = runtime(probe, 960.0, 640.0);
        let panel = element(&rt, "admin-create-dialog").rect;
        assert!(
            rect_contains([0.0, 0.0, 960.0, 640.0], panel),
            "panel {panel:?} fits 960x640 dark={dark}"
        );
        for key in [
            "admin-create-name-input",
            "admin-create-field-opt00",
            "admin-create-cancel",
            "admin-create-submit",
        ] {
            let inner = element(&rt, key).rect;
            assert!(
                rect_contains(panel, inner),
                "{key} {inner:?} sits inside the panel {panel:?} at 960x640 dark={dark}"
            );
            assert!(
                rect_contains([0.0, 0.0, 960.0, 640.0], inner),
                "{key} {inner:?} sits inside the 960x640 window dark={dark}"
            );
        }
        assert_no_horizontal_overflow(&rt, 960.0, "long list 960x640");
    }
}

#[test]
fn audit_long_error_stays_inside_the_panel() {
    let mut probe = Probe::open_db();
    probe.dialog.set_name("wide_db");
    probe.dialog.create();
    probe.dialog.execute_failed(&"x".repeat(200));
    let rt = runtime(probe, 960.0, 640.0);
    let panel = element(&rt, "admin-create-dialog").rect;
    let error = element(&rt, "admin-create-error").rect;
    // The body scrolls under its 400pt cap, so a long error's layout rect
    // may run below the fold — what must hold is that it is horizontally
    // inside the panel, starts inside it, and the footer stays visible.
    assert!(
        error[0] >= panel[0] - 0.5 && error[0] + error[2] <= panel[0] + panel[2] + 0.5,
        "error {error:?} is horizontally inside the panel {panel:?} at 960x640"
    );
    assert!(
        error[1] >= panel[1] - 0.5 && error[1] < panel[1] + panel[3],
        "error {error:?} starts inside the panel {panel:?} at 960x640"
    );
    for key in ["admin-create-cancel", "admin-create-submit"] {
        let footer = element(&rt, key).rect;
        assert!(
            rect_contains(panel, footer),
            "{key} {footer:?} sits inside the panel {panel:?} with a long error"
        );
        assert!(
            rect_contains([0.0, 0.0, 960.0, 640.0], footer),
            "{key} {footer:?} sits inside the 960x640 window with a long error"
        );
    }
    assert_no_horizontal_overflow(&rt, 960.0, "long error 960x640");
}

#[test]
fn audit_running_light_theme_shows_spinner_and_blocks_second_submit() {
    let mut probe = Probe::open_db();
    probe.dark = false;
    probe.dialog.set_name("light_db");
    probe.dialog.create();
    let mut rt = runtime(probe, 1440.0, 816.0);
    assert_eq!(element_text(&rt, "admin-create-dialog-title"), TITLE_DB);
    assert!(
        has_key(&rt, "admin-create-submit-spinner"),
        "the running button shows its large spinner in the light theme"
    );
    click(&mut rt, "admin-create-submit");
    assert_eq!(
        rt.state.dialog.execute_requests().len(),
        1,
        "the disabled running button fires no second execute"
    );
    assert!(rt.state.dialog.running());
    assert_no_horizontal_overflow(&rt, 1440.0, "running light 1440x816");
}
