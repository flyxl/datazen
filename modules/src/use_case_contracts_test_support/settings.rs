//! settings 用例的双传输规格：binding 端口、共享夹具与三个 scenario。
//!
//! 规格本体（本文件除 `baseline` 外的一切）不感知 IPC/HTTP 词汇：
//! scenario 只经 [`SettingsContractBinding`] 触达用例，入参出参与断言
//! 全部落在 `datazen::settings` 的 application 词汇上。两条宿主腿与
//! 无宿主基线各自实现同一个 binding：
//!
//! - 基线（本文件 `baseline`，`cfg(test)`）：直调 `SettingsService`——
//!   宿主整体换掉后规格仍原样过，是 commands-layer-review §1 判据的本体；
//! - HTTP 腿：`server/tests/use_case_contracts.rs`（http-api-contract §6.1
//!   端点形态）；
//! - IPC 腿：桌面 `commands/settings_use_case_contract_tests.rs`
//!   （真实 `#[tauri::command]` 入口）。
//!
//! 三腿的 scenario 测试与本文件 scenario 函数**逐字同名**，绿名单逐条
//! 对得上——「两腿同绿」因此可核对到每条用例。
//!
//! # 本域的错误词汇：无契约码
//!
//! `SettingsError` 不携带契约码（`Validation` / `Repository` 都没有
//! `code()`）——「无码」本身就是两腿一致的契约事实：[`ContractFailure`]
//! 的 `code` 恒为 `None`，`message` 仍须携带 application 文本（校验拒绝
//! 是 application 原句逐字；存储失败的文本属各腿装配裁决——桌面宿主按
//! D1 把仓储细节换成固定句，HTTP 腿原样透传——scenario 只断言「文本在
//! 场」，各腿的确切文本由各自装配层 pin 钉住）。

use async_trait::async_trait;

use crate::settings::{
    ExecutionSettings, MonitorSettings, PortableThemePreference, SettingsError,
    SettingsRepository, SettingsRepositoryError, SharedSettings, SqlFormatOptions, UserPreferences,
};
use crate::use_case_contracts_test_support::{ContractFailure, ContractResult};

// ── 传输装配端口 ─────────────────────────────────────────────────────────

/// settings 用例的传输装配端口（binding harness）。
///
/// 每条腿实现一次：`seed` / `fail_writes` 是 setup——把夹具记录**直写**
/// 进本腿装配的存储（不经用例，对应生产的「存储里已有这些行 / 落盘已坏」）；
/// 两个用例方法则经本腿的真实传输（tauri command / axum handler）触达
/// application。规格对实现方式零假设。
///
/// `?Send` 与 named_views 域同一裁决：binding 由单个测试的栈持有、
/// scenario 顺序 await，规格不假设跨线程；生产端口的并发纪律
/// （`Send + Sync`）属于 application/infra 端口，不属于本测试装配端口。
#[async_trait(?Send)]
pub trait SettingsContractBinding {
    /// setup：夹具设置直写本腿存储。
    async fn seed(&self, settings: SharedSettings) -> Result<(), String>;

    /// 第二个 setup 词汇：把存储打进**写失败态**（读仍照常可用）——存储
    /// 失败路径的舞台。失败细节文本由各腿仓储决定，规格不断言其内容。
    async fn fail_writes(&self) -> Result<(), String>;

    /// `read` 用例（GET /api/v1/settings 与 `get_settings` 的用例半边）
    /// 经本腿传输。
    async fn get(&self) -> ContractResult<SharedSettings>;

    /// `save` 用例（PUT /api/v1/settings 与 `save_settings` 的用例半边）
    /// 经本腿传输。
    async fn save(&self, settings: SharedSettings) -> ContractResult<SharedSettings>;
}

// ── 共享夹具（setup 材料）─────────────────────────────────────────────────

/// 内存仓储假体：单文档读写 + 写失败开关，别无其它（与
/// `settings::application` 测试里的 `RecordingRepository` 同源语义；那份
/// 假体住在该文件的 `cfg(test)` 私有模块里无法 re-export，按模板第 2 步
/// 在这里建规格自带的这一份，语义保持同源——含同一条「写失败 →
/// `BackendUnavailable`」路径）。
#[derive(Default)]
pub struct MemorySettingsStore {
    stored: std::sync::Mutex<Option<SharedSettings>>,
    app_blob: std::sync::Mutex<serde_json::Value>,
    writes_fail: std::sync::Mutex<bool>,
}

impl MemorySettingsStore {
    /// setup 直写：替换整份共享设置。
    pub async fn seed(&self, settings: SharedSettings) {
        *self.stored.lock().expect("memory settings store lock") = Some(settings);
    }

    /// setup 直改：后续 `write_settings` 一律报
    /// `BackendUnavailable("write failed")`，读路径不受影响。
    pub async fn fail_writes(&self) {
        *self.writes_fail.lock().expect("memory settings store lock") = true;
    }
}

#[async_trait]
impl SettingsRepository for MemorySettingsStore {
    async fn read_settings(&self) -> Result<SharedSettings, SettingsRepositoryError> {
        self.stored
            .lock()
            .expect("memory settings store lock")
            .clone()
            .ok_or_else(|| {
                SettingsRepositoryError::BackendUnavailable("settings not seeded".into())
            })
    }

    async fn write_settings(
        &self,
        settings: SharedSettings,
    ) -> Result<SharedSettings, SettingsRepositoryError> {
        if *self.writes_fail.lock().expect("memory settings store lock") {
            return Err(SettingsRepositoryError::BackendUnavailable("write failed".into()));
        }
        *self.stored.lock().expect("memory settings store lock") = Some(settings.clone());
        Ok(settings)
    }

    async fn read_app_settings(&self) -> Result<serde_json::Value, SettingsRepositoryError> {
        Ok(self
            .app_blob
            .lock()
            .expect("memory settings store lock")
            .clone())
    }

    async fn write_app_settings(
        &self,
        settings: serde_json::Value,
    ) -> Result<serde_json::Value, SettingsRepositoryError> {
        *self.app_blob.lock().expect("memory settings store lock") = settings.clone();
        Ok(settings)
    }
}

/// 夹具**全量合法**设置：整份共享文档的稳定起点，供 `seed` 直写与 `save`
/// 组装 request（scenario 在克隆体上改单个字段，改的全是 normalize/
/// 校验/往返真正关心的那几个）。
pub fn contract_settings() -> SharedSettings {
    SharedSettings {
        preferences: UserPreferences {
            theme: PortableThemePreference {
                mode: "dark".into(),
                pack_id: None,
            },
            language: "en-US".into(),
            editor_font_size: 14,
            editor_font_family: "monospace".into(),
            confirm_on_delete: true,
            auto_commit: true,
            confirm_dangerous_execution: true,
            default_page_size: 100,
            auto_chart_on_query: false,
            wapp_settings: serde_json::Map::new(),
            editor_completion_quote_policy: "unquoted".into(),
            editor_completion_include_table_prefix: true,
            enable_fk_prediction: false,
            keymap_preset: "default".into(),
            custom_keymap: std::collections::HashMap::new(),
            sql_execution_strategy: "current_statement".into(),
            sql_syntax_theme: None,
            sql_format_options: SqlFormatOptions::default(),
            sql_snippets: Vec::new(),
            workflow_step_result_order: "desc".into(),
        },
        execution: ExecutionSettings {
            limit_select_results: true,
            query_result_limit: 1000,
            safe_mode: true,
            connection_pool_size: 10,
            mcp_disabled_tools: Vec::new(),
            mcp_permission_mode: "safe_write".into(),
            mcp_allowed_connection_ids: Vec::new(),
            driver_settings: serde_json::Map::new(),
            ai_strict_egress: true,
        },
        monitor: MonitorSettings::default(),
        onboarding: None,
        row_count_mode: "auto".into(),
    }
}

/// application 错误 → 形态中立观测（基线腿的装配映射；两条宿主腿各自的
/// 映射必须产出与这里相同的 code——恒 `None`——词汇）。
pub fn application_failure(error: SettingsError) -> ContractFailure {
    // settings 域没有契约码：`SettingsError` 不携带 code，「无码」本身就是
    // 两腿一致的契约事实（模块头「本域的错误词汇」）。
    ContractFailure::from_application(None, error.to_string())
}

// ── scenarios（规格本体：setup → request → 断言）──────────────────────────

/// 正常路径：save→get 往返——保存返回**normalize 后的记录**（越界的池
/// 尺寸被夹回上限、改动进文档），且经用例可原样读回：返回值即存储事实，
/// 两腿一致。
pub async fn saving_settings_normalizes_and_round_trips(
    binding: &dyn SettingsContractBinding,
) {
    binding
        .seed(contract_settings())
        .await
        .expect("seed valid settings");

    let mut draft = contract_settings();
    draft.preferences.language = "zh-CN".into();
    draft.execution.connection_pool_size = 1000;
    let saved = binding.save(draft).await.expect("valid settings save");

    assert_eq!(
        saved.preferences.language, "zh-CN",
        "the change travels into the saved document"
    );
    assert_eq!(
        saved.execution.connection_pool_size, 100,
        "the answer is the normalized fact, not the raw request"
    );

    let read_back = binding.get().await.expect("read after save");
    assert_eq!(
        read_back, saved,
        "the storage serves exactly what the save answered"
    );
}

/// 错误路径（校验拒绝族）：不满足共享枚举规则的设置在**落库前**被拒；
/// 拒绝无契约码（本域词汇），application 原句逐字随两腿走，存量经用例
/// 读回不受影响。
pub async fn invalid_enum_values_are_refused_before_the_repository(
    binding: &dyn SettingsContractBinding,
) {
    binding
        .seed(contract_settings())
        .await
        .expect("seed valid settings");
    let before = binding.get().await.expect("read the seeded settings");

    let mut invalid = contract_settings();
    invalid.preferences.theme.mode = "neon".into();
    let failure = binding
        .save(invalid)
        .await
        .expect_err("an invalid enum value is refused");
    assert_eq!(
        failure.code, None,
        "settings failures carry no contract code — the absence agrees on both legs"
    );
    assert_eq!(
        failure.message, "Invalid settings enum value",
        "the application sentence travels verbatim"
    );

    let still = binding.get().await.expect("read after the refusal");
    assert_eq!(
        still, before,
        "the refusal happened before the write"
    );
}

/// 错误路径（存储失败族）：写路径故障以**无码**失败浮出——「无码」是
/// 两腿一致的契约事实，application 文本仍然在场（确切文本属各腿装配
/// 裁决，由各自 pin 钉住）；存量经用例读回不受牵连。
pub async fn a_storage_failure_surfaces_without_a_contract_code(
    binding: &dyn SettingsContractBinding,
) {
    binding
        .seed(contract_settings())
        .await
        .expect("seed valid settings");
    let before = binding.get().await.expect("read the seeded settings");
    binding
        .fail_writes()
        .await
        .expect("the storage enters its write-failure state");

    let failure = binding
        .save(contract_settings())
        .await
        .expect_err("a failed storage refuses the write");
    assert_eq!(
        failure.code, None,
        "storage failures carry no contract code — the absence agrees on both legs"
    );
    assert!(
        !failure.message.trim().is_empty(),
        "an application text still travels without a code: {}",
        failure.message
    );

    let still = binding.get().await.expect("read after the refusal");
    assert_eq!(
        still, before,
        "the refused write did not touch the stored facts"
    );
}

// ── 无宿主基线（判据本体：宿主整体换掉，规格原样过）───────────────────────

#[cfg(test)]
mod baseline {
    use super::*;
    use crate::settings::SettingsService;
    use std::sync::Arc;

    /// 无传输绑定：直调 `SettingsService`。实现与本文件 scenario 用的
    /// 是同一端口、同一夹具——这就是「把宿主从 Tauri 换成 axum，测试
    /// 还能原样过」的极限形态（连宿主都没有）。
    struct DirectServiceBinding {
        service: SettingsService,
        store: Arc<MemorySettingsStore>,
    }

    #[async_trait(?Send)]
    impl SettingsContractBinding for DirectServiceBinding {
        async fn seed(&self, settings: SharedSettings) -> Result<(), String> {
            self.store.seed(settings).await;
            Ok(())
        }

        async fn fail_writes(&self) -> Result<(), String> {
            self.store.fail_writes().await;
            Ok(())
        }

        async fn get(&self) -> ContractResult<SharedSettings> {
            self.service.read().await.map_err(application_failure)
        }

        async fn save(&self, settings: SharedSettings) -> ContractResult<SharedSettings> {
            self.service.save(settings).await.map_err(application_failure)
        }
    }

    fn harness() -> DirectServiceBinding {
        let store = Arc::new(MemorySettingsStore::default());
        let service = SettingsService::new(store.clone());
        DirectServiceBinding { service, store }
    }

    #[tokio::test]
    async fn saving_settings_normalizes_and_round_trips() {
        super::saving_settings_normalizes_and_round_trips(&harness()).await;
    }

    #[tokio::test]
    async fn invalid_enum_values_are_refused_before_the_repository() {
        super::invalid_enum_values_are_refused_before_the_repository(&harness()).await;
    }

    #[tokio::test]
    async fn a_storage_failure_surfaces_without_a_contract_code() {
        super::a_storage_failure_surfaces_without_a_contract_code(&harness()).await;
    }

    /// 基线装配 pin：仓储失败的 application 文本逐字（`backend unavailable:
    /// write failed`，`#[error(transparent)]` 链到仓储错误）。宿主腿的映射
    /// 不得偷换这个词汇族：HTTP 原样进 JSON 体（pin 在 server 腿），IPC 换
    /// 成宿主固定句（pin 在桌面腿——D1 纪律，与本钉相对照）。
    #[tokio::test]
    async fn the_repository_failure_text_travels_verbatim() {
        let binding = harness();
        binding.seed(contract_settings()).await.expect("seed");
        binding.fail_writes().await.expect("fail writes");

        let failure = binding
            .save(contract_settings())
            .await
            .expect_err("writes fail");
        assert_eq!(failure.code, None);
        assert_eq!(failure.message, "backend unavailable: write failed");
    }
}
