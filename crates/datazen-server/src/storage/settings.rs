//! `SettingsRepository` 的服务端适配器（契约 §6.1 settings 读侧面的存储半边）。
//!
//! 端口（`datazen::settings` 的 [`SettingsRepository`]）**没有** scope 参数
//! ——与 v2 `known_hosts` 同一条登记口径：组织隔离由宿主按「每组织一个
//! 仓储实例」实现，[`ServerSettingsStore`] 以构造参数携带组织边界（
//! [`ServerSettingsStore::scoped`]，HTTP handler 按会话组织的 `ctx` 逐请求
//! 装配；INV-01：组织只能来自认证链事实，不来自请求体）。
//!
//! 存储形状（v8 `server_settings`，per-section 行）：
//!
//! - `section` = `SharedSettings` 序列化后的**顶层 wire 键**
//!   （`preferences` / `execution` / `monitor` / `onboarding` /
//!   `rowCountMode`——serde `rename_all = "camelCase"` 的产出，存储键与
//!   wire 键同字面，读取期直接拼对象再反序列化，不做第二份键位映射）。
//!   写入按整档覆盖（last-write-wins，契约 §6.1：不要求幂等令牌），
//!   事务内先删掉不在新文档键集合里的旧行（前向兼容：文档缩节不留孤儿）。
//! - **缺行 ≠ 错误**：读取按节补 [`default_shared_settings`] 的缺省值
//!   ——「fresh install 语义」与桌面 `AppSettings::default_for_first_run`
//!   同位。缺省文档镜像前端
//!   `packages/frontend/src/stores/settingsStore.ts` 的 `DEFAULT_SETTINGS`
//!   共享投影（theme `DEFAULT_THEME_PREFERENCE`、字体
//!   `HOST_DEFAULT_EDITOR_FONT`、`rowCountMode` `DEFAULT_ROW_COUNT_MODE`、
//!   monitor `DEFAULT_MONITOR_SETTINGS`——与 Rust `MonitorSettings::default`
//!   逐字段一致）。桌面专属分节（updater / logPath / grid 外观等宿主字段）
//!   **不在**缺省文档里，也就永不出现在 HTTP 面（契约 §6.1）。
//! - 端口的 `read_app_settings` / `write_app_settings`（宿主持有的整档
//!   blob，桌面 `AppSettings` 词汇）以 `section = 'app'` 一行承接：服务端
//!   形态没有桌面宿主，HTTP §6.1 面不暴露它；实现它只为端口完备（键集合
//!   与共享分节天然隔离）。
//!
//! 异步边界：与 `storage::profiles` 同款——同步核心 + `spawn_blocking` 包装。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use rusqlite::{params, OptionalExtension};

use datazen::settings::{
    ExecutionSettings, MonitorSettings, PortableThemePreference, SettingsRepository,
    SettingsRepositoryError, SharedSettings, SqlFormatOptions, UserPreferences,
};

use crate::storage::AdminStore;

/// 宿主 blob（`read_app_settings` / `write_app_settings`）的 section 键。
/// 不与 `SharedSettings` 的顶层 wire 键重叠。
const APP_DOCUMENT_SECTION: &str = "app";

/// 共享设置文档的**缺省值**（fresh install 语义；镜像前端 `DEFAULT_SETTINGS`
/// 的共享投影，见模块头）。枚举值全部落在 `SettingsService::normalize` 的
/// 合法集合内——缺省文档自身可过 normalize 往返。
pub fn default_shared_settings() -> SharedSettings {
    SharedSettings {
        preferences: UserPreferences {
            theme: PortableThemePreference {
                mode: "dark".into(),
                pack_id: None,
            },
            language: "en".into(),
            editor_font_size: 13,
            editor_font_family: "Menlo, Monaco, Consolas, \"Courier New\", monospace".into(),
            confirm_on_delete: true,
            auto_commit: true,
            confirm_dangerous_execution: true,
            default_page_size: 50,
            auto_chart_on_query: false,
            wapp_settings: serde_json::Map::new(),
            editor_completion_quote_policy: "unquoted".into(),
            editor_completion_include_table_prefix: true,
            enable_fk_prediction: false,
            keymap_preset: "default".into(),
            custom_keymap: HashMap::new(),
            sql_execution_strategy: "current_statement".into(),
            sql_syntax_theme: None,
            sql_format_options: SqlFormatOptions::default(),
            sql_snippets: Vec::new(),
            workflow_step_result_order: "desc".into(),
        },
        execution: ExecutionSettings {
            limit_select_results: false,
            query_result_limit: 5000,
            safe_mode: true,
            connection_pool_size: 10,
            mcp_disabled_tools: Vec::new(),
            mcp_permission_mode: "safe_write".into(),
            mcp_allowed_connection_ids: Vec::new(),
            mcp_allowed_databases: Vec::new(),
            mcp_allowed_namespaces: Vec::new(),
            driver_settings: serde_json::Map::new(),
            ai_strict_egress: true,
        },
        monitor: MonitorSettings::default(),
        onboarding: None,
        row_count_mode: "auto".into(),
    }
}

fn backend(error: impl std::fmt::Display) -> SettingsRepositoryError {
    SettingsRepositoryError::BackendUnavailable(error.to_string())
}

fn now_string() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// 每组织一个实例的设置存储适配器。
#[derive(Clone)]
pub struct ServerSettingsStore {
    store: Arc<AdminStore>,
    organization_id: String,
}

impl ServerSettingsStore {
    /// 在已迁移的管理库上构造（调用方负责先跑 `AdminStore::migrate`——v8
    /// 起有 `server_settings`）。
    pub fn new(store: Arc<AdminStore>, organization_id: String) -> Self {
        Self {
            store,
            organization_id,
        }
    }

    /// 按请求上下文的组织装配（HTTP handler 的唯一构造入口；组织来自
    /// 认证链盖章的 `ctx`，INV-01）。
    pub fn scoped(organization_id: &str, store: Arc<AdminStore>) -> Self {
        Self::new(store, organization_id.to_string())
    }

    fn sync_read_sections(
        &self,
    ) -> Result<HashMap<String, serde_json::Value>, SettingsRepositoryError> {
        let conn = self
            .store
            .lock_for_adapter()
            .map_err(|error| backend(error))?;
        let mut stmt = conn
            .prepare(
                "SELECT section, settings_json FROM server_settings
                 WHERE organization_id = ?1",
            )
            .map_err(backend)?;
        let rows = stmt
            .query_map([&self.organization_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(backend)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(backend)?;
        let mut sections = HashMap::new();
        for (section, json) in rows {
            let value: serde_json::Value = serde_json::from_str(&json).map_err(|error| {
                backend(format!("stored section {section} is corrupt: {error}"))
            })?;
            sections.insert(section, value);
        }
        Ok(sections)
    }

    fn sync_write_shared(&self, settings: &SharedSettings) -> Result<(), SettingsRepositoryError> {
        // 文档 → 值树：顶层键即 section 键（serde camelCase 产出，与读取
        // 拼装同字面）。
        let document = serde_json::to_value(settings)
            .map_err(|error| backend(format!("settings document is not serializable: {error}")))?;
        let Some(sections) = document.as_object() else {
            return Err(backend(
                "settings document is not a JSON object".to_string(),
            ));
        };
        let at = now_string();
        let mut conn = self
            .store
            .lock_for_adapter()
            .map_err(|error| backend(error))?;
        let tx = conn.transaction().map_err(backend)?;
        for (section, value) in sections {
            let json = serde_json::to_string(value).map_err(|error| {
                backend(format!("section {section} is not serializable: {error}"))
            })?;
            tx.execute(
                "INSERT INTO server_settings
                   (organization_id, section, settings_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT(organization_id, section) DO UPDATE SET
                   settings_json = excluded.settings_json,
                   updated_at = excluded.updated_at",
                params![self.organization_id, section, json, at],
            )
            .map_err(backend)?;
        }
        // 前向兼容：「整档覆盖写」的字面语义——新文档没有的节不留孤儿行。
        // 现存节先查后删（逐行绑定参数，不做 SQL 拼接）。
        let stale: Vec<String> = {
            let mut stmt = tx
                .prepare("SELECT section FROM server_settings WHERE organization_id = ?1")
                .map_err(backend)?;
            let rows = stmt
                .query_map([&self.organization_id], |row| row.get::<_, String>(0))
                .map_err(backend)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(backend)?;
            rows.into_iter()
                .filter(|section| !sections.contains_key(section))
                .collect()
        };
        for section in stale {
            tx.execute(
                "DELETE FROM server_settings WHERE organization_id = ?1 AND section = ?2",
                params![self.organization_id, section],
            )
            .map_err(backend)?;
        }
        tx.commit().map_err(backend)?;
        Ok(())
    }

    fn sync_write_app_document(
        &self,
        document: &serde_json::Value,
    ) -> Result<(), SettingsRepositoryError> {
        let json = serde_json::to_string(document).map_err(|error| {
            backend(format!(
                "app settings document is not serializable: {error}"
            ))
        })?;
        let at = now_string();
        let conn = self
            .store
            .lock_for_adapter()
            .map_err(|error| backend(error))?;
        conn.execute(
            "INSERT INTO server_settings
               (organization_id, section, settings_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(organization_id, section) DO UPDATE SET
               settings_json = excluded.settings_json,
               updated_at = excluded.updated_at",
            params![self.organization_id, APP_DOCUMENT_SECTION, json, at],
        )
        .map_err(backend)?;
        Ok(())
    }
}

/// 缺行按节补缺省（fresh install 语义，见模块头）：以缺省文档为底、
/// 已存节逐键覆盖，再整档反序列化回 `SharedSettings`（存储被篡改的节
/// 在这里先行失败，不外溢成 wire 形状错误）。
fn assemble(
    sections: HashMap<String, serde_json::Value>,
) -> Result<SharedSettings, SettingsRepositoryError> {
    let fallback = serde_json::to_value(default_shared_settings())
        .map_err(|error| backend(format!("default settings are not serializable: {error}")))?;
    let mut document = match fallback {
        serde_json::Value::Object(map) => map,
        _ => {
            return Err(backend(
                "default settings are not a JSON object".to_string(),
            ))
        }
    };
    for (section, value) in sections {
        // app blob 不属于共享文档词汇，跳过（`read_app_settings` 另取）。
        if section == APP_DOCUMENT_SECTION {
            continue;
        }
        document.insert(section, value);
    }
    serde_json::from_value(serde_json::Value::Object(document)).map_err(|error| {
        backend(format!(
            "stored settings do not fit the shared contract: {error}"
        ))
    })
}

#[async_trait]
impl SettingsRepository for ServerSettingsStore {
    async fn read_settings(&self) -> Result<SharedSettings, SettingsRepositoryError> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || assemble(this.sync_read_sections()?))
            .await
            .map_err(|source| backend(format!("settings read task failed: {source}")))?
    }

    async fn write_settings(
        &self,
        settings: SharedSettings,
    ) -> Result<SharedSettings, SettingsRepositoryError> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            this.sync_write_shared(&settings)?;
            Ok(settings)
        })
        .await
        .map_err(|source| backend(format!("settings write task failed: {source}")))?
    }

    async fn read_app_settings(&self) -> Result<serde_json::Value, SettingsRepositoryError> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            let sections = this.sync_read_sections()?;
            Ok(sections
                .get(APP_DOCUMENT_SECTION)
                .cloned()
                .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new())))
        })
        .await
        .map_err(|source| backend(format!("app settings read task failed: {source}")))?
    }

    async fn write_app_settings(
        &self,
        settings: serde_json::Value,
    ) -> Result<serde_json::Value, SettingsRepositoryError> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            this.sync_write_app_document(&settings)?;
            Ok(settings)
        })
        .await
        .map_err(|source| backend(format!("app settings write task failed: {source}")))?
    }
}
