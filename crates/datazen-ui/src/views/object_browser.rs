//! Port of `windows/connection/ObjectBrowser.tsx`.
//!
//! React keeps five kind tabs, a loaded object list (spinner / copyable error /
//! empty state / rows), and a DDL detail panel with an execute button and a run
//! message. Row and editor context menus are native menus built from the two
//! `objectBrowserContextMenu` helpers. This port keeps that shape; every
//! command (load, DDL fetch, copy, execute, refresh) is an [`ObjectBrowserAction`]
//! an injected host port executes — no fake IPC, timer or thread.
//!
//! ## Keys
//!
//! React `data-testid`s are kept verbatim: `object-browser`,
//! `object-browser-{kind}` on each tab and `object-browser-item` on every row.
//! Rune keys must be unique, so rows carry a suffixed key and the base testid
//! is kept as a hidden marker on the first row. The remaining keys are named
//! after the React elements (`object-browser-list`, `-editor`, `-execute`, ...).
//!
//! ## Local stand-ins (removal stage)
//!
//! The `SqlEditor` engine is a separate port; the DDL is rendered as the
//! read-only `ddl_view` snapshot (as `database_object_view` does). Kind labels,
//! identity keys and the two menu builders are pure ports of their React
//! helpers. No production host mounts this view yet, so every reusable item
//! carries a per-item `#[allow(dead_code)]` with the removal stage "drop when
//! the connection workspace wires the object browser panel".

use std::rc::Rc;

use rune::{col, row, text, El, PointerPress, TextEdit};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::icons::{ICON_PLAY, ICON_REFRESH};
use crate::ui::copyable::CopyableError;
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize};

mod menus;

pub(crate) const ROOT_KEY: &str = "object-browser";
pub(crate) const TAB_PREFIX: &str = "object-browser-";
pub(crate) const ITEM_KEY: &str = "object-browser-item";
pub(crate) const LIST_KEY: &str = "object-browser-list";
pub(crate) const EDITOR_KEY: &str = "object-browser-editor";
pub(crate) const LIST_MENU_KEY: &str = "object-browser-list-menu";
pub(crate) const EDITOR_MENU_KEY: &str = "object-browser-editor-menu";

/// React `DatabaseObjectKind` and the tab order from `KINDS`.
#[allow(dead_code)] // contract mirror; remove when the host owns the panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum BrowserKind {
    #[default]
    Function,
    Procedure,
    Trigger,
    Sequence,
    Type,
}

impl BrowserKind {
    pub(crate) const ALL: [Self; 5] = [
        Self::Function,
        Self::Procedure,
        Self::Trigger,
        Self::Sequence,
        Self::Type,
    ];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Procedure => "procedure",
            Self::Trigger => "trigger",
            Self::Sequence => "sequence",
            Self::Type => "type",
        }
    }

    /// The tab label React renders for each kind.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Function => t("objects.function"),
            Self::Procedure => t("objects.procedure"),
            Self::Trigger => t("objects.trigger"),
            Self::Sequence => t("schemaTree.sequences"),
            Self::Type => t("schemaTree.types"),
        }
    }
}

/// React `DatabaseObject` plus the optional routine/trigger identity fields the
/// DDL command reads.
#[allow(dead_code)] // VM mirror; remove when the host owns the panel store.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BrowserObject {
    pub(crate) kind: BrowserKind,
    pub(crate) schema: Option<String>,
    pub(crate) name: String,
    pub(crate) signature: Option<String>,
    pub(crate) target_schema: Option<String>,
    pub(crate) target_name: Option<String>,
}

fn json_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn json_nullable(value: Option<&str>) -> String {
    value.map_or_else(|| "null".to_owned(), json_string)
}

/// React `databaseObjectIdentityKey`: the JSON of six identity fields, so two
/// overloaded routines with the same name stay independently selectable.
#[allow(dead_code)] // pure helper; remove when the shared helper is ported.
pub(crate) fn identity_key(object: &BrowserObject) -> String {
    format!(
        "[{},{},{},{},{},{}]",
        json_string(object.kind.as_str()),
        json_nullable(object.schema.as_deref()),
        json_string(&object.name),
        json_nullable(object.signature.as_deref()),
        json_nullable(object.target_schema.as_deref()),
        json_nullable(object.target_name.as_deref()),
    )
}

/// React `sameObjectIdentity`.
#[allow(dead_code)] // pure helper; remove when the shared helper is ported.
pub(crate) fn same_object_identity(left: Option<&BrowserObject>, right: &BrowserObject) -> bool {
    left.is_some_and(|left| identity_key(left) == identity_key(right))
}

/// React's eight `useState` values plus the two context-menu anchors.
#[allow(dead_code)] // VM; owned by the host until the panel view lands.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ObjectBrowserVm {
    pub(crate) kind: BrowserKind,
    pub(crate) objects: Vec<BrowserObject>,
    pub(crate) loading: bool,
    pub(crate) error: String,
    pub(crate) selected: Option<BrowserObject>,
    pub(crate) ddl: TextEdit,
    pub(crate) running: bool,
    pub(crate) run_message: String,
    pub(crate) list_menu: Option<PointerPress>,
    pub(crate) menu_object: Option<BrowserObject>,
    pub(crate) editor_menu: Option<PointerPress>,
}

#[allow(dead_code)] // VM helpers; remove with the host.
impl ObjectBrowserVm {
    /// React `disabled={running || !ddl.trim()}` on the execute button.
    pub(crate) fn execute_disabled(&self) -> bool {
        self.running || self.ddl.text.trim().is_empty()
    }

    /// React's header label: `kind · name` once selected, else `objects.pick`.
    pub(crate) fn selection_label(&self) -> String {
        match &self.selected {
            Some(object) => format!("{} · {}", object.kind.as_str(), object.name),
            None => t("objects.pick").to_owned(),
        }
    }

    /// Whether an object is still selected after a kind switch (React clears it).
    pub(crate) fn begin_kind(&mut self, kind: BrowserKind) {
        self.kind = kind;
        self.selected = None;
        self.ddl = TextEdit::new(String::new());
    }

    /// React `load` prologue.
    pub(crate) fn begin_load(&mut self) {
        self.loading = true;
        self.error.clear();
    }

    /// React's `.then` / `.catch` / `finally` of `load`.
    pub(crate) fn apply_load(&mut self, result: Result<Vec<BrowserObject>, String>) {
        self.loading = false;
        match result {
            Ok(objects) => {
                self.objects = objects;
                self.error.clear();
            }
            Err(message) => {
                self.objects.clear();
                self.error = message;
            }
        }
    }

    /// React `openObject`: selects, clears the run message, then the host feeds
    /// DDL through [`ObjectBrowserVm::apply_ddl`].
    pub(crate) fn select_object(&mut self, object: &BrowserObject) {
        self.selected = Some(object.clone());
        self.run_message.clear();
    }

    /// A failed DDL fetch is shown as a SQL comment in the editor.
    pub(crate) fn apply_ddl(&mut self, result: Result<String, String>) {
        self.ddl = TextEdit::new(match result {
            Ok(text) => text,
            Err(message) => format!("-- {message}"),
        });
    }

    /// React's execute answer: success shows `objects.executeOk`.
    pub(crate) fn apply_execute(&mut self, result: Result<(), String>) {
        self.running = false;
        self.run_message = match result {
            Ok(()) => t("objects.executeOk").to_owned(),
            Err(message) => message,
        };
    }

    /// Reference host reducer for the view-owned state; commands stay with the
    /// host. Consumers: the panel host (not landed) and the observation tests.
    pub(crate) fn reduce(&mut self, action: &ObjectBrowserAction) {
        match action {
            ObjectBrowserAction::SetKind(kind) => self.begin_kind(*kind),
            ObjectBrowserAction::Load { .. } | ObjectBrowserAction::Refresh { .. } => {
                self.begin_load()
            }
            ObjectBrowserAction::OpenObject { object } => self.select_object(object),
            ObjectBrowserAction::Execute { .. } => {
                self.running = true;
                self.run_message.clear();
            }
            ObjectBrowserAction::ListContextMenu { object, press } => {
                self.list_menu = Some(*press);
                self.menu_object = Some(object.clone());
            }
            ObjectBrowserAction::EditorContextMenu { press } => self.editor_menu = Some(*press),
            ObjectBrowserAction::CloseMenu => {
                self.list_menu = None;
                self.menu_object = None;
                self.editor_menu = None;
            }
            ObjectBrowserAction::CopyName { .. }
            | ObjectBrowserAction::CopyDdl { .. }
            | ObjectBrowserAction::FormatDdl
            | ObjectBrowserAction::ToggleComment => {}
        }
    }
}

/// Every interaction the view asks the host to perform.
#[allow(dead_code)] // host port; remove when the panel wires the view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ObjectBrowserAction {
    SetKind(BrowserKind),
    Load { kind: BrowserKind },
    Refresh { kind: BrowserKind },
    OpenObject { object: BrowserObject },
    CopyName { name: String },
    CopyDdl { object: BrowserObject },
    Execute { sql: String, schema: Option<String> },
    FormatDdl,
    ToggleComment,
    ListContextMenu { object: BrowserObject, press: PointerPress },
    EditorContextMenu { press: PointerPress },
    CloseMenu,
}

#[allow(dead_code)] // host port; remove when the panel wires the view.
pub(crate) type ObjectBrowserActionFn<S> = Rc<dyn Fn(&mut S, ObjectBrowserAction) + 'static>;

fn emit<S: 'static>(
    on_action: &Option<ObjectBrowserActionFn<S>>,
    action: ObjectBrowserAction,
) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Render the browser into `layer`, including either context menu when open.
#[allow(dead_code)] // view entry; remove when the panel wires it.
pub(crate) fn build<S: 'static>(
    vm: &ObjectBrowserVm,
    on_action: &Option<ObjectBrowserActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    let mut root = row()
        .key(ROOT_KEY)
        .w_full()
        .h_full()
        .min_h(0.0)
        .min_w(0.0);

    // ── Left column: kind tabs, count/refresh and the object list ─────
    let mut tabs = row()
        .key("object-browser-tabs")
        .w_full()
        .gap(4.0)
        .center_y()
        .px(8.0)
        .py(8.0);
    for kind in BrowserKind::ALL {
        let active = kind == vm.kind;
        let mut tab = row()
            .key(format!("{TAB_PREFIX}{}", kind.as_str()))
            .role("button")
            .label(kind.label().to_owned())
            .focusable(true)
            .h(24.0)
            .grow(1.0)
            .min_w(0.0)
            .center()
            .radius(4.0)
            .fill(if active {
                k.alpha(k.c.text_accent, 0.15)
            } else {
                rune::Color::TRANSPARENT
            })
            .child(
                text(kind.label())
                    .font(k.ty.text_xsmall)
                    .color(if active { k.c.text_accent } else { k.c.text_muted }),
            );
        tab = tab.on_press(emit(on_action, ObjectBrowserAction::SetKind(kind)));
        tabs = tabs.child(tab);
    }

    let refresh = Button::new(String::new())
        .icon(&ICON_REFRESH)
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Small)
        .build(k)
        .key("object-browser-refresh")
        .on_press(emit(
            on_action,
            ObjectBrowserAction::Refresh { kind: vm.kind },
        ));
    let count_row = row()
        .key("object-browser-count-row")
        .w_full()
        .gap(8.0)
        .center_y()
        .justify_between()
        .px(8.0)
        .py(4.0)
        .child(
            text(vm.objects.len().to_string())
                .key("object-browser-count")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        )
        .child(refresh);

    let mut list = col()
        .key(LIST_KEY)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .clip_soft(0.0)
        .scroll(rune::ScrollSpec { stick_bottom: false });
    if vm.loading {
        list = list.child(
            row()
                .key("object-browser-loading")
                .w_full()
                .gap(8.0)
                .center_y()
                .px(12.0)
                .py(8.0)
                .child(Spinner::new("object-browser-spinner").size(SpinnerSize::Md).build(dark))
                .child(
                    text(t("common.loading"))
                        .key("object-browser-loading-text")
                        .font(k.ty.text_small)
                        .color(k.c.text_muted),
                ),
        );
    }
    if !vm.error.is_empty() {
        list = list.child(
            CopyableError::new("object-browser-error", vm.error.clone())
                .build::<S>(k)
                .px(12.0)
                .py(8.0),
        );
    }
    if !vm.loading && vm.objects.is_empty() && vm.error.is_empty() {
        list = list.child(
            row()
                .key("object-browser-empty")
                .w_full()
                .px(12.0)
                .py(12.0)
                .child(
                    text(t("objects.empty"))
                        .key("object-browser-empty-text")
                        .font(k.ty.text_small)
                        .color(k.c.text_muted),
                ),
        );
    }
    for (at, object) in vm.objects.iter().enumerate() {
        list = list.child(object_row(object, at, vm.selected.as_ref(), on_action, dark));
    }

    root = root.child(
        col()
            .key("object-browser-aside")
            .w(256.0)
            .shrink()
            .min_h(0.0)
            .fill(k.c.layer_02)
            .child(tabs)
            .child(count_row)
            .child(list),
    );

    // ── Right column: selection, execute, DDL and the run message ─────
    let mut execute = Button::new(t("query.execute"))
        .icon(&ICON_PLAY)
        .variant(ButtonVariant::Contrast)
        .control_size(ControlSize::Small)
        .disabled(vm.execute_disabled())
        .build(k)
        .key("object-browser-execute");
    if !vm.execute_disabled() {
        let callback = on_action.clone();
        let sql = vm.ddl.text.clone();
        let schema = vm.selected.as_ref().and_then(|object| object.schema.clone());
        execute = execute.on_press(move |s: &mut S| {
            if let Some(callback) = &callback {
                callback(
                    s,
                    ObjectBrowserAction::Execute {
                        sql: sql.clone(),
                        schema: schema.clone(),
                    },
                );
            }
        });
    }
    let mut editor = col()
        .key(EDITOR_KEY)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0);
    let mut ddl_layer = Vec::new();
    crate::views::ddl_view::build_snapshot::<S>(&vm.ddl.text, None, dark, &mut ddl_layer);
    for element in ddl_layer {
        editor = editor.child(element);
    }
    if let Some(on_action) = on_action.clone() {
        editor = editor.on_secondary_press(move |s, press, _tx| {
            on_action(s, ObjectBrowserAction::EditorContextMenu { press });
        });
    }
    let mut detail = col()
        .key("object-browser-detail")
        .grow(1.0)
        .min_w(0.0)
        .min_h(0.0)
        .child(
            row()
                .key("object-browser-detail-header")
                .w_full()
                .h(36.0)
                .min_w(0.0)
                .center_y()
                .gap(8.0)
                .px(12.0)
                .child(
                    text(vm.selection_label())
                        .key("object-browser-selection")
                        .font(k.ty.text_small)
                        .color(k.c.text_muted)
                        .truncate()
                        .min_w(0.0),
                )
                .child(row().grow(1.0).min_w(0.0))
                .child(execute),
        )
        .child(editor);
    if !vm.run_message.is_empty() {
        detail = detail.child(
            row()
                .key("object-browser-message")
                .w_full()
                .px(12.0)
                .py(4.0)
                .child(
                    text(vm.run_message.clone())
                        .key("object-browser-message-text")
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted)
                        .wrap_anywhere(),
                ),
        );
    }
    root = root.child(detail);
    layer.push(root);

    if let (Some(press), Some(object)) = (vm.list_menu, vm.menu_object.as_ref()) {
        menus::build_list_menu(object, press, on_action, dark, layer);
    }
    if let Some(press) = vm.editor_menu {
        menus::build_editor_menu(vm, press, on_action, dark, layer);
    }
}

fn object_row<S: 'static>(
    object: &BrowserObject,
    at: usize,
    selected: Option<&BrowserObject>,
    on_action: &Option<ObjectBrowserActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let active = same_object_identity(selected, object);
    let mut row_el = col()
        .key(format!("{ITEM_KEY}-{at}"))
        .role("button")
        .focusable(true)
        .w_full()
        .min_w(0.0)
        .gap(2.0)
        .px(12.0)
        .py(6.0)
        .fill(if active {
            k.c.surface_raised
        } else {
            rune::Color::TRANSPARENT
        })
        .hover(k.c.surface_raised)
        .child(
            text(&object.name)
                .key(format!("{ITEM_KEY}-name-{at}"))
                .font(k.ty.text_small)
                .color(if active { k.c.text_base } else { k.c.text_weak })
                .truncate()
                .w_full()
                .min_w(0.0),
        );
    if let Some(schema) = object.schema.as_deref() {
        row_el = row_el.child(
            text(schema.to_owned())
                .key(format!("{ITEM_KEY}-schema-{at}"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );
    }
    if at == 0 {
        // Keep React's exact `data-testid` observable once.
        row_el = row_el.child(el_marker());
    }
    if let Some(on_action) = on_action.clone() {
        let open_callback = on_action.clone();
        let open = object.clone();
        row_el = row_el.on_press(move |s: &mut S| {
            open_callback(s, ObjectBrowserAction::OpenObject { object: open.clone() });
        });
        let context = object.clone();
        row_el = row_el.on_secondary_press(move |s, press, _tx| {
            on_action(
                s,
                ObjectBrowserAction::ListContextMenu {
                    object: context.clone(),
                    press,
                },
            );
        });
    }
    row_el
}

fn el_marker() -> El {
    rune::row().key(ITEM_KEY).size(0.0, 0.0).a11y_hidden()
}
