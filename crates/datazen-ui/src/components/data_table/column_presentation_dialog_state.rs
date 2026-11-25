//! T054-S2: `ColumnPresentationDialog` 的状态机 ——
//! `packages/frontend/src/components/DataTable/ColumnPresentationDialog.tsx`
//! 的 props、state 与保存/开启接线（对话框组件本身，渲染在
//! `column_presentation_dialog_view.rs`）。
//!
//! | React source | Rune item |
//! |---|---|
//! | `ColumnPresentationDialogProps` | [`ColumnPresentationDialogConfig`] |
//! | `draft` / `error` / `saving` / `enabling` / `enableError` state | [`ColumnPresentationDialogState`] |
//! | 打开/换列时重建草稿的 `useEffect` | [`ColumnPresentationDialogState::open_dialog`] |
//! | `updateDraft` | [`ColumnPresentationDialogState::update_draft`] |
//! | `handleSave` | [`ColumnPresentationDialogState::save`] + `save_succeeded` / `save_failed` |
//! | `handleEnableTimeDisplay` | [`ColumnPresentationDialogState::enable_time_display`] + `enable_succeeded` / `enable_failed` |
//! | 派生（`effectiveCurrentOverride` / `globalOptions` / `timeEnabled` / `sample` / `candidate` / `draftProblems` / `preview` / `currentPlan` / `effectiveSourceLabel`） | 同名方法（见下） |
//!
//! # 状态机形状（对照 React，见各方法注释）
//!
//! - 草稿只在打开/换列时重建，关闭期间不保留（下次打开重建覆盖）。
//! - IANA 名的输入框是 `TextEdit`（Rune 文本框只写 `TextEdit`）：
//!   两个名字以 [`ColumnPresentationDialogState::display_iana_name_edit`]
//!   / `source_iana_name_edit` 为准，[`ColumnPresentationDialogState::draft`]
//!   读草稿时把两处文本代入；`open_dialog` 重建时双向同步。
//! - Select 的开合由状态持有（kit `Select` 的 `open` + `on_toggle` /
//!   `on_close` 模型），见 [`ColumnPresentationSelect`]。
//!
//! # 持久化接缝（宿主的活，不在本模块）
//!
//! React 的默认保存路径经 settingsStore 写持久化（乐观写会话存储 →
//! `persist` updater → 成功后清除会话条目），默认开启路径经
//! `updateValuePresentationSettings` 把 `datazen.timestamp` 置
//! enabled —— 两条链路的引擎（会话存储、store updater、
//! `saveColumnOverride` / `withTimestampRenderer`）分别是 T037 /
//! T125 / T312 的活，本 crate 还没有。本模块只做到接缝：
//! - [`ColumnPresentationDialogState::save`] 做全部对话框侧校验后，
//!   按 `has_on_save` 把请求记进 `save_requests` outbox
//!   （`OnSave` 即 React 的 `onSave` 回调路径，保存后立即关闭；
//!   `Persist` 即默认路径，宿主执行保存链路）；宿主的回答经
//!   `save_succeeded`（关闭）/ `save_failed`（就地报错、面板保持
//!   打开、会话条目保留、旧配置继续生效——由宿主保证）回来。
//! - [`ColumnPresentationDialogState::enable_time_display`] 同理记进
//!   `enable_requests` outbox（`OnEnable` / `Persist{options}`，
//!   options 沿用快照里的合法全局值否则默认 native / iso / local）；
//!   宿主回答经 `enable_succeeded` / `enable_failed` 回来。
//! - **保存绝不静默打开总开关**：`save` 的两个分支都不碰
//!   `enabledRendererIds`，开启只走显式的 enable 路径。
//!
//! # 两个有意的偏差
//!
//! - `save` 在 `saving` 为真时直接返回（React 的 `handleSave` 没有
//!   这道 guard，只有按钮禁用；二次提交会写同一列两次，所以这里
//!   与 `AdminCreateDialogState::create` 的 in-flight guard 同理）。
//! - 打字（IANA 名输入）不清除 `error`：React 的 `updateDraft` 在
//!   每次输入时清错，但 Rune 的 `TextField` 没有输入后回调，只有
//!   `on_edit`（取 `&mut TextEdit`）；错误在下一次保存尝试或任
//!   何 Select / radio 改动时清除。

// The production consumer is the DataTable wiring task; until then the
// state is exercised by `tests/column_presentation_dialog_s2.rs` only.
#![allow(dead_code)]

use rune::TextEdit;

use super::column_presentation_dialog::{
    draft_from_column_override, draft_to_column_override, global_timestamp_options_from_snapshot,
    validate_column_presentation_draft, validate_column_presentation_override,
    ColumnPresentationDraft, ColumnPresentationOverride, InterpretationChoice, PresentationMode,
    PresentationSemanticKind, SourceTimeZoneChoice, TimeZoneKindChoice, TimeZoneSpec,
    TimestampInterpretation, TimestampOptions, TimestampOutputFormat, ValidationProblem,
    ValuePresentationSnapshot, DEFAULT_DISPLAY_IANA_NAME, TIMESTAMP_CONTRIBUTION_ID,
};
use super::column_presentation_plan::{
    build_column_presentation_plan, column_override_from_snapshot,
    compute_column_presentation_preview, fallback_sample_for_column,
    format_column_presentation_sample_value, resolve_presentation_semantic_kind,
    snapshot_without_column_override, ColumnPresentationPlan, ColumnPresentationPreview,
    ColumnSource, PresentationPlanColumn, PresentationValue, TimestampPresenter,
};
use super::column_presentation_rules::ValuePresentationRegistry;
use crate::i18n::t;
use crate::support::cell_types::CellTypeHints;

/// 对话框的 props（`ColumnPresentationDialogProps`），由宿主持有、
//  随打开列固定。`registry` 缺省为全局 `valuePresentationRegistry` ——
//  Rune 侧没有全局单例，宿主在视图层把注册中心传进来（见
//  `column_presentation_dialog_view.rs`）；`snapshot` 同样由宿主每
//  帧传入，不存在状态里。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ColumnPresentationDialogConfig {
    pub column_name: String,
    pub column_type: Option<String>,
    pub cell_type_hints: Option<CellTypeHints>,
    /// 表身份；缺省表示查询结果等无来源列——保存覆盖禁用（PRD §3.2）。
    pub source: Option<ColumnSource>,
    /// 预览样本：调用方给真实行值；缺省时按语义种给 PRD 双锚点样本。
    pub sample_value: Option<PresentationValue>,
    /// 当前覆盖；缺省时从快照取。
    pub current_override: Option<ColumnPresentationOverride>,
    /// 调用方是否传了自己的 `onSave`（只调回调、不碰持久化）。
    pub has_on_save: bool,
    /// 调用方是否传了自己的 `onEnableTimeDisplay`。
    pub has_on_enable_time_display: bool,
}

impl ColumnPresentationDialogConfig {
    pub fn new(column_name: impl Into<String>) -> Self {
        Self {
            column_name: column_name.into(),
            ..Self::default()
        }
    }
}

/// 保存请求的 outbox 记录（宿主排空；见模块头「持久化接缝」）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColumnPresentationSaveRequest {
    /// React 的 `onSave` 路径：只调回调、不碰持久化，记录后立即关闭。
    OnSave(ColumnPresentationOverride),
    /// 默认路径：宿主经 `saveColumnOverride` 写持久化（先乐观写会话
    /// 存储，成功后清除同一键的会话条目，失败就地报错、面板保持打开、
    /// 会话条目保留、旧配置继续生效）。
    Persist {
        source: ColumnSource,
        column_name: String,
        column_override: ColumnPresentationOverride,
    },
}

/// 开启总开关请求的 outbox 记录（宿主排空；见模块头）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColumnPresentationEnableRequest {
    /// React 的 `onEnableTimeDisplay` 路径：只调回调。
    OnEnable,
    /// 默认路径：宿主经 `updateValuePresentationSettings` 只把
    /// `datazen.timestamp` 置 enabled（updater 形式、只改这一项，
    /// options 沿用快照里的合法全局值否则默认 native / iso / local）。
    Persist { options: TimestampOptions },
}

/// 草稿补丁（React 的 `Partial<ColumnPresentationDraft>`；IANA 名
/// 不走补丁——它们由两个 `TextEdit` 持有，见模块头）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ColumnPresentationDraftPatch {
    pub mode: Option<PresentationMode>,
    pub interpretation: Option<InterpretationChoice>,
    pub format: Option<TimestampOutputFormat>,
    pub display_time_zone_kind: Option<TimeZoneKindChoice>,
    pub source_time_zone_kind: Option<SourceTimeZoneChoice>,
}

/// 对话框里四个 Select 的开合（kit `Select` 的 `open` 状态由应用持有）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnPresentationSelect {
    Interpretation,
    Format,
    DisplayTimeZone,
    SourceTimeZone,
}

/// 对话框的状态：props + React 的五个 `useState` + 四个 Select 开合
/// + 两个 IANA 名输入 + 保存/开启 outbox。
#[derive(Clone, Debug)]
pub struct ColumnPresentationDialogState {
    config: ColumnPresentationDialogConfig,
    open: bool,
    /// 每次打开 +1（`Dialog::open_epoch` 模式：重开滚回顶部）。
    open_epoch: u64,
    draft: ColumnPresentationDraft,
    /// IANA 名输入框（见模块头：名字以这两个 `TextEdit` 为准）。
    display_iana_name: TextEdit,
    source_iana_name: TextEdit,
    interpretation_open: bool,
    format_open: bool,
    display_time_zone_open: bool,
    source_time_zone_open: bool,
    error: Option<String>,
    saving: bool,
    enabling: bool,
    enable_error: Option<String>,
    save_requests: Vec<ColumnPresentationSaveRequest>,
    enable_requests: Vec<ColumnPresentationEnableRequest>,
}

impl ColumnPresentationDialogState {
    pub fn new(config: ColumnPresentationDialogConfig) -> Self {
        let kind = resolve_presentation_semantic_kind(
            config.column_type.as_deref(),
            config.cell_type_hints.as_ref(),
        );
        let draft = draft_from_column_override(&ColumnPresentationOverride::Inherit, None, kind);
        Self {
            display_iana_name: TextEdit::new(draft.display_iana_name.clone()),
            source_iana_name: TextEdit::new(draft.source_iana_name.clone()),
            config,
            open: false,
            open_epoch: 0,
            draft,
            interpretation_open: false,
            format_open: false,
            display_time_zone_open: false,
            source_time_zone_open: false,
            error: None,
            saving: false,
            enabling: false,
            enable_error: None,
            save_requests: Vec::new(),
            enable_requests: Vec::new(),
        }
    }

    pub fn config(&self) -> &ColumnPresentationDialogConfig {
        &self.config
    }

    /// 换列时替换 props（宿主在 `open_dialog` 之前调；React 是新
    /// props + `[open, columnName]` effect 重建草稿）。
    pub fn set_config(&mut self, config: ColumnPresentationDialogConfig) {
        self.config = config;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open_epoch(&self) -> u64 {
        self.open_epoch
    }

    /// 当前草稿：模式与选项取存储的草稿，IANA 名取两个输入框的实时文本。
    pub fn draft(&self) -> ColumnPresentationDraft {
        let mut draft = self.draft.clone();
        draft.display_iana_name = self.display_iana_name.text.clone();
        draft.source_iana_name = self.source_iana_name.text.clone();
        draft
    }

    pub fn display_iana_name_edit(&mut self) -> &mut TextEdit {
        &mut self.display_iana_name
    }

    pub fn display_iana_name_edit_ref(&self) -> &TextEdit {
        &self.display_iana_name
    }

    pub fn source_iana_name_edit(&mut self) -> &mut TextEdit {
        &mut self.source_iana_name
    }

    pub fn source_iana_name_edit_ref(&self) -> &TextEdit {
        &self.source_iana_name
    }

    /// 测试 / 宿主辅助：不经输入框改 IANA 名。
    pub fn set_display_iana_name(&mut self, name: impl Into<String>) {
        self.display_iana_name = TextEdit::new(name.into());
    }

    /// 测试 / 宿主辅助：不经输入框改源 IANA 名。
    pub fn set_source_iana_name(&mut self, name: impl Into<String>) {
        self.source_iana_name = TextEdit::new(name.into());
    }

    pub fn is_select_open(&self, which: ColumnPresentationSelect) -> bool {
        match which {
            ColumnPresentationSelect::Interpretation => self.interpretation_open,
            ColumnPresentationSelect::Format => self.format_open,
            ColumnPresentationSelect::DisplayTimeZone => self.display_time_zone_open,
            ColumnPresentationSelect::SourceTimeZone => self.source_time_zone_open,
        }
    }

    pub fn set_select_open(&mut self, which: ColumnPresentationSelect, open: bool) {
        match which {
            ColumnPresentationSelect::Interpretation => self.interpretation_open = open,
            ColumnPresentationSelect::Format => self.format_open = open,
            ColumnPresentationSelect::DisplayTimeZone => self.display_time_zone_open = open,
            ColumnPresentationSelect::SourceTimeZone => self.source_time_zone_open = open,
        }
    }

    pub fn toggle_select(&mut self, which: ColumnPresentationSelect) {
        let open = !self.is_select_open(which);
        self.set_select_open(which, open);
    }

    /// Select 选中一个值：先过 S1 的守卫（非法值忽略，与 React 的
    /// `is*` 守卫一致），合法时更新草稿、清错、合上列表框。
    pub fn choose_select(&mut self, which: ColumnPresentationSelect, value: &str) {
        let mut patch = ColumnPresentationDraftPatch::default();
        match which {
            ColumnPresentationSelect::Interpretation => {
                match InterpretationChoice::from_str(value) {
                    Some(choice) => patch.interpretation = Some(choice),
                    None => return,
                }
            }
            ColumnPresentationSelect::Format => match TimestampOutputFormat::from_str(value) {
                Some(format) => patch.format = Some(format),
                None => return,
            },
            ColumnPresentationSelect::DisplayTimeZone => {
                match TimeZoneKindChoice::from_str(value) {
                    Some(kind) => patch.display_time_zone_kind = Some(kind),
                    None => return,
                }
            }
            ColumnPresentationSelect::SourceTimeZone => {
                match SourceTimeZoneChoice::from_str(value) {
                    Some(kind) => patch.source_time_zone_kind = Some(kind),
                    None => return,
                }
            }
        }
        self.set_select_open(which, false);
        self.update_draft(patch);
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn saving(&self) -> bool {
        self.saving
    }

    pub fn enabling(&self) -> bool {
        self.enabling
    }

    pub fn enable_error(&self) -> Option<&str> {
        self.enable_error.as_deref()
    }

    pub fn save_requests(&self) -> &[ColumnPresentationSaveRequest] {
        &self.save_requests
    }

    pub fn enable_requests(&self) -> &[ColumnPresentationEnableRequest] {
        &self.enable_requests
    }

    // ------------------------------------------------------------------
    // 打开 / 关闭 / 草稿
    // ------------------------------------------------------------------

    /// 打开（或换列后重新打开）：用当前覆盖重建草稿，清掉错误与开启
    /// 错误。关闭期间的草稿不保留 —— 下次打开一定重建。
    pub fn open_dialog(&mut self, snapshot: &ValuePresentationSnapshot) {
        let draft = draft_from_column_override(
            &self.effective_current_override(snapshot),
            self.global_options(snapshot).as_ref(),
            self.semantic_kind(),
        );
        self.display_iana_name = TextEdit::new(draft.display_iana_name.clone());
        self.source_iana_name = TextEdit::new(draft.source_iana_name.clone());
        self.draft = draft;
        self.open = true;
        self.open_epoch += 1;
        self.error = None;
        self.enable_error = None;
    }

    /// 关闭：草稿不重置（下次打开重建覆盖），与 React 的关闭后
    /// `useEffect` 不运行、草稿留待下次重建一致。
    pub fn close(&mut self) {
        self.open = false;
    }

    /// React 的 `updateDraft`：合并不含 IANA 名的补丁，并清掉错误。
    pub fn update_draft(&mut self, patch: ColumnPresentationDraftPatch) {
        if let Some(mode) = patch.mode {
            self.draft.mode = mode;
        }
        if let Some(interpretation) = patch.interpretation {
            self.draft.interpretation = interpretation;
        }
        if let Some(format) = patch.format {
            self.draft.format = format;
        }
        if let Some(kind) = patch.display_time_zone_kind {
            self.draft.display_time_zone_kind = kind;
        }
        if let Some(kind) = patch.source_time_zone_kind {
            self.draft.source_time_zone_kind = kind;
        }
        self.error = None;
    }

    // ------------------------------------------------------------------
    // 派生
    // ------------------------------------------------------------------

    /// 列的计划输入（名 + 类型 + hints）。
    pub fn plan_column(&self) -> PresentationPlanColumn {
        PresentationPlanColumn {
            name: self.config.column_name.clone(),
            data_type: self.config.column_type.clone(),
            hints: self.config.cell_type_hints.clone(),
        }
    }

    /// 语义种：Rune 侧尚无 `resolvePresentationSemanticKind` 的独立
    /// 任务，推导直接做在这里（`CellType` / `CellTypeHints` → 种，
    /// 见 S1 模块头的接线口）。
    pub fn semantic_kind(&self) -> PresentationSemanticKind {
        resolve_presentation_semantic_kind(
            self.config.column_type.as_deref(),
            self.config.cell_type_hints.as_ref(),
        )
    }

    /// 当前覆盖；缺省时从快照取。
    pub fn effective_current_override(
        &self,
        snapshot: &ValuePresentationSnapshot,
    ) -> ColumnPresentationOverride {
        match &self.config.current_override {
            Some(override_) => override_.clone(),
            None => column_override_from_snapshot(
                snapshot,
                self.config.source.as_ref(),
                &self.config.column_name,
            ),
        }
    }

    /// 全局默认时间选项（快照 `configs` 里 `datazen.timestamp` 一项，
    /// 非法时缺省）。
    pub fn global_options(
        &self,
        snapshot: &ValuePresentationSnapshot,
    ) -> Option<TimestampOptions> {
        global_timestamp_options_from_snapshot(snapshot)
    }

    /// 总开关是否启用了时间显示。
    pub fn time_enabled(&self, snapshot: &ValuePresentationSnapshot) -> bool {
        snapshot
            .enabled_renderer_ids
            .iter()
            .any(|id| id == TIMESTAMP_CONTRIBUTION_ID)
    }

    /// 时间展示贡献是否在场（T11：对话框每次 `build` 都重读注册
    /// 中心，停用/重启用即时反映，见计划层模块头）。
    pub fn contribution_missing(&self, registry: &ValuePresentationRegistry) -> bool {
        registry.get(TIMESTAMP_CONTRIBUTION_ID).is_none()
    }

    /// 预览样本：调用方给真实行值，缺省时按语义种给 PRD 双锚点样本；
    /// `has_sample` 为 `sample !== null || sampleValue 已给`（TS 原式）。
    pub fn sample(&self) -> (PresentationValue, bool) {
        let sample = match &self.config.sample_value {
            Some(value) => value.clone(),
            None => {
                fallback_sample_for_column(&self.plan_column()).unwrap_or(PresentationValue::Null)
            }
        };
        let has_sample = sample != PresentationValue::Null || self.config.sample_value.is_some();
        (sample, has_sample)
    }

    /// 样本值的展示文本（预览框「原值」）。
    pub fn sample_text(&self) -> String {
        let (sample, _) = self.sample();
        format_column_presentation_sample_value(&sample)
    }

    /// 草稿 → 列覆盖（保存与预览共用同一候选）。
    pub fn candidate(&self) -> ColumnPresentationOverride {
        draft_to_column_override(&self.draft())
    }

    /// 草稿校验（保存与预览共用）：`is_known_time_zone` 由宿主提供
    /// 平台检查（S1 接线口）。
    pub fn draft_problems(
        &self,
        is_known_time_zone: &dyn Fn(&str) -> bool,
    ) -> Vec<ValidationProblem> {
        validate_column_presentation_draft(&self.draft(), is_known_time_zone)
    }

    /// 预览（原值与结果同时显示；无时间引擎时 `ready` 按
    /// `plan-error` 回退，见计划层模块头）。
    pub fn preview(
        &self,
        snapshot: &ValuePresentationSnapshot,
        registry: &ValuePresentationRegistry,
        presenter: Option<&TimestampPresenter>,
    ) -> ColumnPresentationPreview {
        let (sample, _) = self.sample();
        compute_column_presentation_preview(
            &self.candidate(),
            &self.plan_column(),
            self.config.source.as_ref(),
            snapshot,
            sample,
            registry,
            presenter,
        )
    }

    /// 当前生效计划（inherit 预览与生效来源提示共用同一计划）。
    /// 草稿为 `inherit` 时按「清除覆盖后」建计划（T09-BUG-001）；
    /// custom / raw 草稿的来源本就是列覆盖，标签不依赖该计划。
    pub fn current_plan(
        &self,
        snapshot: &ValuePresentationSnapshot,
        registry: &ValuePresentationRegistry,
        presenter: Option<&TimestampPresenter>,
    ) -> ColumnPresentationPlan {
        let snapshot_for_plan = if self.draft().mode == PresentationMode::Inherit {
            snapshot_without_column_override(
                snapshot,
                self.config.source.as_ref(),
                &self.config.column_name,
            )
        } else {
            snapshot.clone()
        };
        build_column_presentation_plan(
            &self.plan_column(),
            self.config.source.as_ref(),
            &snapshot_for_plan,
            registry,
            presenter,
        )
    }

    /// 生效来源标签（T09-BUG-001）：custom / raw 保存后即为列覆盖；
    /// inherit 保存后清除覆盖，来源为剥离后的计划所命中的规则或全
    /// 局默认——不看保存前的 `effectiveCurrentOverride`。
    pub fn effective_source_label(&self, plan: &ColumnPresentationPlan) -> String {
        match self.draft().mode {
            PresentationMode::Custom | PresentationMode::Raw => {
                t("columnPresentation.source.override").to_owned()
            }
            PresentationMode::Inherit => match plan.matched_rule_name() {
                Some(name) => t("columnPresentation.source.rule").replace("{name}", name),
                None => t("columnPresentation.source.global").to_owned(),
            },
        }
    }

    /// 源时区选择只在 text / native 解释下显示。
    pub fn show_source_time_zone(&self) -> bool {
        matches!(
            self.draft().interpretation,
            InterpretationChoice::Text | InterpretationChoice::Native
        )
    }

    /// 保存禁用：无来源列（`source` 缺省）时禁用并说明原因。
    pub fn save_disabled(&self) -> bool {
        self.config.source.is_none()
    }

    // ------------------------------------------------------------------
    // 保存
    // ------------------------------------------------------------------

    /// React 的 `handleSave`（对话框侧；持久化由宿主排空 outbox
    /// 执行，见模块头）：
    /// - `source` 缺省时直接返回（按钮已禁用，防御性复核）；
    /// - 草稿问题非空与二次校验失败时就地报错、不关闭；
    /// - 有 `onSave` 时只记回调请求、不碰持久化，记录后立即关闭；
    /// - 默认路径记持久化请求、置 `saving`，等宿主回答。
    pub fn save(&mut self, is_known_time_zone: &dyn Fn(&str) -> bool) {
        if self.saving {
            return;
        }
        let Some(source) = self.config.source.clone() else {
            return;
        };
        let problems = self.draft_problems(is_known_time_zone);
        if !problems.is_empty() {
            self.error = Some(format!(
                "{}: {}",
                t("settings.valuePresentation.invalidConfig"),
                format_problems(&problems)
            ));
            return;
        }
        let candidate = self.candidate();
        let validated = match validate_column_presentation_override(&candidate.to_json()) {
            Ok(override_) => override_,
            Err(problems) => {
                self.error = Some(format!(
                    "{}: {}",
                    t("settings.valuePresentation.invalidConfig"),
                    format_problems(&problems)
                ));
                return;
            }
        };
        if self.config.has_on_save {
            self.save_requests
                .push(ColumnPresentationSaveRequest::OnSave(validated));
            self.close();
            return;
        }
        self.saving = true;
        self.error = None;
        self.save_requests
            .push(ColumnPresentationSaveRequest::Persist {
                source,
                column_name: self.config.column_name.clone(),
                column_override: validated,
            });
    }

    /// 宿主的回答：保存成功 —— 快照即持久值，关闭面板。
    pub fn save_succeeded(&mut self) {
        self.saving = false;
        self.close();
    }

    /// 宿主的回答：保存失败 —— 就地报错、面板保持打开、会话条目
    /// 保留、旧配置继续生效（后三者由宿主的保存链路保证），用户可重试。
    pub fn save_failed(&mut self) {
        self.saving = false;
        self.error = Some(t("settings.valuePresentation.saveFailed").to_owned());
    }

    // ------------------------------------------------------------------
    // 开启总开关
    // ------------------------------------------------------------------

    /// 默认开启用的 options：沿用快照里的合法全局值，否则默认
    /// native / iso / local。
    pub fn default_enable_options(
        &self,
        snapshot: &ValuePresentationSnapshot,
    ) -> TimestampOptions {
        self.global_options(snapshot).unwrap_or(TimestampOptions {
            interpretation: TimestampInterpretation::Native,
            format: TimestampOutputFormat::Iso,
            display_time_zone: TimeZoneSpec::Local,
            source_time_zone: None,
        })
    }

    /// React 的 `handleEnableTimeDisplay`（对话框侧；写 store 由宿
    /// 主排空 outbox 执行）：
    /// - 有 `onEnableTimeDisplay` 时只记回调请求；
    /// - 默认路径只把 `datazen.timestamp` 置 enabled（updater 形式、
    ///   只改这一项，options 见 [`ColumnPresentationDialogState::default_enable_options`]）；
    /// - 保存列配置本身绝不走这条路。
    pub fn enable_time_display(&mut self, snapshot: &ValuePresentationSnapshot) {
        let options = self.default_enable_options(snapshot);
        self.enable_time_display_with(options);
    }

    /// [`ColumnPresentationDialogState::enable_time_display`] 的后半段：
    /// options 由调用方在按压时从当前快照算好传入（视图的按压闭包
    /// 是 `'static` 的，借不到每帧的快照；状态方法本身仍从快照算，
    /// 两处是同一 options）。
    pub fn enable_time_display_with(&mut self, options: TimestampOptions) {
        if self.enabling {
            return;
        }
        self.enabling = true;
        self.enable_error = None;
        if self.config.has_on_enable_time_display {
            self.enable_requests
                .push(ColumnPresentationEnableRequest::OnEnable);
            return;
        }
        self.enable_requests
            .push(ColumnPresentationEnableRequest::Persist { options });
    }

    /// 宿主的回答：开启成功 —— 只清 `enabling`（面板不关闭）。
    pub fn enable_succeeded(&mut self) {
        self.enabling = false;
    }

    /// 宿主的回答：开启失败 —— 就地报错，不假装成功。
    pub fn enable_failed(&mut self) {
        self.enabling = false;
        self.enable_error = Some(t("settings.valuePresentation.saveFailed").to_owned());
    }
}

/// 问题列表的就地报错文本：`path: message; path: message`（与
/// React 的 `handleSave` / 预览 invalid 行的拼接一致）。
fn format_problems(problems: &[ValidationProblem]) -> String {
    problems
        .iter()
        .map(|problem| format!("{}: {}", problem.path, problem.message))
        .collect::<Vec<_>>()
        .join("; ")
}

/// 对话框默认的 IANA 名占位（S1 的 `DEFAULT_DISPLAY_IANA_NAME`，
/// 视图的输入框 placeholder 用）。
pub const DISPLAY_IANA_PLACEHOLDER: &str = DEFAULT_DISPLAY_IANA_NAME;
