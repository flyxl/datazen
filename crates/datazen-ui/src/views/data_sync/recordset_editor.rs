//! T360: `windows/data-sync/RecordsetEditor.tsx` ported to Rune.
//!
//! The Data Sync wizard's row-range editor: the toggle that attaches or
//! drops a recordset, the single-key editor (order column, start/end
//! bounds with inclusive flags, a limit with its own validation error),
//! and the composite-key tuple editor. The wire type is the transfer
//! window's [`TransferRecordset`] — the `DataSyncRecordset` of
//! `backend-client/types/dataSync.ts` is the same shape (`orderBy` /
//! `start` / `end` / `tupleRange` / `limit`), so one Rust mirror serves
//! both windows and the fields stay 1:1.
//!
//! ## The state model (React → Rune)
//!
//! React keeps the value inputs controlled (`value={bound?.value ?? ''}`,
//! the whole `onChange` chain per keystroke) and one `useState` for the
//! limit error. Rune has no per-keystroke re-entry, so the buffers live
//! in the host ([`RecordsetHost`]: the two bound texts, the per-key tuple
//! texts, the limit text, the limit-error flag) and commit on Enter —
//! the same shape the transfer recordset editor (T023-S3) documents for
//! its inputs. The recordset itself is the host's controlled model.
//!
//! ## The pure update chain (verbatim from React)
//!
//! * `update(patch)` merges into `{ ...(recordset ?? {}) }`: a `None`
//!   recordset is an empty base, so the first edit on a fresh recordset
//!   keeps the other fields absent.
//! * An emptied bound value **deletes the bound** (React: `delete
//!   next[name]`), it never stores an empty string.
//! * An emptied tuple component deletes the whole tuple bound only when
//!   every component is empty (`values.every(c => c === '')`); otherwise
//!   the values array is padded to the key count with `''` for the
//!   components nobody typed yet.
//! * Choosing any tuple bound clears the scalar trio (`orderBy: undefined,
//!   start: undefined, end: undefined`) — a range is either scalar or
//!   tuple, never a mix.
//! * The limit accepts a **whole, safe, ≥ 1 integer** only; anything
//!   else keeps the model and raises the limit error flag
//!   (`/^\d+$/`, `Number.isSafeInteger`, `parsed < 1`), and an empty
//!   text clears both the limit and the error.
//! * The toggle drops the recordset entirely, or creates the right empty
//!   one: a tuple range over the composite key, or `orderBy` on the
//!   single key (only when it exists — zero keys disable the toggle).
//!
//! ## Naming
//!
//! React also has a transfer-window `RecordsetEditor` (T023-S3, in
//! `ColumnMappingEditor.tsx`); the two live in different files there but
//! share one name. Rust has one crate namespace, so this one is
//! `SyncRecordsetEditor` and the transfer one keeps `RecordsetEditor` —
//! the `data-sync-` key prefix stays the component's own.
//!
//! ## Keys
//!
//! React `data-testid` becomes the element key, verbatim; the rest are
//! internal (`-header`, `-hint`, `-label`, `-box`, `-text` suffixes):
//!
//! | part | key |
//! |---|---|
//! | root (React test id) | `data-sync-recordset` |
//! | header row (internal) | `…-header` |
//! | toggle button (React test id) | `data-sync-recordset-toggle` |
//! | editor body (React test id) | `data-sync-recordset-editor` |
//! | tuple editor (React test id) | `data-sync-recordset-tuple-editor` |
//! | tuple order line (internal) | `…-tuple-order` |
//! | tuple bound box (internal) | `…-tuple-{start,end}-box` |
//! | tuple component input (React test id) | `data-sync-recordset-{start,end}-<index>` |
//! | tuple inclusive (React test id) | `…-{start,end}-inclusive` |
//! | order select trigger (React test id) | `data-sync-recordset-order-trigger` |
//! | single bound input (React test id) | `data-sync-recordset-{start,end}` |
//! | single inclusive (React test id) | `…-{start,end}-inclusive` |
//! | limit input (React test id) | `data-sync-recordset-limit` |
//! | limit error (React test id) | `data-sync-recordset-limit-error` |
//!
//! ## Documented deviations from React
//!
//! * **Text commits on submit, not per keystroke** (the T023-S3 shape):
//!   the buffers are the host's `TextEdit`s, Enter commits them through
//!   the same chain every other control uses.
//! * **`inclusive` is a plain `bool`** (default `true`), which is React's
//!   `inclusive?: boolean` with its `!== false` reads.
//! * **The order select closes on choose**, the kit way (see T023-S3's
//!   same note): the builder closes it in `on_choose` and the host
//!   writes the flag back.

// The production consumer is the Data Sync MappingPanel (T336); until it
// wires this in, the component tests are the only callers, so the
// dead-code lint is silenced per module rather than crate-wide.
#![allow(dead_code)] // reusable component; wired by the Data Sync mapping panel (T336)

use std::rc::Rc;

use rune::{col, row, text, El, KeyEvent, TextEdit, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::select::Select;
use crate::ui::toggle::Checkbox;
use crate::views::{
    TransferRecordset, TransferRecordsetBound, TransferRecordsetTupleBound,
    TransferRecordsetTupleRange,
};

/// Which end of a range a patch addresses.
pub(crate) use crate::views::BoundName;
pub(crate) use crate::views::RecordsetHost;

/// `defaultOrder`: the single primary key, or `''` for any other count.
pub(crate) fn default_order(primary_keys: &[String]) -> String {
    match primary_keys {
        [only] => only.clone(),
        _ => String::new(),
    }
}

/// React `tupleMode`: a tuple range exists, or a composite key sits on a
/// recordset with none of the scalar fields set.
pub(crate) fn tuple_mode(primary_keys: &[String], recordset: Option<&TransferRecordset>) -> bool {
    let Some(recordset) = recordset else {
        return false;
    };
    recordset.tuple_range.is_some()
        || (primary_keys.len() > 1
            && recordset.order_by.is_none()
            && recordset.start.is_none()
            && recordset.end.is_none())
}

/// React `update(patch)` over an absent base: `None` starts as an empty
/// recordset, existing fields stay.
fn with_base(recordset: Option<&TransferRecordset>) -> TransferRecordset {
    recordset.cloned().unwrap_or_default()
}

/// The toggle: drop the recordset, or create the right empty one.
pub(crate) fn toggle_recordset(
    primary_keys: &[String],
    recordset: Option<&TransferRecordset>,
) -> Option<TransferRecordset> {
    if recordset.is_some() {
        return None;
    }
    if primary_keys.len() > 1 {
        Some(TransferRecordset {
            tuple_range: Some(TransferRecordsetTupleRange {
                columns: primary_keys.to_vec(),
                start: None,
                end: None,
            }),
            ..Default::default()
        })
    } else {
        let order = default_order(primary_keys);
        Some(TransferRecordset {
            order_by: (!order.is_empty()).then_some(order),
            ..Default::default()
        })
    }
}

/// The single-bound value edit: an emptied value deletes the bound,
/// otherwise the bound is created (`{ value: '', inclusive: true }` base)
/// and its value replaced.
pub(crate) fn set_bound_value(
    recordset: Option<&TransferRecordset>,
    name: BoundName,
    value: &str,
) -> TransferRecordset {
    let mut next = with_base(recordset);
    if value.is_empty() {
        match name {
            BoundName::Start => next.start = None,
            BoundName::End => next.end = None,
        }
    } else {
        // `{ ...(current ?? { value: '', inclusive: true }), value }`:
        // an existing bound keeps its inclusive flag.
        let inclusive = match name {
            BoundName::Start => next.start.as_ref().map(|b| b.inclusive).unwrap_or(true),
            BoundName::End => next.end.as_ref().map(|b| b.inclusive).unwrap_or(true),
        };
        let bound = TransferRecordsetBound {
            value: value.to_owned(),
            inclusive,
        };
        match name {
            BoundName::Start => next.start = Some(bound),
            BoundName::End => next.end = Some(bound),
        }
    }
    next
}

/// The single-bound inclusive flag edit (a bound exists by construction:
/// the checkbox only renders while it does).
pub(crate) fn set_bound_inclusive(
    recordset: Option<&TransferRecordset>,
    name: BoundName,
    inclusive: bool,
) -> TransferRecordset {
    let mut next = with_base(recordset);
    let slot = match name {
        BoundName::Start => &mut next.start,
        BoundName::End => &mut next.end,
    };
    *slot = Some(TransferRecordsetBound {
        value: slot.as_ref().map(|b| b.value.clone()).unwrap_or_default(),
        inclusive,
    });
    next
}

/// The order-by edit: `value || undefined` (an empty choice clears it).
pub(crate) fn set_order_by(
    recordset: Option<&TransferRecordset>,
    order: &str,
) -> TransferRecordset {
    let mut next = with_base(recordset);
    next.order_by = (!order.is_empty()).then(|| order.to_owned());
    next
}

/// The base tuple range for an edit: the existing one, or one over the
/// current primary keys.
fn tuple_base(
    primary_keys: &[String],
    recordset: Option<&TransferRecordset>,
) -> TransferRecordsetTupleRange {
    recordset
        .and_then(|recordset| recordset.tuple_range.clone())
        .unwrap_or_else(|| TransferRecordsetTupleRange {
            columns: primary_keys.to_vec(),
            start: None,
            end: None,
        })
}

/// React `updateTupleBound`: set one tuple bound's patch and clear the
/// scalar trio — a range is either scalar or tuple.
pub(crate) fn set_tuple_bound(
    primary_keys: &[String],
    recordset: Option<&TransferRecordset>,
    name: BoundName,
    bound: TransferRecordsetTupleBound,
) -> TransferRecordset {
    let mut next = with_base(recordset);
    let mut range = tuple_base(primary_keys, recordset);
    match name {
        BoundName::Start => range.start = Some(bound),
        BoundName::End => range.end = Some(bound),
    }
    next.tuple_range = Some(range);
    next.order_by = None;
    next.start = None;
    next.end = None;
    next
}

/// React `setTupleComponent`: one typed component of one tuple bound. The
/// values array is padded to the key count; all-empty deletes the bound.
pub(crate) fn set_tuple_component(
    primary_keys: &[String],
    recordset: Option<&TransferRecordset>,
    name: BoundName,
    index: usize,
    value: &str,
) -> TransferRecordset {
    let mut next = with_base(recordset);
    let mut range = tuple_base(primary_keys, recordset);
    let current = match name {
        BoundName::Start => range.start.as_ref(),
        BoundName::End => range.end.as_ref(),
    };
    let values: Vec<String> = (0..primary_keys.len())
        .map(|component| {
            if component == index {
                value.to_owned()
            } else {
                current
                    .and_then(|bound| bound.values.get(component))
                    .cloned()
                    .unwrap_or_default()
            }
        })
        .collect();
    if values.iter().all(|component| component.is_empty()) {
        // `delete next[name]`: the bound goes, the range stays.
        match name {
            BoundName::Start => range.start = None,
            BoundName::End => range.end = None,
        }
        next.tuple_range = Some(range);
    } else {
        let bound = TransferRecordsetTupleBound {
            values,
            inclusive: current.map(|bound| bound.inclusive).unwrap_or(true),
        };
        match name {
            BoundName::Start => range.start = Some(bound),
            BoundName::End => range.end = Some(bound),
        }
        next.tuple_range = Some(range);
        next.order_by = None;
        next.start = None;
        next.end = None;
    }
    next
}

/// React `setLimit`: the model change plus the error flag. An empty text
/// clears the limit and the error; a whole safe integer ≥ 1 sets it; the
/// rest keeps the model and raises the error.
pub(crate) fn set_limit(
    recordset: Option<&TransferRecordset>,
    value: &str,
) -> (TransferRecordset, bool) {
    let mut next = with_base(recordset);
    if value.is_empty() {
        next.limit = None;
        return (next, false);
    }
    // JS gates: `/^\d+$/`, `Number.isSafeInteger`, `parsed >= 1`. A safe
    // integer is at most `2^53 - 1` (9_007_199_254_740_991).
    let trimmed = value.trim();
    const MAX_SAFE: u64 = 9_007_199_254_740_991;
    let valid = !trimmed.is_empty()
        && trimmed.bytes().all(|b| b.is_ascii_digit())
        && trimmed
            .parse::<u64>()
            .map(|v| (1..=MAX_SAFE).contains(&v))
            .unwrap_or(false);
    if valid {
        let parsed = trimmed.parse::<f64>().unwrap_or_default();
        next.limit = Some(parsed);
        (next, false)
    } else {
        // The model keeps its previous limit (React never calls `update`).
        (with_base(recordset), true)
    }
}

/// The controlled view model of the editor.
#[derive(Clone, Debug, Default)]
pub(crate) struct SyncRecordsetEditorVm {
    pub primary_keys: Vec<String>,
    pub recordset: Option<TransferRecordset>,
    /// The kit `Select` open flag the host owns.
    pub order_select_open: bool,
    /// React's `limitError` `useState`, owned by the host here: the flag
    /// the limit validation raises and the error paragraph renders.
    pub limit_error: bool,
}

/// The editor builder. `S` is the host state that owns the recordset, the
/// input buffers ([`RecordsetHost`]), the limit-error flag, and the order
/// select's open flag.
pub(crate) struct SyncRecordsetEditor<S: 'static> {
    vm: SyncRecordsetEditorVm,
    on_change: Option<Rc<dyn Fn(&mut S, Option<TransferRecordset>)>>,
    on_toggle_order: Option<Rc<dyn Fn(&mut S)>>,
    on_close_order: Option<Rc<dyn Fn(&mut S)>>,
    bind_start: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>>,
    bind_end: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>>,
    bind_limit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>>,
    bind_tuple: Option<Rc<dyn Fn(&mut S, BoundName, usize) -> &mut TextEdit>>,
}

impl<S: 'static> SyncRecordsetEditor<S> {
    pub(crate) fn new(vm: SyncRecordsetEditorVm) -> Self {
        Self {
            vm,
            on_change: None,
            on_toggle_order: None,
            on_close_order: None,
            bind_start: None,
            bind_end: None,
            bind_limit: None,
            bind_tuple: None,
        }
    }

    /// React `onChange`.
    pub(crate) fn on_change(
        mut self,
        f: impl Fn(&mut S, Option<TransferRecordset>) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// The host records the order select opening.
    pub(crate) fn on_toggle_order(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_order = Some(Rc::new(f));
        self
    }

    /// The host closes the order select.
    pub(crate) fn on_close_order(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_order = Some(Rc::new(f));
        self
    }

    /// Where the start-bound input's buffer lives in the host.
    pub(crate) fn bind_start(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.bind_start = Some(Rc::new(f));
        self
    }

    /// Where the end-bound input's buffer lives in the host.
    pub(crate) fn bind_end(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.bind_end = Some(Rc::new(f));
        self
    }

    /// Where the limit input's buffer lives in the host.
    pub(crate) fn bind_limit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.bind_limit = Some(Rc::new(f));
        self
    }

    /// Where one tuple component's buffer lives in the host.
    pub(crate) fn bind_tuple(
        mut self,
        f: impl Fn(&mut S, BoundName, usize) -> &mut TextEdit + 'static,
    ) -> Self {
        self.bind_tuple = Some(Rc::new(f));
        self
    }

    /// The editor on its own kit root.
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let mut layer = Vec::new();
        let root = self.build_inner(dark, &k, &mut layer);
        rune_kit::root(root, layer)
    }

    /// The same editor without its kit root (for a parent that roots the
    /// overlay once).
    pub(crate) fn build_inner(self, dark: bool, k: &Kit, layer: &mut Vec<El>) -> El {
        let mut root = col()
            .key("data-sync-recordset")
            .w_full()
            .gap(8.0)
            .radius(k.radius.lg)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_weak)
            .padding(12.0)
            .child(self.header(k));
        if self.vm.recordset.is_some() {
            root = root.child(self.editor_body(dark, k, layer));
        }
        root
    }

    /// The header: the title + hint, and the toggle button.
    fn header(&self, k: &Kit) -> El {
        let recordset = self.vm.recordset.clone();
        let primary_keys = self.vm.primary_keys.clone();
        let mut toggle = Button::new(if recordset.is_some() {
            t("sync.recordsetDisable")
        } else {
            t("sync.recordsetEnable")
        })
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Small)
        .disabled(primary_keys.is_empty());
        if !primary_keys.is_empty() {
            if let Some(on_change) = &self.on_change {
                let on_change = on_change.clone();
                let next = toggle_recordset(&primary_keys, recordset.as_ref());
                toggle = toggle.on_press(move |s: &mut S| on_change(s, next.clone()));
            }
        }
        row()
            .key("data-sync-recordset-header")
            .w_full()
            .items_center()
            .justify_between()
            .gap(8.0)
            .child(
                col().key("data-sync-recordset-title").child(
                    text(t("sync.recordset"))
                        .font(k.ty.text_small)
                        .color(k.c.text_base),
                ).child(
                    text(t("sync.recordsetHint"))
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted)
                        .wrap(),
                ),
            )
            .child(toggle.build(k).key("data-sync-recordset-toggle"))
    }

    /// The editor body: the tuple editor or the scalar editor, plus the
    /// limit row.
    fn editor_body(&self, dark: bool, k: &Kit, layer: &mut Vec<El>) -> El {
        let mut body = col()
            .key("data-sync-recordset-editor")
            .w_full()
            .gap(8.0);
        if tuple_mode(&self.vm.primary_keys, self.vm.recordset.as_ref()) {
            body = body.child(self.tuple_editor(dark, k));
        } else {
            body = body.child(self.scalar_editor(dark, k, layer));
        }
        body = body.child(self.limit_row(dark, k));
        if self.vm.limit_error {
            body = body.child(
                text(limit_error_text())
                    .key("data-sync-recordset-limit-error")
                    .font(k.ty.text_xsmall)
                    .color(k.c.warning.fg),
            );
        }
        body
    }

    /// The tuple editor: the key chain line, then the start/end boxes.
    fn tuple_editor(&self, dark: bool, k: &Kit) -> El {
        let mut editor = col()
            .key("data-sync-recordset-tuple-editor")
            .w_full()
            .gap(8.0)
            .child(
                text(self.vm.primary_keys.join(" → "))
                    .key("data-sync-recordset-tuple-order")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            );
        for name in [BoundName::Start, BoundName::End] {
            editor = editor.child(self.tuple_bound_box(dark, k, name));
        }
        editor
    }

    /// One tuple bound box: a labelled input per key component, and the
    /// inclusive checkbox while the bound exists.
    fn tuple_bound_box(&self, dark: bool, k: &Kit, name: BoundName) -> El {
        let label = match name {
            BoundName::Start => t("sync.recordsetStart"),
            BoundName::End => t("sync.recordsetEnd"),
        };
        let bound = self.vm.recordset.as_ref().and_then(|r| {
            r.tuple_range.as_ref().and_then(|range| match name {
                BoundName::Start => range.start.as_ref(),
                BoundName::End => range.end.as_ref(),
            })
        });
        let mut box_el = col()
            .key(format!(
                "data-sync-recordset-tuple-{}-box",
                match name {
                    BoundName::Start => "start",
                    BoundName::End => "end",
                }
            ))
            .w_full()
            .gap(4.0)
            .border(1.0, k.c.border_base)
            .radius(k.radius.sm)
            .padding(8.0)
            .child(
                text(label)
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            );
        for (index, column) in self.vm.primary_keys.iter().enumerate() {
            box_el = box_el.child(
                row()
                    .key(format!(
                        "data-sync-recordset-{}-{}-label",
                        match name {
                            BoundName::Start => "start",
                            BoundName::End => "end",
                        },
                        index
                    ))
                    .items_center()
                    .gap(8.0)
                    .child(
                        text(column.clone())
                            .w(96.0)
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted),
                    )
                    .child(self.tuple_component_input(dark, k, name, index, bound)),
            );
        }
        if bound.is_some() {
            box_el = box_el.child(self.tuple_inclusive_row(k, name));
        }
        box_el
    }

    /// One tuple component input.
    fn tuple_component_input(
        &self,
        dark: bool,
        k: &Kit,
        name: BoundName,
        index: usize,
        bound: Option<&TransferRecordsetTupleBound>,
    ) -> El {
        let key = format!(
            "data-sync-recordset-{}-{index}",
            match name {
                BoundName::Start => "start",
                BoundName::End => "end",
            }
        );
        if let Some(bind) = &self.bind_tuple {
            let bind = bind.clone();
            let on_change = self.on_change.clone();
            let recordset = self.vm.recordset.clone();
            let primary_keys = self.vm.primary_keys.clone();
            // The initial text: this component's current value (the host's
            // buffer is the live one; a missing buffer renders the model's
            // value as the stand-in, mirroring React's controlled text).
            let placeholder = t("sync.recordsetUnbounded");
            let initial = bound
                .and_then(|bound| bound.values.get(index))
                .cloned()
                .unwrap_or_default();
            let field = TextField::new(key.clone(), &TextEdit::new(initial))
                .height(28.0)
                .min_width(144.0)
                .grow()
                .text_style(k.ty.text_xsmall)
                .placeholder(placeholder)
                .on_edit({
                    let bind = bind.clone();
                    move |s: &mut S| bind(s, name, index)
                })
                .on_submit(move |s: &mut S, _tx: &mut Tx| {
                    let value = bind(s, name, index).text.clone();
                    if let Some(on_change) = &on_change {
                        let next = set_tuple_component(&primary_keys, recordset.as_ref(), name, index, &value);
                        on_change(s, Some(next));
                    }
                })
                .build(dark);
            return field;
        }
        // Without a buffer binding the row still shows the model's value.
        TextField::new(key, &TextEdit::new(
            bound
                .and_then(|bound| bound.values.get(index))
                .cloned()
                .unwrap_or_default(),
        ))
        .height(28.0)
        .min_width(144.0)
        .grow()
        .text_style(k.ty.text_xsmall)
        .placeholder(t("sync.recordsetUnbounded"))
        .build(dark)
    }

    /// The tuple inclusive row (a checkbox while the bound exists):
    /// `updateTupleBound(name, { inclusive })` — the values stay, the
    /// scalar trio is cleared by the same call in React.
    fn tuple_inclusive_row(&self, k: &Kit, name: BoundName) -> El {
        let key = format!(
            "data-sync-recordset-{}-inclusive",
            match name {
                BoundName::Start => "start",
                BoundName::End => "end",
            }
        );
        let bound = self.vm.recordset.as_ref().and_then(|r| {
            r.tuple_range.as_ref().and_then(|range| match name {
                BoundName::Start => range.start.as_ref(),
                BoundName::End => range.end.as_ref(),
            })
        });
        let inclusive = bound.map(|bound| bound.inclusive).unwrap_or(true);
        let mut checkbox = Checkbox::new(key, inclusive);
        if let Some(on_change) = &self.on_change {
            let on_change = on_change.clone();
            let recordset = self.vm.recordset.clone();
            let primary_keys = self.vm.primary_keys.clone();
            let values = bound.map(|bound| bound.values.clone()).unwrap_or_default();
            checkbox = checkbox.on_toggle(move |s: &mut S| {
                let next = set_tuple_bound(
                    &primary_keys,
                    recordset.as_ref(),
                    name,
                    TransferRecordsetTupleBound {
                        values: values.clone(),
                        // The kit checkbox reports its next state; React's
                        // onChange receives `checked`.
                        inclusive: !inclusive,
                    },
                );
                on_change(s, Some(next));
            });
        }
        row()
            .key(format!(
                "data-sync-recordset-{}-inclusive-row",
                match name {
                    BoundName::Start => "start",
                    BoundName::End => "end",
                }
            ))
            .items_center()
            .gap(4.0)
            .child(
                text(t("sync.recordsetInclusive"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(checkbox.build(k))
    }

    /// The scalar editor: the order select, the start/end rows, and the
    /// limit row.
    fn scalar_editor(&self, dark: bool, k: &Kit, layer: &mut Vec<El>) -> El {
        let mut editor = col()
            .key("data-sync-recordset-scalar-editor")
            .w_full()
            .gap(8.0)
            .child(self.order_row(k, layer));
        for name in [BoundName::Start, BoundName::End] {
            editor = editor.child(self.bound_row(dark, k, name));
        }
        editor
    }

    /// The order row: `sync.recordsetOrder` + the select over the keys.
    fn order_row(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let key_refs: Vec<&str> = self.vm.primary_keys.iter().map(String::as_str).collect();
        let order_by = self
            .vm
            .recordset
            .as_ref()
            .and_then(|r| r.order_by.clone())
            .unwrap_or_else(|| default_order(&self.vm.primary_keys));
        let mut select = Select::new("data-sync-recordset-order", &key_refs)
            .width(144.0)
            .open(self.vm.order_select_open)
            .current(Some(order_by.as_str()));
        if let Some(on_toggle_order) = &self.on_toggle_order {
            let on_toggle_order = on_toggle_order.clone();
            select = select.on_toggle(move |s: &mut S| on_toggle_order(s));
        }
        if let Some(on_close_order) = &self.on_close_order {
            let on_close_order = on_close_order.clone();
            select = select.on_close(move |s: &mut S| on_close_order(s));
        }
        if let Some(on_change) = &self.on_change {
            let recordset = self.vm.recordset.clone();
            let on_change = on_change.clone();
            let on_close = self.on_close_order.clone();
            select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
                let next = set_order_by(recordset.as_ref(), value);
                on_change(s, Some(next));
                if let Some(on_close) = &on_close {
                    on_close(s);
                }
            });
        }
        row()
            .key("data-sync-recordset-order-row")
            .items_center()
            .gap(8.0)
            .child(
                text(t("sync.recordsetOrder"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(select.build(k, layer))
    }

    /// One scalar bound row: the label, the input (empty deletes the
    /// bound on submit), and the inclusive flag while the bound exists.
    fn bound_row(&self, dark: bool, k: &Kit, name: BoundName) -> El {
        let (label, key) = match name {
            BoundName::Start => (t("sync.recordsetStart"), "data-sync-recordset-start"),
            BoundName::End => (t("sync.recordsetEnd"), "data-sync-recordset-end"),
        };
        let bound = self.vm.recordset.as_ref().and_then(|r| match name {
            BoundName::Start => r.start.as_ref(),
            BoundName::End => r.end.as_ref(),
        });
        let bind = match name {
            BoundName::Start => self.bind_start.clone(),
            BoundName::End => self.bind_end.clone(),
        };
        let mut row_el = row()
            .key(format!("{key}-row"))
            .flex_wrap()
            .items_center()
            .gap(8.0)
            .child(
                text(label)
                    .w(64.0)
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            );
        if let Some(bind) = bind {
            let bind = bind;
            let on_change = self.on_change.clone();
            let recordset = self.vm.recordset.clone();
            let initial = bound.map(|b| b.value.clone()).unwrap_or_default();
            let field = TextField::new(key, &TextEdit::new(initial))
                .height(28.0)
                .min_width(144.0)
                .grow()
                .text_style(k.ty.text_xsmall)
                .placeholder(t("sync.recordsetUnbounded"))
                .on_edit({
                    let bind = bind.clone();
                    move |s: &mut S| bind(s)
                })
                .on_submit(move |s: &mut S, _tx: &mut Tx| {
                    let value = bind(s).text.clone();
                    if let Some(on_change) = &on_change {
                        let next = set_bound_value(recordset.as_ref(), name, &value);
                        on_change(s, Some(next));
                    }
                })
                .build(dark);
            row_el = row_el.child(field);
        }
        if bound.is_some() {
            // The inclusive flag: `updateBound(name, { inclusive })`.
            let inclusive = bound.map(|b| b.inclusive).unwrap_or(true);
            let mut checkbox = Checkbox::new(format!("{key}-inclusive"), inclusive);
            if let Some(on_change) = &self.on_change {
                let on_change = on_change.clone();
                let recordset = self.vm.recordset.clone();
                checkbox = checkbox.on_toggle(move |s: &mut S| {
                    let next = set_bound_inclusive(recordset.as_ref(), name, !inclusive);
                    on_change(s, Some(next));
                });
            }
            row_el = row_el.child(checkbox.build(k));
        }
        row_el
    }

    /// The limit row: `sync.recordsetLimit` + the numeric input.
    fn limit_row(&self, dark: bool, k: &Kit) -> El {
        let limit = self.vm.recordset.as_ref().and_then(|r| r.limit);
        let mut row_el = row()
            .key("data-sync-recordset-limit-row")
            .items_center()
            .gap(8.0)
            .child(
                text(t("sync.recordsetLimit"))
                    .w(64.0)
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            );
        if let Some(bind) = &self.bind_limit {
            let bind = bind.clone();
            let on_change = self.on_change.clone();
            let recordset = self.vm.recordset.clone();
            let initial = crate::views::format_limit(limit);
            let field = TextField::new("data-sync-recordset-limit", &TextEdit::new(initial))
                .height(28.0)
                .width(144.0)
                .text_style(k.ty.text_xsmall)
                .placeholder(t("sync.recordsetUnbounded"))
                .on_edit({
                    let bind = bind.clone();
                    move |s: &mut S| bind(s)
                })
                .on_submit(move |s: &mut S, _tx: &mut Tx| {
                    let value = bind(s).text.clone();
                    if let Some(on_change) = &on_change {
                        let (next, _error) = set_limit(recordset.as_ref(), &value);
                        on_change(s, Some(next));
                    }
                })
                .build(dark);
            row_el = row_el.child(field);
        }
        row_el
    }
}

/// An Enter handler helper the tests reuse: nothing here — the limit error
/// paragraph renders from the host's flag, whose wiring the panel owns.
pub(crate) fn limit_error_text() -> &'static str {
    t("sync.recordsetLimitInvalid")
}
