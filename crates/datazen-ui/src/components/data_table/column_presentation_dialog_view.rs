//! `ColumnPresentationDialog` 的渲染 —— T054-S2 的视图半。
//!
//! Ported from `packages/frontend/src/components/DataTable/ColumnPresentationDialog.tsx`
//! 的 JSX（`Dialog` 内全部内容）：总开关提示 + 显式开启按钮、贡献缺
//! 失提示、生效来源行、模式三 radio、custom 表单（4 个 Select + 2 个
//! IANA 名输入）、预览框（原值 + Unix 秒/毫秒 badge + 四种结果）、无
//! 来源列提示、error 行、保存/取消按钮。状态机在
//! [`super::column_presentation_dialog_state`]；调用方每帧传快照、注
//! 册中心与可选的 presenter（见下）。
//!
//! # Key 约定（React 的 `data-testid` 即 key；Dialog 底座再补
//! `-title` / `-close` / `-body` / `-footer` / `-scrim` / `-layer`）
//!
//! | part | key |
//! |---|---|
//! | panel（`ui::Dialog` 底座） | `column-presentation-dialog` |
//! | body（Dialog 底座的滚动视口，`body_max_height` 448；body 流内
//!   的保存/取消行超出时需先滚到底再点） | `column-presentation-dialog-body` |
//! | 未启用总开关提示 | `column-presentation-not-enabled`（文本为 `-text`） |
//! | 显式开启按钮 | `column-presentation-enable` |
//! | 开启失败行（React 无 testId，新增） | `column-presentation-enable-error` |
//! | 贡献缺失提示 | `column-presentation-contribution-missing`（文本为 `-text`） |
//! | 生效来源行 | `column-presentation-effective-source` |
//! | 模式 radio 组 | `column-presentation-mode`，三项依次为 `-item-0`（inherit）/ `-item-1`（raw）/ `-item-2`（custom） |
//! | 解释 Select | 挂载的 trigger 为 `column-presentation-interpretation-trigger`（label 为 `-label`；kit `Select` 的 trigger key 是 `{key}-trigger`，不是 `{key}` 本身） |
//! | 格式 Select | `column-presentation-format-trigger` |
//! | 显示时区 Select | `column-presentation-display-timezone-trigger` |
//! | 显示 IANA 名输入 | `column-presentation-display-iana-name` |
//! | 源时区 Select | `column-presentation-source-timezone-trigger` |
//! | 源 IANA 名输入 | `column-presentation-source-iana-name` |
//! | 预览框 | `column-presentation-preview` |
//! | 原值 | `column-presentation-original` |
//! | Unix 秒/毫秒 badge | `column-presentation-preview-unix-badge` |
//! | 预览结果 | `column-presentation-preview-result` |
//! | 无来源列提示 | `column-presentation-no-source`（文本为 `-text`） |
//! | error 行 | `column-presentation-error` |
//! | 保存按钮 | `column-presentation-save` |
//! | 取消按钮 | `column-presentation-cancel` |
//!
//! # Deliberate boundaries
//!
//! * **快照 / 注册中心 / presenter 由调用方每帧传入**：它们是宿主
//!   状态，不存在对话框状态里（与 `AdminCreateDialog` 的定义由宿主
//!   喂入同理）。缺任何一个 `build` 都直接返回、不挂载 —— 没有快
//!   照算不出派生，没有注册中心判不了贡献缺失。
//! * **Select 的 listbox 在 modal 之上**：kit `Select::build` 把打
//!   开的 listbox 推入调用方给的 `layer`；直接传外层 `layer` 会让
//!   它落在 Dialog 的 scrim/panel 之前（被遮住）。这里先推入本地
//!   `select_layer`，在 `Dialog::build` 之后再 `extend` 到外层，
//!   listbox 始终在 modal 之上。
//! * **保存/取消按钮在 body 流内、左对齐**：React 把它们放在 body
//!   末尾的 `flex` 行里，不是 Dialog 的 footer 槽；用底座 footer
//!   会右对齐，所以这里作为 body 的最后一个孩子。
//! * **`is_known_time_zone` 缺省恒假**：S1 的接线口，宿主应传平台
//!   检查；不传时 IANA 名一律判未知（fail-closed），保存会被就地
//!   拦住而不是放行一个无法验证的时区。

// The production consumer is the DataTable wiring task; until then the view
// is exercised by `tests/column_presentation_dialog_s2.rs` only.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, row, text, El, Key, KeyEvent, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use super::column_presentation_dialog::{
    ColumnPresentationDraft, InterpretationChoice, PresentationMode, SourceTimeZoneChoice,
    TimeZoneKindChoice, TimestampOutputFormat, ValuePresentationSnapshot,
};
use super::column_presentation_dialog_state::{
    ColumnPresentationDialogState, ColumnPresentationDraftPatch, ColumnPresentationSelect,
    DISPLAY_IANA_PLACEHOLDER,
};
use super::column_presentation_plan::{ColumnPresentationPreview, TimestampPresenter};
use super::column_presentation_rules::ValuePresentationRegistry;
use crate::i18n::t;
use crate::ui::banner::{Banner, BannerVariant};
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::select::Select;
use crate::ui::toggle::{RadioGroup, RadioItem};

/// Body cap: the scrolling region's max height (the T012 body cap — React
/// has no cap here, but a tall custom form must not push the actions out).
const BODY_MAX_HEIGHT: f32 = 448.0;
/// React 的 `min-w-80`（body 最小宽）。
const BODY_MIN_WIDTH: f32 = 320.0;
/// IANA 名输入与 Select 的行高（React `INPUT_CLASS` 的 `h-9`）。
const FIELD_HEIGHT: f32 = 36.0;

/// S1 选择类型 → Select 的 `values` 用字符串（与 `from_str` 的接受集一致）。
fn interpretation_value(choice: InterpretationChoice) -> &'static str {
    match choice {
        InterpretationChoice::Native => "native",
        InterpretationChoice::UnixSeconds => "unix-seconds",
        InterpretationChoice::UnixMilliseconds => "unix-milliseconds",
        InterpretationChoice::Text => "text",
    }
}

fn format_value(format: TimestampOutputFormat) -> &'static str {
    match format {
        TimestampOutputFormat::Iso => "iso",
        TimestampOutputFormat::IsoTz => "iso-tz",
        TimestampOutputFormat::Locale => "locale",
    }
}

fn time_zone_kind_value(kind: TimeZoneKindChoice) -> &'static str {
    match kind {
        TimeZoneKindChoice::Local => "local",
        TimeZoneKindChoice::Utc => "utc",
        TimeZoneKindChoice::Iana => "iana",
    }
}

fn source_time_zone_value(kind: SourceTimeZoneChoice) -> &'static str {
    match kind {
        SourceTimeZoneChoice::None => "none",
        SourceTimeZoneChoice::Local => "local",
        SourceTimeZoneChoice::Utc => "utc",
        SourceTimeZoneChoice::Iana => "iana",
    }
}

/// The column-presentation dialog panel.
pub(crate) struct ColumnPresentationDialog<'a, S: 'static> {
    state: &'a ColumnPresentationDialogState,
    snapshot: Option<&'a ValuePresentationSnapshot>,
    registry: Option<&'a ValuePresentationRegistry>,
    presenter: Option<TimestampPresenter>,
    is_known_time_zone: Rc<dyn Fn(&str) -> bool>,
    host: Option<Rc<dyn Fn(&mut S) -> &mut ColumnPresentationDialogState>>,
}

impl<'a, S: 'static> ColumnPresentationDialog<'a, S> {
    pub(crate) fn new(state: &'a ColumnPresentationDialogState) -> Self {
        Self {
            state,
            snapshot: None,
            registry: None,
            presenter: None,
            is_known_time_zone: Rc::new(|_| false),
            host: None,
        }
    }

    /// The snapshot the dialog derives from (host-owned, passed per frame).
    pub(crate) fn snapshot(mut self, snapshot: &'a ValuePresentationSnapshot) -> Self {
        self.snapshot = Some(snapshot);
        self
    }

    /// The registry the dialog reads the contribution from (host-owned,
    /// passed per frame; a fresh one each render is what makes the T11
    /// missing notice appear/disappear while the panel is open).
    pub(crate) fn registry(mut self, registry: &'a ValuePresentationRegistry) -> Self {
        self.registry = Some(registry);
        self
    }

    /// The timestamp engine's `present` (T253/T209/T274 wiring); `None`
    /// means `ready` previews fall back to `plan-error` (see the plan
    /// module header).
    pub(crate) fn presenter(mut self, presenter: TimestampPresenter) -> Self {
        self.presenter = Some(presenter);
        self
    }

    /// The platform's IANA known-name check (S1's seam); defaults to
    /// fail-closed (`|_| false`).
    pub(crate) fn is_known_time_zone(mut self, check: Rc<dyn Fn(&str) -> bool>) -> Self {
        self.is_known_time_zone = check;
        self
    }

    /// The state accessor every button and field drives, e.g.
    /// `|state| &mut state.column_presentation_dialog`.
    pub(crate) fn host_mut(
        mut self,
        f: impl Fn(&mut S) -> &mut ColumnPresentationDialogState + 'static,
    ) -> Self {
        self.host = Some(Rc::new(f));
        self
    }

    /// Renders the dialog into `layer`. Nothing is pushed when the dialog
    /// is closed, when no host accessor was given, or when the snapshot /
    /// registry was not supplied (see the module docs).
    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) {
        let state = self.state;
        if !state.is_open() {
            return;
        }
        let Some(host) = &self.host else {
            return;
        };
        let Some(snapshot) = self.snapshot else {
            return;
        };
        let Some(registry) = self.registry else {
            return;
        };
        let k = kit(dark);
        let draft = state.draft();
        let plan = state.current_plan(snapshot, registry, self.presenter.as_ref());
        let preview = state.preview(snapshot, registry, self.presenter.as_ref());
        // The preview already carries its sample; only `has_sample`
        // decides the `noSample` line.
        let (_, has_sample) = state.sample();

        // Select listboxes go here first, then above the modal (see the
        // module docs).
        let mut select_layer: Vec<El> = Vec::new();
        // No key here: the Dialog base already keys its scroll viewport
        // `column-presentation-dialog-body`; a second element with the
        // same key would make `find`/`maybe` ambiguous.
        let mut body = col().w_full().min_w(BODY_MIN_WIDTH).gap(16.0);

        if !state.time_enabled(snapshot) {
            body = body.child(self.not_enabled_block(dark, host, snapshot));
        }
        if state.contribution_missing(registry) {
            body = body.child(self.status_box(
                k,
                "column-presentation-contribution-missing",
                t("settings.valuePresentation.contributionMissing"),
            ));
        }
        body = body.child(
            text(format!(
                "{}: {}",
                t("columnPresentation.effectiveSource"),
                state.effective_source_label(&plan)
            ))
            .key("column-presentation-effective-source")
            .font(k.ty.text_small)
            .color(k.c.text_muted),
        );
        body = body.child(self.mode_group(k, host, &draft));
        if draft.mode == PresentationMode::Custom {
            body = body.child(self.custom_form(dark, host, &draft, &mut select_layer));
        }
        body = body.child(self.preview_block(k, &draft, &preview, has_sample));
        if state.save_disabled() {
            body = body.child(self.status_box(
                k,
                "column-presentation-no-source",
                t("columnPresentation.noSource"),
            ));
        }
        if let Some(error) = state.error() {
            if let Some(banner) = Banner::new("column-presentation-error", error)
                .variant(BannerVariant::Plain)
                .build(k)
            {
                body = body.child(banner);
            }
        }
        body = body.child(self.actions_row(dark, host));

        let title = format!(
            "{} — {}",
            t("columnPresentation.title"),
            state.config().column_name
        );
        let close_host = Rc::clone(host);
        Dialog::new("column-presentation-dialog", title)
            .open(true)
            .open_epoch(state.open_epoch())
            .fit()
            .body_max_height(BODY_MAX_HEIGHT)
            .on_close(move |state: &mut S| close_host(state).close())
            .child(body)
            .build(k, layer);
        layer.extend(select_layer);
    }

    /// A muted bordered status box (React's `rounded-md border
    /// border-edge bg-surface-alt px-3 py-2 text-sm text-fg-secondary`
    /// status lines).
    fn status_box(&self, k: &Kit, key: &str, message: &str) -> El {
        col()
            .key(key)
            .role("status")
            .w_full()
            .radius(k.radius.md)
            .fill(k.c.bg_deep)
            .border(1.0, k.c.border_muted)
            .px(12.0)
            .py(8.0)
            .child(
                text(message.to_owned())
                    .key(format!("{key}-text"))
                    .font(k.ty.text_small)
                    .color(k.c.text_muted),
            )
    }

    /// The not-enabled notice with its explicit enable button
    /// (`column-presentation-not-enabled` + `column-presentation-enable`).
    /// The enable options are computed at build time from the current
    /// snapshot and moved into the press handler: a `'static` closure
    /// cannot borrow the per-frame snapshot.
    fn not_enabled_block(
        &self,
        dark: bool,
        host: &Rc<dyn Fn(&mut S) -> &mut ColumnPresentationDialogState>,
        snapshot: &ValuePresentationSnapshot,
    ) -> El {
        let k = kit(dark);
        let state = self.state;
        let options = state.default_enable_options(snapshot);
        let enable_host = Rc::clone(host);
        let mut enable = Button::new(t("columnPresentation.enable").to_string())
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::Regular)
            .help(t("columnPresentation.enable").to_string())
            .disabled(state.enabling())
            .build(k)
            .key("column-presentation-enable");
        enable = enable.on_press(move |state: &mut S| {
            enable_host(state).enable_time_display_with(options.clone());
        });
        let mut block = col()
            .key("column-presentation-not-enabled")
            .role("status")
            .w_full()
            .gap(8.0)
            .radius(k.radius.md)
            .fill(k.c.bg_deep)
            .border(1.0, k.c.border_muted)
            .px(12.0)
            .py(8.0)
            .child(
                text(t("columnPresentation.notEnabled").to_string())
                    .key("column-presentation-not-enabled-text")
                    .font(k.ty.text_small)
                    .color(k.c.text_muted),
            )
            .child(enable);
        if let Some(enable_error) = state.enable_error() {
            block = block.child(
                text(enable_error.to_string())
                    .key("column-presentation-enable-error")
                    .role("alert")
                    .font(k.ty.text_xsmall)
                    .color(k.c.danger.fg),
            );
        }
        block
    }

    /// The mode radio group (inherit / raw / custom, each with its hint).
    fn mode_group(
        &self,
        k: &Kit,
        host: &Rc<dyn Fn(&mut S) -> &mut ColumnPresentationDialogState>,
        draft: &ColumnPresentationDraft,
    ) -> El {
        let mut group = RadioGroup::new("column-presentation-mode")
            .label(t("columnPresentation.mode").to_string());
        for (mode, key) in [
            (PresentationMode::Inherit, "inherit"),
            (PresentationMode::Raw, "raw"),
            (PresentationMode::Custom, "custom"),
        ] {
            let select_host = Rc::clone(host);
            let item = RadioItem::new(
                t(&format!("columnPresentation.mode.{key}")).to_string(),
                draft.mode == mode,
            )
            .description(t(&format!("columnPresentation.mode.{key}.hint")).to_string())
            .on_select(move |state: &mut S| {
                select_host(state).update_draft(ColumnPresentationDraftPatch {
                    mode: Some(mode),
                    ..ColumnPresentationDraftPatch::default()
                });
            });
            group = group.item(item);
        }
        group.build(k)
    }

    /// The custom form: interpretation / format / display-timezone
    /// Selects, the display IANA input (iana only), the source-timezone
    /// Select (text / native only) and the source IANA input (iana only).
    fn custom_form(
        &self,
        dark: bool,
        host: &Rc<dyn Fn(&mut S) -> &mut ColumnPresentationDialogState>,
        draft: &ColumnPresentationDraft,
        select_layer: &mut Vec<El>,
    ) -> El {
        let k = kit(dark);
        let state = self.state;
        let mut form = col().w_full().gap(12.0);

        let interpretation_labels = [
            t("settings.valuePresentation.preview.source.native"),
            t("settings.valuePresentation.preview.source.unix-seconds"),
            t("settings.valuePresentation.preview.source.unix-milliseconds"),
            t("settings.valuePresentation.preview.source.text"),
        ];
        let interpretation_values = ["native", "unix-seconds", "unix-milliseconds", "text"];
        form = form.child(self.select_field(
            k,
            host,
            select_layer,
            "column-presentation-interpretation",
            t("settings.valuePresentation.rules.editor.interpretation"),
            &interpretation_labels,
            &interpretation_values,
            interpretation_value(draft.interpretation),
            ColumnPresentationSelect::Interpretation,
        ));

        let format_labels = [
            t("settings.valuePresentation.format.iso"),
            t("settings.valuePresentation.format.iso-tz"),
            t("settings.valuePresentation.format.locale"),
        ];
        let format_values = ["iso", "iso-tz", "locale"];
        form = form.child(self.select_field(
            k,
            host,
            select_layer,
            "column-presentation-format",
            t("settings.valuePresentation.format"),
            &format_labels,
            &format_values,
            format_value(draft.format),
            ColumnPresentationSelect::Format,
        ));

        let tz_labels = [
            t("settings.valuePresentation.timeZone.local"),
            t("settings.valuePresentation.timeZone.utc"),
            t("settings.valuePresentation.timeZone.iana"),
        ];
        let tz_values = ["local", "utc", "iana"];
        form = form.child(self.select_field(
            k,
            host,
            select_layer,
            "column-presentation-display-timezone",
            t("settings.valuePresentation.displayTimeZone"),
            &tz_labels,
            &tz_values,
            time_zone_kind_value(draft.display_time_zone_kind),
            ColumnPresentationSelect::DisplayTimeZone,
        ));

        if draft.display_time_zone_kind == TimeZoneKindChoice::Iana {
            let edit_host = Rc::clone(host);
            let field = TextField::new(
                "column-presentation-display-iana-name",
                state.display_iana_name_edit_ref(),
            )
            .height(FIELD_HEIGHT)
            .placeholder(DISPLAY_IANA_PLACEHOLDER.to_string())
            .on_edit(move |state: &mut S| edit_host(state).display_iana_name_edit())
            .build(dark);
            form = form.child(
                col()
                    .w_full()
                    .gap(4.0)
                    .child(
                        text(t("settings.valuePresentation.ianaName").to_string())
                            .key("column-presentation-display-iana-name-label")
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted),
                    )
                    .child(field),
            );
        }

        if state.show_source_time_zone() {
            let source_tz_labels = [
                t("settings.valuePresentation.rules.editor.sourceTimeZone.none"),
                t("settings.valuePresentation.timeZone.local"),
                t("settings.valuePresentation.timeZone.utc"),
                t("settings.valuePresentation.timeZone.iana"),
            ];
            let source_tz_values = ["none", "local", "utc", "iana"];
            form = form.child(self.select_field(
                k,
                host,
                select_layer,
                "column-presentation-source-timezone",
                t("columnPresentation.sourceTimeZone"),
                &source_tz_labels,
                &source_tz_values,
                source_time_zone_value(draft.source_time_zone_kind),
                ColumnPresentationSelect::SourceTimeZone,
            ));
            if draft.source_time_zone_kind == SourceTimeZoneChoice::Iana {
                let edit_host = Rc::clone(host);
                let field = TextField::new(
                    "column-presentation-source-iana-name",
                    state.source_iana_name_edit_ref(),
                )
                .height(FIELD_HEIGHT)
                .placeholder(DISPLAY_IANA_PLACEHOLDER.to_string())
                .on_edit(move |state: &mut S| edit_host(state).source_iana_name_edit())
                .build(dark);
                form = form.child(
                    col()
                        .w_full()
                        .gap(4.0)
                        .child(
                            text(t("settings.valuePresentation.ianaName").to_string())
                                .key("column-presentation-source-iana-name-label")
                                .font(k.ty.text_xsmall)
                                .color(k.c.text_muted),
                        )
                        .child(field),
                );
            }
        }
        form
    }

    /// One labelled Select: the visible label above the field, the kit
    /// field itself (labels shown, values answered), open state from the
    /// dialog state, listbox into `select_layer`.
    #[allow(clippy::too_many_arguments)]
    fn select_field(
        &self,
        k: &Kit,
        host: &Rc<dyn Fn(&mut S) -> &mut ColumnPresentationDialogState>,
        select_layer: &mut Vec<El>,
        key: &str,
        label: &str,
        labels: &[&str],
        values: &[&str],
        current: &str,
        which: ColumnPresentationSelect,
    ) -> El {
        let state = self.state;
        let toggle_host = Rc::clone(host);
        let close_host = Rc::clone(host);
        let choose_host = Rc::clone(host);
        let field = Select::new(key, labels)
            .values(values)
            .current(Some(current))
            .label(label.to_string())
            .open(state.is_select_open(which))
            .on_toggle(move |state: &mut S| toggle_host(state).toggle_select(which))
            .on_close(move |state: &mut S| close_host(state).set_select_open(which, false))
            .on_choose(move |state: &mut S, value: &str, _tx: &mut Tx| {
                choose_host(state).choose_select(which, value);
            })
            .build(k, select_layer);
        col()
            .w_full()
            .gap(4.0)
            .child(
                text(label.to_string())
                    .key(format!("{key}-label"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(field)
    }

    /// The preview box: the original value, the Unix seconds /
    /// milliseconds badge (custom + unix only), and the result line —
    /// `noSample` / `text` / `unchanged` / `invalid` / `unavailable`.
    fn preview_block(
        &self,
        k: &Kit,
        draft: &ColumnPresentationDraft,
        preview: &ColumnPresentationPreview,
        has_sample: bool,
    ) -> El {
        let state = self.state;
        let mut head = row()
            .w_full()
            .gap(8.0)
            .items_center()
            .child(
                text(t("columnPresentation.original").to_string())
                    .font(k.ty.text_small)
                    .color(k.c.text_muted),
            )
            .child(
                text(state.sample_text())
                    .key("column-presentation-original")
                    .font(k.ty.mono)
                    .color(k.c.text_base),
            );
        if draft.mode == PresentationMode::Custom {
            let badge = match draft.interpretation {
                InterpretationChoice::UnixSeconds => {
                    Some(t("settings.valuePresentation.preview.unixSeconds"))
                }
                InterpretationChoice::UnixMilliseconds => {
                    Some(t("settings.valuePresentation.preview.unixMilliseconds"))
                }
                _ => None,
            };
            if let Some(badge) = badge {
                head = head.child(
                    text(badge.to_string())
                        .key("column-presentation-preview-unix-badge")
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_accent),
                );
            }
        }
        let mut block = col()
            .key("column-presentation-preview")
            .w_full()
            .gap(6.0)
            .radius(k.radius.md)
            .fill(k.c.bg_deep)
            .border(1.0, k.c.border_muted)
            .px(12.0)
            .py(12.0)
            .child(head);
        let result = self.preview_result_text(k, preview, has_sample);
        block = block.child(
            text(result.text)
                .key("column-presentation-preview-result")
                .font(k.ty.mono)
                .color(result.color),
        );
        block
    }

    /// The preview result line's text and colour.
    fn preview_result_text(
        &self,
        k: &Kit,
        preview: &ColumnPresentationPreview,
        has_sample: bool,
    ) -> PreviewResultText {
        if !has_sample {
            return PreviewResultText {
                text: t("columnPresentation.preview.noSample").to_string(),
                color: k.c.text_muted,
            };
        }
        match preview {
            ColumnPresentationPreview::Text { text, .. } => PreviewResultText {
                text: text.clone(),
                color: k.c.text_base,
            },
            ColumnPresentationPreview::Unchanged { reason, .. } => PreviewResultText {
                text: match reason {
                    Some(reason) => {
                        format!("{}: {reason}", t("columnPresentation.preview.unchanged"))
                    }
                    None => t("columnPresentation.preview.unchanged").to_string(),
                },
                color: k.c.text_muted,
            },
            ColumnPresentationPreview::Invalid { problems, .. } => PreviewResultText {
                text: format!(
                    "{}: {}",
                    t("settings.valuePresentation.preview.invalid"),
                    problems
                        .iter()
                        .map(|problem| format!("{}: {}", problem.path, problem.message))
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
                color: k.c.danger.fg,
            },
            ColumnPresentationPreview::Unavailable { .. } => PreviewResultText {
                text: t("settings.valuePresentation.preview.unavailable").to_string(),
                color: k.c.text_muted,
            },
        }
    }

    /// The Save / Cancel row (body flow, left aligned — see the module docs).
    fn actions_row(
        &self,
        dark: bool,
        host: &Rc<dyn Fn(&mut S) -> &mut ColumnPresentationDialogState>,
    ) -> El {
        let k = kit(dark);
        let state = self.state;
        let save_host = Rc::clone(host);
        let known = Rc::clone(&self.is_known_time_zone);
        let mut save = Button::new(t("columnPresentation.save").to_string())
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::Regular)
            .help(t("columnPresentation.save").to_string())
            .disabled(state.save_disabled() || state.saving())
            .build(k)
            .key("column-presentation-save");
        save = save.on_press(move |state: &mut S| {
            let known = Rc::clone(&known);
            save_host(state).save(&*known);
        });
        let cancel_host = Rc::clone(host);
        let mut cancel = Button::new(t("common.cancel").to_string())
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Regular)
            .help(t("common.cancel").to_string())
            .build(k)
            .key("column-presentation-cancel");
        cancel = cancel.on_press(move |state: &mut S| cancel_host(state).close());
        // Enter on a focused button activates it (the admin dialog's
        // `on_key` shape); the Save press reuses the same handler.
        let save_key_host = Rc::clone(host);
        let save_key_known = Rc::clone(&self.is_known_time_zone);
        save = save.on_key(move |state: &mut S, event: &KeyEvent, _tx: &mut Tx| {
            if event.key == Key::Enter {
                let known = Rc::clone(&save_key_known);
                save_key_host(state).save(&*known);
                true
            } else {
                false
            }
        });
        let cancel_key_host = Rc::clone(host);
        cancel = cancel.on_key(move |state: &mut S, event: &KeyEvent, _tx: &mut Tx| {
            if event.key == Key::Enter {
                cancel_key_host(state).close();
                true
            } else {
                false
            }
        });
        row().w_full().gap(8.0).child(save).child(cancel)
    }
}

/// The preview result line's text with its colour token.
struct PreviewResultText {
    text: String,
    color: rune::Color,
}
