//! DDL detail view, ported from
//! `packages/frontend/src/windows/connection/DDLView.tsx`.
//!
//! React loads a relation's DDL with `fetchRelationDdl`, shows a spinner while
//! loading, a `CopyableError` on failure, and a read-only `SqlCodeBlock` with a
//! copy button otherwise. The Rune port keeps that three-way split. Async work
//! is an injected [`DdlAction`] the host executes; copying uses the host copy
//! port (`Tx::copy`) with a real 2000ms feedback wake — no fake IPC, no threads.
//!
//! ## Local stand-ins (removal stage)
//!
//! `SqlCodeBlock` is a CodeMirror engine port with no Rune counterpart; the DDL
//! is rendered as a selectable keyed block. Remove when that engine lands.
//! `DIdentity` mirrors React props until the host panel store owns them.
//!
//! ## Keys
//!
//! `ddl-view` (root; `db-object-ddl` when composed into T305's editor slot),
//! `-loading`, `-loading-text`, `-error`, `-header`, `-title`, `-copy`,
//! `-code`, `-ddl`.

use std::rc::Rc;
use std::time::Duration;

use rune::{col, row, text, El, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::icons::{ICON_CHECK, ICON_COPY};
use crate::ui::copyable::CopyableError;
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize};

/// React's copy feedback lifetime: `setTimeout(..., 2000)`.
const COPIED_MS: u64 = 2000;

/// The identity React passes to `fetchRelationDdl`.
#[allow(dead_code)] // contract mirror; remove when the host panel store lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DdlIdentity {
    pub table_name: String,
    pub database: String,
    pub database_type: Option<String>,
    pub is_view: bool,
}

/// The four `useState` values; `copied_until` is the host-owned deadline.
#[allow(dead_code)] // VM surface; owned by the host until the panel store lands.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DdlViewState {
    pub ddl: String,
    pub loading: bool,
    pub error: String,
    pub copied_until: Option<f64>,
}

/// Host calls the view emits.
#[allow(dead_code)] // host port; remove when the panel-store view track wires it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DdlAction {
    Load,
    Copy,
}

impl DdlViewState {
    /// React's effect prologue: clear both prior values and start the spinner.
    #[allow(dead_code)] // consumer: the DDL host (not landed); remove when wired.
    pub fn begin_load(&mut self) {
        self.loading = true;
        self.error.clear();
        self.ddl.clear();
    }

    /// React's answer: `if (!result)` is false for any string, including an
    /// empty or whitespace-only one, so every `Some(text)` is the DDL. Only a
    /// missing result (`None`) or a rejection is a failure. Both clear the spinner.
    #[allow(dead_code)] // consumer: the DDL host result feed (not landed); remove when wired.
    pub fn apply_load(&mut self, result: Result<Option<String>, String>) {
        self.loading = false;
        match result {
            Ok(Some(text)) => {
                self.ddl = text;
                self.error.clear();
            }
            Ok(_) => {
                self.ddl.clear();
                self.error = t("ddl.loadFailed").to_owned();
            }
            Err(message) => {
                self.ddl.clear();
                self.error = if message.is_empty() { t("ddl.loadFailed").to_owned() } else { message };
            }
        }
    }

    /// React `copied` is true for 2000ms after a successful copy.
    pub fn copied(&self, now: f64) -> bool {
        self.copied_until.is_some_and(|until| now < until)
    }
}

type ActionFn<S> = Rc<dyn Fn(&mut S, DdlAction) + 'static>;
type CopyFn<S> = Rc<dyn Fn(&mut S, &mut Tx) + 'static>;

/// Render the DDL view into `layer`. `root_key` is `ddl-view` for the standalone
/// view and `db-object-ddl` when composed into T305's editor slot.
#[allow(dead_code)] // consumer: connection_page / database_object composition.
pub fn build<S: 'static>(
    identity: &DdlIdentity,
    state: &DdlViewState,
    root_key: &str,
    now: f64,
    on_action: Option<ActionFn<S>>,
    on_copy: Option<CopyFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    if state.loading {
        layer.push(
            row()
                .key(format!("{root_key}-loading"))
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .center()
                .gap(8.0)
                .child(Spinner::new(format!("{root_key}-spinner")).size(SpinnerSize::Lg).build(dark))
                .child(
                    text(t("ddl.generating"))
                        .key(format!("{root_key}-loading-text"))
                        .wrap(),
                ),
        );
        return;
    }
    if !state.error.is_empty() {
        layer.push(
            row()
                .key(format!("{root_key}-error"))
                .w_full()
                .min_w(0.0)
                .center()
                .child(CopyableError::new(format!("{root_key}-error-text"), state.error.clone()).build::<S>(k)),
        );
        return;
    }

    let copied = state.copied(now);
    let mut root = col().key(root_key.to_owned()).w_full().h_full().min_w(0.0).min_h(0.0);

    let title = format!("{} · DDL", identity.table_name);
    let mut copy = Button::new(if copied { t("common.copied") } else { t("common.copy") })
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Small)
        .icon(if copied { &ICON_CHECK } else { &ICON_COPY })
        .build(k)
        .key(format!("{root_key}-copy"));
    if !state.ddl.is_empty() {
        let ddl = state.ddl.clone();
        let on_action = on_action.clone();
        let on_copy = on_copy.clone();
        copy = copy.on_press_tx(move |s: &mut S, tx: &mut Tx| {
            tx.copy(ddl.clone());
            // Real timed wake for the 2000ms feedback revert.
            tx.wake_after(Duration::from_millis(COPIED_MS));
            if let Some(handler) = &on_copy {
                handler(s, tx);
            }
            if let Some(handler) = &on_action {
                handler(s, DdlAction::Copy);
            }
        });
    }
    root = root.child(
        row()
            .key(format!("{root_key}-header"))
            .w_full()
            .min_w(0.0)
            .gap(8.0)
            .center_y()
            .child(text(title).key(format!("{root_key}-title")).wrap())
            .child(row().grow(1.0).min_w(0.0))
            .child(copy),
    );
    root = root.child(
        col()
            .key(format!("{root_key}-code"))
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .min_w(0.0)
            .child(
                text(state.ddl.clone())
                    .key(format!("{root_key}-ddl"))
                    .font(crate::sql_editor::editor_font())
                    .color(k.c.text_base)
                    .selectable(true)
                    .w_full()
                    .min_w(0.0)
                    .wrap_anywhere(),
            ),
    );
    layer.push(root);
}

/// Snapshot entry point for T305's `db-object-ddl` block: its own state machine
/// already loaded the DDL, so only display/copy are wired here.
#[allow(dead_code)] // consumer: database_object_view composition; remove when the SqlCodeBlock port lands.
pub fn build_snapshot<S: 'static>(
    ddl: &str,
    on_copy: Option<CopyFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let identity = DdlIdentity {
        table_name: String::new(),
        database: String::new(),
        database_type: None,
        is_view: false,
    };
    let state = DdlViewState {
        ddl: ddl.to_owned(),
        ..Default::default()
    };
    build::<S>(&identity, &state, "db-object-ddl", 0.0, None, on_copy, dark, layer);
}
