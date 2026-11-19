//! Database object detail view, ported from
//! `packages/frontend/src/windows/connection/DatabaseObjectView.tsx`.
//!
//! React loads the object's DDL through `databaseCommands.getObjectDdl`, shows
//! it in a `SqlEditor`, and can execute it through `queryCommands.executeQuery`.
//! The Rune port keeps the same two-step flow: presses emit [`ObjectAction`]s,
//! the host executes them and feeds [`ObjectEvent`]s back — no fake IPC.
//!
//! ## Local stand-ins (removal stage)
//!
//! The full `SqlEditor` engine is a separate port; this view renders the DDL as
//! a selectable keyed block (`db-object-ddl`). Remove when that editor is
//! composed here. The object identity fields mirror React's props because the
//! host has no panel/object-store slice yet; remove when that slice lands.
//!
//! ## Keys
//!
//! `db-object-view` (root), `-header`, `-title`, `-refresh`, `-execute`,
//! `-loading`, `-editor`, `-ddl`, `-message`.

use std::rc::Rc;

use rune::{col, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::icons::{ICON_PLAY, ICON_REFRESH};
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize};

/// React `DatabaseObjectKind` (from `@datazen/extension-points`).
#[allow(dead_code)] // contract mirror; remove when the extension-points port lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ObjectKind {
    Function,
    Procedure,
    Trigger,
    Sequence,
    Type,
}

impl ObjectKind {
    /// React's kind label: three kinds are translated, the rest use the raw id.
    pub(crate) fn label(self) -> String {
        match self {
            Self::Function => t("objects.function").to_owned(),
            Self::Procedure => t("objects.procedure").to_owned(),
            Self::Trigger => t("objects.trigger").to_owned(),
            Self::Sequence => "sequence".to_owned(),
            Self::Type => "type".to_owned(),
        }
    }
}

/// The object identity the host supplies (React props).
#[allow(dead_code)] // VM surface; owned by the host until the panel store lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ObjectIdentity {
    pub db_session_id: String,
    pub database_type: Option<String>,
    pub database: String,
    pub kind: ObjectKind,
    pub name: String,
    pub object_schema: Option<String>,
    pub object_signature: Option<String>,
    pub object_target_schema: Option<String>,
    pub object_target_name: Option<String>,
}

/// The three `useState` values plus the loaded DDL.
#[allow(dead_code)] // VM surface; owned by the host until the panel store lands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ObjectViewState {
    pub ddl: String,
    pub loading: bool,
    pub running: bool,
    pub message: String,
}

/// Host calls the view emits. The host owns commands/session ids.
#[allow(dead_code)] // host port; remove when the panel-store view track wires it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ObjectAction {
    LoadDdl,
    Execute { sql: String, database: String, schema: Option<String> },
}

/// Host answers fed back into the view state.
#[allow(dead_code)] // host port; remove when the panel-store view track wires it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ObjectEvent {
    DdlLoaded(Result<String, String>),
    Executed(Result<(), String>),
}

impl ObjectViewState {
    /// React `loadDdl` prologue: clear the prior message and show the spinner.
    #[allow(dead_code)] // consumer: the object-detail host result feed (not landed); remove when wired.
    pub(crate) fn begin_load(&mut self) {
        self.loading = true;
        self.message.clear();
    }

    /// React's DDL answer: a failure is shown as a SQL comment, not an error bar.
    #[allow(dead_code)] // consumer: the object-detail host result feed (not landed); remove when wired.
    pub(crate) fn apply_ddl(&mut self, result: Result<String, String>) {
        self.loading = false;
        match result {
            Ok(text) => self.ddl = text,
            Err(error) => self.ddl = format!("-- {error}"),
        }
    }

    /// React `handleExecute` guard + payload readiness.
    #[allow(dead_code)] // consumer: the object-detail host (not landed); remove when wired.
    pub(crate) fn begin_execute(&mut self) -> bool {
        if self.ddl.trim().is_empty() || self.loading || self.running {
            return false;
        }
        self.running = true;
        self.message.clear();
        true
    }

    /// React's execute answer: success shows the localized `objects.executeOk`.
    #[allow(dead_code)] // consumer: the object-detail host result feed (not landed); remove when wired.
    pub(crate) fn apply_execute(&mut self, result: Result<(), String>) {
        self.running = false;
        self.message = match result {
            Ok(()) => t("objects.executeOk").to_owned(),
            Err(error) => error,
        };
    }

    /// React `disabled={running || !ddl.trim() || loading}`.
    pub(crate) fn execute_disabled(&self) -> bool {
        self.running || self.ddl.trim().is_empty() || self.loading
    }
}

/// The snapshot a host supplies when an object is selected; carried by
/// `ContentViewVm` so `connection_page` can compose this view.
#[allow(dead_code)] // composition VM; remove when the panel/object store lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DatabaseObjectVm {
    pub identity: ObjectIdentity,
    pub state: ObjectViewState,
}

type ActionFn<S> = Rc<dyn Fn(&mut S, ObjectAction) + 'static>;

fn emit<S: 'static>(on_action: &Option<ActionFn<S>>, action: ObjectAction) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Render the object detail view into `layer`.
#[allow(dead_code)] // consumer: connection_page composition (wired by T305).
pub(crate) fn build<S: 'static>(
    identity: &ObjectIdentity,
    state: &ObjectViewState,
    on_action: Option<ActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    let mut root = col().key("db-object-view").w_full().h_full().min_w(0.0).min_h(0.0).gap(0.0);

    // Header: `kindLabel · schema.name`, then refresh and run actions.
    let title = match &identity.object_schema {
        Some(schema) if !schema.is_empty() => {
            format!("{} · {}.{}", identity.kind.label(), schema, identity.name)
        }
        _ => format!("{} · {}", identity.kind.label(), identity.name),
    };
    let refresh = Button::new("")
        .icon(&ICON_REFRESH)
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Small)
        .build(k)
        .key("db-object-refresh")
        .on_press(emit(&on_action, ObjectAction::LoadDdl));

    let execute = Button::new(t("query.execute"))
        .icon(&ICON_PLAY)
        .variant(ButtonVariant::Contrast)
        .control_size(ControlSize::Small)
        .disabled(state.execute_disabled())
        .build(k)
        .key("db-object-execute")
        .on_press({
            let callback = on_action.clone();
            let sql = state.ddl.clone();
            let database = identity.database.clone();
            let schema = identity.object_schema.clone();
            move |s: &mut S| {
                if let Some(callback) = &callback {
                    callback(
                        s,
                        ObjectAction::Execute { sql: sql.clone(), database: database.clone(), schema: schema.clone() },
                    );
                }
            }
        });
    root = root.child(
        row()
            .key("db-object-header")
            .w_full()
            .h(36.0)
            .min_w(0.0)
            .center_y()
            .gap(8.0)
            .child(text(title).key("db-object-title").wrap())
            .child(row().grow(1.0).min_w(0.0))
            .child(refresh)
            .child(execute),
    );

    if state.loading {
        root = root.child(
            row()
                .key("db-object-loading")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .center()
                .child(
                    Spinner::new("db-object-spinner")
                        .size(SpinnerSize::Lg)
                        .build(dark),
                ),
        );
    } else {
        // T322: the DDL text block is the real DDLView (read-only code block,
        // copy control and header). The `db-object-ddl` root key is preserved.
        let mut ddl_layer = Vec::new();
        crate::views::ddl_view::build_snapshot::<S>(
            &state.ddl,
            None::<std::rc::Rc<dyn Fn(&mut S, &mut rune::Tx)>>,
            dark,
            &mut ddl_layer,
        );
        let mut editor = col()
            .key("db-object-editor")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .min_w(0.0);
        for element in ddl_layer {
            editor = editor.child(element);
        }
        root = root.child(editor);
    }
    if !state.message.is_empty() {
        root = root.child(
            row()
                .key("db-object-message")
                .w_full()
                .min_w(0.0)
                .child(
                    text(state.message.clone())
                        .key("db-object-message-text")
                        .w_full()
                        .min_w(0.0)
                        .wrap_anywhere(),
                ),
        );
    }
    layer.push(root);
}
