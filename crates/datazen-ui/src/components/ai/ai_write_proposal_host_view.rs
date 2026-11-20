//! `AiWriteProposalHost`'s view — T012, the render half of the port.
//!
//! Ported from `packages/frontend/src/components/AiWriteProposalHost.tsx`
//! 132–270: the dialog (exact SQL, optional diff, static impact), the
//! applied-receipt toast and the audit-error toast. The state machine it
//! renders lives in [`super::ai_write_proposal_host`]; the caller owns the
//! [`AiWriteProposalHostState`] and passes `&state` each frame, and presses
//! call back through the `host_mut` accessor into the state machine,
//! exactly as the S6 shell wires its children.
//!
//! # Key convention
//!
//! Keys are React's `data-testid` values verbatim: `ai-write-proposal-backdrop`,
//! `ai-write-proposal-dialog`, `ai-write-proposal-connection`,
//! `ai-write-proposal-database`, `ai-write-proposal-production-warning`,
//! `ai-write-proposal-readonly-warning`, `ai-write-proposal-sql`,
//! `ai-write-proposal-diff`, `ai-write-proposal-impact`,
//! `ai-write-proposal-summary`, `ai-write-proposal-error`,
//! `ai-write-proposal-deny`, `ai-write-proposal-allow`,
//! `ai-write-applied-receipt`, `ai-write-audit-error`. Internal keys (no
//! React testid) are documented here so the host wiring can find them:
//! `ai-write-proposal-title` (React's `aria-labelledby` id),
//! `ai-write-proposal-remaining` (the expiry countdown / `expired` line),
//! `ai-write-applied-receipt-dismiss` and `ai-write-audit-error-dismiss`
//! (React's un-keyed dismiss buttons), `ai-write-proposal-header`,
//! `ai-write-proposal-shield`, `ai-write-proposal-footer`,
//! `ai-write-applied-receipt-strip` and `ai-write-audit-error-strip`
//! (layout wrappers), and `ai-write-proposal-body-{epoch}` (the scrolling
//! body, epoch-suffixed so a reopened dialog starts at the top, as the
//! `Dialog` base does).
//!
//! # Deliberate boundaries
//!
//! * **Panel height.** React caps the panel at `max-h-[85vh]`; Rune has no
//!   viewport-relative size at the component layer, so the body scrolls at
//!   a 400pt cap instead (header + footer stay pinned). The SQL `<pre>`
//!   keeps React's 224pt cap and the diff its 160pt cap.
//! * **Shield glyphs.** React renders lucide `ShieldAlert` for a
//!   production-protected proposal and `ShieldCheck` otherwise; both are
//!   registered in `icons.rs` from the installed `lucide-react@0.487.0`
//!   (`ICON_SHIELD_ALERT`, `ICON_SHIELD_CHECK`).

use std::rc::Rc;

use rune::{col, row, text, Color, El, Key, KeyEvent, ScrollSpec, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize};

use super::ai_write_proposal_host::{
    AiWriteApplyReceipt, AiWriteProposal, AiWriteProposalHostState,
};
use crate::i18n::t;
use crate::icons::{ICON_SHIELD_ALERT, ICON_SHIELD_CHECK};
use crate::ui::kit::kit;

/// Panel corner radius (React's `rounded-lg`).
const PANEL_RADIUS: f32 = 8.0;
/// Panel max width (React's `max-w-2xl`).
const PANEL_WIDTH: f32 = 672.0;
/// The scrim's opacity (React's `bg-black/40`).
const SCRIM_ALPHA: f32 = 0.4;
/// Body cap: the scrolling region's max height (see the module docs —
/// React's `max-h-[85vh]` has no viewport-relative equivalent here).
const BODY_MAX_HEIGHT: f32 = 400.0;
/// The SQL `<pre>` cap (React's `max-h-56`).
const SQL_MAX_HEIGHT: f32 = 224.0;
/// The diff `<pre>` cap (React's `max-h-40`).
const DIFF_MAX_HEIGHT: f32 = 160.0;
/// The metadata label column (React's `grid-cols-[auto_1fr]` sizes to the
/// widest label — `Connection` — so a fixed column keeps the rows aligned).
const META_LABEL_WIDTH: f32 = 76.0;
/// The footer's height (a 28pt button inside React's `py-3`).
const FOOTER_HEIGHT: f32 = 56.0;
pub(crate) struct AiWriteProposalHost<'a, S: 'static> {
    state: &'a AiWriteProposalHostState,
    host: Option<Rc<dyn Fn(&mut S) -> &mut AiWriteProposalHostState>>,
}

impl<'a, S: 'static> AiWriteProposalHost<'a, S> {
    pub(crate) fn new(state: &'a AiWriteProposalHostState) -> Self {
        Self { state, host: None }
    }

    /// The state accessor every button drives, e.g.
    /// `|state| &mut state.ai_write_proposal`.
    pub(crate) fn host_mut(
        mut self,
        f: impl Fn(&mut S) -> &mut AiWriteProposalHostState + 'static,
    ) -> Self {
        self.host = Some(Rc::new(f));
        self
    }

    /// Renders the dialog, the receipt toast and the audit-error toast into
    /// `layer` (the app's overlay layer, as `modal_layer` does for dialogs).
    /// Nothing is pushed when the host is invisible — React returns `null`.
    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) {
        if !self.state.is_visible() {
            return;
        }
        if let Some(current) = self.state.current() {
            layer.push(self.backdrop(dark, current));
        }
        if self.state.current().is_none() {
            if let Some(receipt) = self.state.receipt() {
                layer.push(self.receipt_toast(dark, receipt));
            }
        }
        if let Some(message) = self.state.audit_error() {
            layer.push(self.audit_toast(dark, message));
        }
    }

    /// The backdrop + dialog (`AiWriteProposalHost.tsx:137–232`). The
    /// backdrop is React's `fixed inset-0 … bg-black/40 p-4` flex container;
    /// the panel is the `role="dialog"` card.
    fn backdrop(&self, dark: bool, current: &AiWriteProposal) -> El {
        let k = kit(dark);
        let state = self.state;
        let epoch = state.open_epoch();
        let panel = col()
            .key("ai-write-proposal-dialog")
            .role("dialog")
            .label(t("ai.write.title").to_string())
            .w_full()
            .max_w(PANEL_WIDTH)
            .radius(PANEL_RADIUS)
            .fill(k.c.bg_base)
            .border(1.0, k.c.border_base)
            .child(self.header(dark, current))
            .child(self.body(dark, current, epoch))
            .child(self.footer(dark, current));
        let panel = k.elevate(panel, &k.elevation.overlay);
        col()
            .key("ai-write-proposal-backdrop")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .padding(16.0)
            .center()
            .fill(Color::BLACK.alpha(SCRIM_ALPHA))
            .child(panel)
    }

    /// The header row (`AiWriteProposalHost.tsx:150–164`): the shield tinted
    /// danger for a production-protected proposal and accent otherwise, the
    /// title + description, and the remaining / expired line at the right.
    fn header(&self, dark: bool, current: &AiWriteProposal) -> El {
        let k = kit(dark);
        let state = self.state;
        let (icon, tint) = if current.production_protected {
            (&ICON_SHIELD_ALERT, k.c.danger.icon)
        } else {
            (&ICON_SHIELD_CHECK, k.c.text_accent)
        };
        let remaining = if state.expired() {
            t("ai.write.expired").to_string()
        } else {
            t("ai.write.remaining").replace("{seconds}", &state.remaining_seconds().to_string())
        };
        row()
            .key("ai-write-proposal-header")
            .w_full()
            .px(16.0)
            .py(12.0)
            .items_center()
            .gap(8.0)
            .border(1.0, k.c.border_muted)
            .child(
                rune::icon(icon, 16.0)
                    .color(tint)
                    .key("ai-write-proposal-shield"),
            )
            .child(
                col()
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(2.0)
                    .child(
                        text(t("ai.write.title"))
                            .key("ai-write-proposal-title")
                            .font(k.ty.ui_medium.lh(20.0))
                            .color(k.c.text_base)
                            .truncate(),
                    )
                    .child(
                        text(t("ai.write.description"))
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted)
                            .wrap(),
                    ),
            )
            .child(
                text(remaining)
                    .key("ai-write-proposal-remaining")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
    }

    /// The scrolling body (`AiWriteProposalHost.tsx:166–221`): the
    /// connection / database / hash grid, the production or read-only
    /// warning, the exact SQL, the optional diff, the static impact, and
    /// the response error.
    fn body(&self, dark: bool, current: &AiWriteProposal, epoch: u64) -> El {
        let k = kit(dark);
        let mut body = col()
            .key(format!("ai-write-proposal-body-{epoch}"))
            .w_full()
            .px(16.0)
            .py(12.0)
            .gap(12.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .max_h(BODY_MAX_HEIGHT)
            .child(self.meta_grid(dark, current));
        if current.production_protected {
            body = body.child(
                col()
                    .key("ai-write-proposal-production-warning")
                    .w_full()
                    .px(12.0)
                    .py(8.0)
                    .radius(6.0)
                    .fill(k.c.danger.bg)
                    .border(1.0, k.c.danger.border)
                    .child(
                        text(t("ai.write.productionWarning"))
                            .font(k.ty.text_xsmall)
                            .color(k.c.danger.fg)
                            .wrap(),
                    ),
            );
        } else if !current.apply_allowed {
            body = body.child(
                col()
                    .key("ai-write-proposal-readonly-warning")
                    .w_full()
                    .px(12.0)
                    .py(8.0)
                    .radius(6.0)
                    .fill(k.c.warning.bg)
                    .border(1.0, k.c.warning.border)
                    .child(
                        text(t("ai.write.readOnlyWarning"))
                            .font(k.ty.text_xsmall)
                            .color(k.c.warning.fg)
                            .wrap(),
                    ),
            );
        }
        body = body
            .child(
                text(t("ai.write.exactSql"))
                    .font(k.ty.text_xsmall_medium)
                    .color(k.c.text_base),
            )
            .child(
                col()
                    .key("ai-write-proposal-sql")
                    .w_full()
                    .px(12.0)
                    .py(8.0)
                    .radius(6.0)
                    .fill(k.c.layer_02)
                    .border(1.0, k.c.border_base)
                    .scroll(ScrollSpec {
                        stick_bottom: false,
                    })
                    .max_h(SQL_MAX_HEIGHT)
                    .child(
                        text(current.sql.clone())
                            .font(k.ty.mono)
                            .color(k.c.text_base)
                            .wrap(),
                    ),
            );
        if let Some(diff) = &current.diff {
            body = body
                .child(
                    text(t("ai.write.diff"))
                        .font(k.ty.text_xsmall_medium)
                        .color(k.c.text_base),
                )
                .child(
                    col()
                        .key("ai-write-proposal-diff")
                        .w_full()
                        .px(12.0)
                        .py(8.0)
                        .radius(6.0)
                        .fill(k.c.layer_02)
                        .border(1.0, k.c.border_base)
                        .scroll(ScrollSpec {
                            stick_bottom: false,
                        })
                        .max_h(DIFF_MAX_HEIGHT)
                        .child(
                            text(diff.clone())
                                .font(k.ty.mono)
                                .color(k.c.text_base)
                                .wrap(),
                        ),
                );
        }
        body = body
            .child(
                text(t("ai.write.impact"))
                    .font(k.ty.text_xsmall_medium)
                    .color(k.c.text_base),
            )
            .child(self.impact_box(dark, current));
        if let Some(error) = self.state.error() {
            body = body.child(
                text(error.to_string())
                    .key("ai-write-proposal-error")
                    .font(k.ty.text_xsmall)
                    .color(k.c.danger.fg)
                    .wrap(),
            );
        }
        body
    }

    /// The `grid-cols-[auto_1fr]` metadata grid
    /// (`AiWriteProposalHost.tsx:167–178`).
    fn meta_grid(&self, dark: bool, current: &AiWriteProposal) -> El {
        let k = kit(dark);
        let database = if current.database.is_empty() {
            t("ai.write.noDatabase").to_string()
        } else {
            current.database.clone()
        };
        let mut grid = col().w_full().gap(4.0);
        for (label, value, key) in [
            (
                t("ai.write.connection"),
                current.connection_id.clone(),
                "ai-write-proposal-connection",
            ),
            (
                t("ai.write.database"),
                database,
                "ai-write-proposal-database",
            ),
        ] {
            grid = grid.child(
                row()
                    .w_full()
                    .gap(12.0)
                    .child(
                        text(label)
                            .w(META_LABEL_WIDTH)
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted),
                    )
                    .child(
                        text(value)
                            .key(key)
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_base)
                            .wrap(),
                    ),
            );
        }
        grid.child(
            row()
                .w_full()
                .gap(12.0)
                .child(
                    text(t("ai.write.hash"))
                        .w(META_LABEL_WIDTH)
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted),
                )
                .child(
                    text(current.sql_hash.clone())
                        .font(k.ty.mono)
                        .color(k.c.text_base)
                        .truncate()
                        .help_when_truncated(current.sql_hash.clone()),
                ),
        )
    }

    /// The static impact box (`AiWriteProposalHost.tsx:203–220`).
    fn impact_box(&self, dark: bool, current: &AiWriteProposal) -> El {
        let k = kit(dark);
        let impact = &current.impact;
        let mut counts =
            t("ai.write.statements").replace("{count}", &impact.statement_count.to_string());
        if !impact.tables.is_empty() {
            counts.push_str(" · ");
            counts.push_str(&t("ai.write.tables").replace("{tables}", &impact.tables.join(", ")));
        }
        let mut box_el = col()
            .key("ai-write-proposal-impact")
            .w_full()
            .px(12.0)
            .py(8.0)
            .radius(6.0)
            .fill(k.c.layer_02)
            .border(1.0, k.c.border_base)
            .gap(4.0)
            .child(
                text(impact.summary.clone())
                    .key("ai-write-proposal-summary")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_base)
                    .wrap(),
            )
            .child(
                text(counts)
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted)
                    .wrap(),
            );
        if impact.destructive || !impact.has_where {
            let warning = if impact.destructive {
                t("ai.write.destructiveWarning")
            } else {
                t("ai.write.noWhereWarning")
            };
            box_el = box_el.child(
                text(warning)
                    .font(k.ty.text_xsmall)
                    .color(k.c.danger.fg)
                    .wrap(),
            );
        }
        box_el
    }

    /// The footer (`AiWriteProposalHost.tsx:224–245`): Deny (ghost) and
    /// Allow once (contrast). The allow button mirrors React's `disabled`
    /// (`responding || expired || !applyAllowed`); deny is disabled while
    /// responding.
    fn footer(&self, dark: bool, current: &AiWriteProposal) -> El {
        let k = kit(dark);
        let state = self.state;
        let deny_label = t("ai.write.deny").to_string();
        let mut deny = Button::new(deny_label.clone())
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Regular)
            .help(deny_label)
            .disabled(state.responding())
            .build(k)
            .key("ai-write-proposal-deny");
        let allow_label = t("ai.write.allowOnce").to_string();
        let mut allow = Button::new(allow_label.clone())
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::Regular)
            .help(allow_label)
            .disabled(state.responding() || state.expired() || !current.apply_allowed)
            .build(k)
            .key("ai-write-proposal-allow");
        if let Some(access) = &self.host {
            let deny_press = Rc::clone(access);
            let deny_keyed = Rc::clone(access);
            deny = deny
                .on_press(move |state: &mut S| deny_press(state).deny())
                .on_key(move |state: &mut S, event: &KeyEvent, _tx: &mut Tx| {
                    if event.key == Key::Enter {
                        deny_keyed(state).deny();
                        true
                    } else {
                        false
                    }
                });
            let allow_press = Rc::clone(access);
            let allow_keyed = Rc::clone(access);
            allow = allow
                .on_press(move |state: &mut S| allow_press(state).allow())
                .on_key(move |state: &mut S, event: &KeyEvent, _tx: &mut Tx| {
                    if event.key == Key::Enter {
                        allow_keyed(state).allow();
                        true
                    } else {
                        false
                    }
                });
        }
        row()
            .key("ai-write-proposal-footer")
            .w_full()
            .h(FOOTER_HEIGHT)
            .px(16.0)
            .py(12.0)
            .center_y()
            .justify_end()
            .gap(8.0)
            .border(1.0, k.c.border_muted)
            .child(deny)
            .child(allow)
    }

    /// The applied receipt toast (`AiWriteProposalHost.tsx:248–265`):
    /// bottom-centre, shown only while no proposal is current.
    fn receipt_toast(&self, dark: bool, receipt: &AiWriteApplyReceipt) -> El {
        let k = kit(dark);
        let message = match receipt.row_count {
            Some(count) => t("ai.write.applied").replace("{count}", &count.to_string()),
            None => t("ai.write.appliedUnknown").to_string(),
        };
        let dismiss_label = t("ai.write.dismiss").to_string();
        let mut dismiss = Button::new(dismiss_label.clone())
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .help(dismiss_label)
            .build(k)
            .key("ai-write-applied-receipt-dismiss");
        if let Some(access) = &self.host {
            let dismiss_press = Rc::clone(access);
            let dismiss_keyed = Rc::clone(access);
            dismiss = dismiss
                .on_press(move |state: &mut S| dismiss_press(state).dismiss_receipt())
                .on_key(move |state: &mut S, event: &KeyEvent, _tx: &mut Tx| {
                    if event.key == Key::Enter {
                        dismiss_keyed(state).dismiss_receipt();
                        true
                    } else {
                        false
                    }
                });
        }
        let toast = row()
            .key("ai-write-applied-receipt")
            .px(16.0)
            .py(8.0)
            .items_center()
            .gap(12.0)
            .radius(6.0)
            .fill(k.c.bg_base)
            .border(1.0, k.c.border_base)
            .child(
                text(message)
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted)
                    .wrap(),
            )
            .child(dismiss);
        let toast = k.elevate(toast, &k.elevation.floating);
        row()
            .key("ai-write-applied-receipt-strip")
            .pin(Some(0.0), None, Some(0.0), Some(16.0))
            .justify_center()
            .child(toast)
    }

    /// The audit-error toast (`AiWriteProposalHost.tsx:267–278`):
    /// bottom-right, independent of the dialog and the receipt.
    fn audit_toast(&self, dark: bool, message: &str) -> El {
        let k = kit(dark);
        let dismiss_label = t("ai.write.dismiss").to_string();
        let mut dismiss = Button::new(dismiss_label.clone())
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .help(dismiss_label)
            .build(k)
            .key("ai-write-audit-error-dismiss");
        if let Some(access) = &self.host {
            let dismiss_press = Rc::clone(access);
            let dismiss_keyed = Rc::clone(access);
            dismiss = dismiss
                .on_press(move |state: &mut S| dismiss_press(state).dismiss_audit_error())
                .on_key(move |state: &mut S, event: &KeyEvent, _tx: &mut Tx| {
                    if event.key == Key::Enter {
                        dismiss_keyed(state).dismiss_audit_error();
                        true
                    } else {
                        false
                    }
                });
        }
        let toast = row()
            .key("ai-write-audit-error")
            .px(12.0)
            .py(8.0)
            .items_center()
            .gap(12.0)
            .radius(6.0)
            .fill(k.c.danger.bg)
            .border(1.0, k.c.danger.border)
            .child(
                text(message.to_string())
                    .font(k.ty.text_xsmall)
                    .color(k.c.danger.fg)
                    .wrap(),
            )
            .child(dismiss);
        let toast = k.elevate(toast, &k.elevation.floating);
        row()
            .key("ai-write-audit-error-strip")
            .pin(None, None, Some(16.0), Some(16.0))
            .child(toast)
    }
}
