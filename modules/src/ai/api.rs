//! AI feature 的宿主面用例（host-boundary §3；F4a `connection::api` 先例）。
//!
//! 入参是已构造好的 registry / [`AiConfigStore`] 端口 / 请求 DTO，不依赖
//! Tauri、Axum 或 `AppState`。供应商懒注册、持久化配置恢复、保存时的
//! initialize 顺序、删除时的 reset 顺序——这些编排全部在这里定稿；桌面
//! 宿主在 `commands/ai` 只保留传输胶水（`ai:config-changed` 事件发射、
//! 命令名日志），Web 宿主以后复用同一批用例函数而不必再抄一份编排。
//!
//! 错误用 AI 用例信封 [`AiUseCaseError`]；桌面在 `commands::error` 持有
//! 唯一的 `AiUseCaseError → CommandError` 映射表（同桶映射，IPC 文案与
//! 失败类别不变），Web 宿主以后自持一份 HTTP 版映射。
//!
//! DTO 复用 domain 的 `AiProviderConfig` / `AiSettingsConfig` / `ModelInfo`，
//! 不另定义一份；[`ProviderListItem`] 是供应商列表用例的展示 DTO，归
//! 模块所有（序列化形状与旧宿主定义逐字段一致，前端手写 TS 接口不变）。

use std::sync::Arc;

use super::application::ports::AiConfigStore;
use super::application::prompt_resolver::PromptResolver;
use super::application::AiUseCaseError;
use super::domain::safety;
use super::domain::traits::AiProvider;
use super::domain::types::{
    AiProviderConfig, AiProviderType, AiSafetyGateConfig, AiSettingsConfig, ModelInfo,
};
use super::providers::custom::{self, CustomProtocol};
use super::providers::http::HttpClient;
use super::providers::metadata::provider_defaults;
use super::providers::registry::AiProviderRegistry;
use serde::Serialize;

/// `ai_get_providers` 的展示 DTO：供应商元数据 + 内建端点 / 协议默认值。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderListItem {
    pub provider_type: AiProviderType,
    pub display_name: String,
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub default_endpoint: String,
    pub default_protocol: String,
}

/// AI 栈的懒启动：注册供应商 → 首次注册时恢复持久化配置 → 按界面语言
/// 装载提示词模板。
///
/// GUI 启动路径不跑这一步，首次 AI / 提示词使用时才跑；重复调用只补
/// 语言装载，注册与配置恢复各至多一次。
pub async fn ensure_ai_ready(
    registry: &AiProviderRegistry,
    store: &dyn AiConfigStore,
    prompts: &PromptResolver,
) {
    if registry.ensure_registered().await {
        if let Some(ai_config) = store.get_ai_config().await {
            registry.restore_config(&ai_config).await;
        }
    }
    let lang = store.settings_language().await;
    prompts.ensure_ready(&lang).await;
}

/// 列出全部已注册供应商（展示面：元数据 + 内建默认值）。
pub async fn list_providers(registry: &AiProviderRegistry) -> Vec<ProviderListItem> {
    registry
        .all_providers()
        .await
        .into_iter()
        .map(|p| {
            let (ep, proto) = provider_defaults(p.provider_type());
            ProviderListItem {
                provider_type: p.provider_type(),
                display_name: p.display_name().to_string(),
                supports_streaming: p.supports_streaming(),
                supports_tools: p.supports_tools(),
                default_endpoint: ep.into(),
                default_protocol: proto.into(),
            }
        })
        .collect()
}

/// 校验一份供应商配置（不落盘、不热替换运行态）。
pub async fn validate_provider_config(
    registry: &AiProviderRegistry,
    config: &AiProviderConfig,
) -> Result<(), AiUseCaseError> {
    let provider = registry.get(&config.provider_type).await.ok_or_else(|| {
        AiUseCaseError::NotFound(format!("Provider {:?} not found", config.provider_type))
    })?;
    provider
        .validate_config(config)
        .await
        .map_err(AiUseCaseError::Ai)
}

/// 保存供应商配置：先 initialize（校验并热替换运行态），成功才落盘。
pub async fn save_provider_config(
    registry: &AiProviderRegistry,
    store: &dyn AiConfigStore,
    config: AiProviderConfig,
) -> Result<(), AiUseCaseError> {
    let provider = registry.get(&config.provider_type).await.ok_or_else(|| {
        AiUseCaseError::NotFound(format!("Provider {:?} not found", config.provider_type))
    })?;
    provider
        .initialize(&config)
        .await
        .map_err(AiUseCaseError::Ai)?;
    store.save_ai_config(&config).await
}

/// 当前激活的供应商配置；未配置时 `None`。
pub async fn get_provider_config(store: &dyn AiConfigStore) -> Option<AiProviderConfig> {
    store.get_ai_config().await
}

/// 删除全部 AI 配置：先 reset 每个运行态供应商，再清持久化。
pub async fn delete_provider_config(
    registry: &AiProviderRegistry,
    store: &dyn AiConfigStore,
) -> Result<(), AiUseCaseError> {
    for provider in registry.all_providers().await {
        provider.reset().await;
    }
    store.delete_ai_config().await
}

/// 多档案设置全文。
pub async fn get_settings_config(store: &dyn AiConfigStore) -> AiSettingsConfig {
    store.get_ai_settings_config().await
}

/// 界面语言（提示词解析与响应文本处理的请求输入；宿主设置事实的派生读量）。
///
/// chat / generate 族命令经此取语言而不是直读宿主 store——命令层只递
/// 端口，读什么、从哪读由用例面说了算。
pub async fn settings_language(store: &dyn AiConfigStore) -> String {
    store.settings_language().await
}

/// 保存多档案设置：选中档案若对得上供应商则热初始化（失败不阻断落盘），
/// 然后整文落盘。
pub async fn save_settings_config(
    registry: &AiProviderRegistry,
    store: &dyn AiConfigStore,
    config: AiSettingsConfig,
) -> Result<(), AiUseCaseError> {
    if let Some(profile) = config.selected_profile() {
        let provider_config = AiProviderConfig::from(profile);
        if let Some(provider) = registry.get(&provider_config.provider_type).await {
            let _ = provider.initialize(&provider_config).await;
        }
    }
    store.save_ai_settings_config(&config).await
}

/// 切换激活档案：先落盘，再把新激活的配置热初始化（失败不阻断）。
pub async fn set_active_profile(
    registry: &AiProviderRegistry,
    store: &dyn AiConfigStore,
    profile_id: &str,
) -> Result<(), AiUseCaseError> {
    store.set_active_profile(profile_id).await?;
    if let Some(cfg) = store.get_ai_config().await {
        if let Some(provider) = registry.get(&cfg.provider_type).await {
            let _ = provider.initialize(&cfg).await;
        }
    }
    Ok(())
}

/// 解析当前激活的供应商实例 + 配置（chat / generate 族的公共前置）。
///
/// 未配置 → `AI_NOT_CONFIGURED`；配置的供应商类型不在注册表 →
/// `AI_PROVIDER_NOT_AVAILABLE`（码文本是前端契约，保持逐字不变）。
pub async fn resolve_active_provider(
    registry: &AiProviderRegistry,
    store: &dyn AiConfigStore,
) -> Result<(Arc<dyn AiProvider>, AiProviderConfig), AiUseCaseError> {
    let config = store
        .get_ai_config()
        .await
        .ok_or_else(|| AiUseCaseError::NotConfigured("AI_NOT_CONFIGURED".into()))?;

    let safe_endpoint = config
        .endpoint
        .as_deref()
        .map(datazen_infra::redaction::redact_url_for_log);
    tracing::debug!(
        provider = %config.provider_type,
        model = %config.model,
        endpoint = ?safe_endpoint,
        "resolve_ai: provider config"
    );

    let provider = registry
        .get(&config.provider_type)
        .await
        .ok_or_else(|| AiUseCaseError::NotConfigured("AI_PROVIDER_NOT_AVAILABLE".into()))?;
    Ok((provider, config))
}

/// 生效的安全门配置：设置档案 + 执行设置合并；无档案时回退仅凭据门。
pub async fn effective_safety_gate(store: &dyn AiConfigStore) -> AiSafetyGateConfig {
    safety::effective_safety_gate(
        &store.get_ai_settings_config().await,
        &store.execution_settings().await,
    )
}

/// 拉取远端模型清单（自定义供应商的模型发现）。
///
/// 协议字符串是 IPC 词汇（`open_ai_compatible` / `open_ai_responses` /
/// `anthropic_compatible`），映射在这里定稿，Web 宿主复用同一词汇表；
/// 未知词表值按 Validation 拒绝，文案逐字保留。
pub async fn fetch_remote_models(
    protocol: &str,
    endpoint: &str,
    api_key: &str,
) -> Result<Vec<ModelInfo>, AiUseCaseError> {
    let proto = match protocol {
        "open_ai_compatible" => CustomProtocol::OpenAiCompatible,
        "open_ai_responses" => CustomProtocol::OpenAiResponses,
        "anthropic_compatible" => CustomProtocol::AnthropicCompatible,
        other => {
            return Err(AiUseCaseError::Validation(format!(
                "Unknown protocol: {other}"
            )))
        }
    };
    custom::fetch_remote_models_with_client(HttpClient::desktop(), proto, endpoint, api_key)
        .await
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn test_execution_settings() -> crate::settings::ExecutionSettings {
        crate::settings::ExecutionSettings {
            limit_select_results: true,
            query_result_limit: 1000,
            safe_mode: true,
            connection_pool_size: 10,
            mcp_disabled_tools: Vec::new(),
            mcp_permission_mode: "safe_write".into(),
            mcp_allowed_connection_ids: Vec::new(),
            driver_settings: serde_json::Map::new(),
            ai_strict_egress: true,
        }
    }

    /// 端口假实现：记录读次数，可注入返回值。
    struct FakeConfigStore {
        config: Option<AiProviderConfig>,
        settings: AiSettingsConfig,
        config_reads: AtomicUsize,
        language_reads: AtomicUsize,
    }

    impl FakeConfigStore {
        fn empty() -> Self {
            Self {
                config: None,
                settings: AiSettingsConfig::default(),
                config_reads: AtomicUsize::new(0),
                language_reads: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl AiConfigStore for FakeConfigStore {
        async fn get_ai_config(&self) -> Option<AiProviderConfig> {
            self.config_reads.fetch_add(1, Ordering::SeqCst);
            self.config.clone()
        }

        async fn get_ai_settings_config(&self) -> AiSettingsConfig {
            self.settings.clone()
        }

        async fn save_ai_settings_config(
            &self,
            _config: &AiSettingsConfig,
        ) -> Result<(), AiUseCaseError> {
            Ok(())
        }

        async fn set_active_profile(&self, _profile_id: &str) -> Result<(), AiUseCaseError> {
            Ok(())
        }

        async fn save_ai_config(&self, _config: &AiProviderConfig) -> Result<(), AiUseCaseError> {
            Ok(())
        }

        async fn delete_ai_config(&self) -> Result<(), AiUseCaseError> {
            Ok(())
        }

        async fn settings_language(&self) -> String {
            self.language_reads.fetch_add(1, Ordering::SeqCst);
            "en".into()
        }

        async fn execution_settings(&self) -> crate::settings::ExecutionSettings {
            test_execution_settings()
        }
    }

    fn sample_config(provider_type: AiProviderType) -> AiProviderConfig {
        AiProviderConfig {
            provider_type,
            api_key: Some("sk-test".into()),
            endpoint: None,
            model: "test-model".into(),
            max_tokens: 200_000,
            extra: serde_json::Value::Null,
        }
    }

    #[tokio::test]
    async fn list_providers_maps_builtin_metadata_and_defaults() {
        let registry = super::super::providers::registry::init_ai_providers().await;
        let list = list_providers(&registry).await;

        assert!(list.len() >= 5, "built-ins must all be listed");
        let openai = list
            .iter()
            .find(|item| item.provider_type == AiProviderType::OpenAi)
            .expect("OpenAi listed");
        assert_eq!(openai.display_name, "OpenAI");
        assert_eq!(openai.default_endpoint, "https://api.openai.com/v1");
        assert_eq!(openai.default_protocol, "open_ai_compatible");
        let anthropic = list
            .iter()
            .find(|item| item.provider_type == AiProviderType::Anthropic)
            .expect("Anthropic listed");
        assert_eq!(anthropic.default_endpoint, "https://api.anthropic.com");
        assert_eq!(anthropic.default_protocol, "anthropic_compatible");

        // 序列化形状钉死（原宿主 `test_provider_list_item_serialization`
        // 的等价断言）：camelCase 键 + snake_case 枚举值，前端 TS 接口依赖。
        let json = serde_json::to_string(openai).unwrap();
        assert!(json.contains("\"providerType\":\"open_ai\""));
        assert!(json.contains("\"displayName\":\"OpenAI\""));
        assert!(json.contains("\"supportsStreaming\":"));
        assert!(json.contains("\"supportsTools\":"));
        assert!(json.contains("\"defaultEndpoint\":\"https://api.openai.com/v1\""));
        assert!(json.contains("\"defaultProtocol\":\"open_ai_compatible\""));
    }

    #[tokio::test]
    async fn ensure_ai_ready_registers_and_reads_config_exactly_once() {
        let registry = AiProviderRegistry::new();
        let store = FakeConfigStore::empty();
        let prompts = PromptResolver::new(
            std::path::Path::new("/tmp/datazen-ai-api-tests-no-such-dir"),
            None,
        );

        ensure_ai_ready(&registry, &store, &prompts).await;
        ensure_ai_ready(&registry, &store, &prompts).await;

        assert!(
            registry.available_providers().await.len() >= 5,
            "built-ins registered on first call"
        );
        assert_eq!(
            store.config_reads.load(Ordering::SeqCst),
            1,
            "persisted config is restored only on the one-time registration"
        );
        assert_eq!(
            store.language_reads.load(Ordering::SeqCst),
            2,
            "prompt language resolves on every call"
        );
    }

    #[tokio::test]
    async fn resolve_active_provider_reports_the_contract_codes() {
        let registry = AiProviderRegistry::new();
        let store = FakeConfigStore::empty();

        // `unwrap_err` 需要 Ok 侧 Debug（`Arc<dyn AiProvider>` 不是），用 match。
        let unconfigured = match resolve_active_provider(&registry, &store).await {
            Err(error) => error,
            Ok(_) => panic!("expected AI_NOT_CONFIGURED"),
        };
        assert!(
            matches!(unconfigured, AiUseCaseError::NotConfigured(code) if code == "AI_NOT_CONFIGURED")
        );

        // 配置在、注册表还没注册 → 供应商不可用码（注册表为空即可复现）。
        let store_with_config = FakeConfigStore {
            config: Some(sample_config(AiProviderType::OpenAi)),
            ..FakeConfigStore::empty()
        };
        let unavailable = match resolve_active_provider(&registry, &store_with_config).await {
            Err(error) => error,
            Ok(_) => panic!("expected AI_PROVIDER_NOT_AVAILABLE"),
        };
        assert!(
            matches!(unavailable, AiUseCaseError::NotConfigured(code) if code == "AI_PROVIDER_NOT_AVAILABLE")
        );

        // 注册后同一配置解析成功，返回的配置逐字段一致。
        let ready_registry = super::super::providers::registry::init_ai_providers().await;
        let (provider, config) = resolve_active_provider(&ready_registry, &store_with_config)
            .await
            .unwrap();
        assert_eq!(provider.provider_type(), AiProviderType::OpenAi);
        assert_eq!(config.model, "test-model");
    }

    #[tokio::test]
    async fn fetch_remote_models_rejects_unknown_protocol_with_the_wire_text() {
        let error = fetch_remote_models("not_a_protocol", "https://x", "k")
            .await
            .unwrap_err();
        assert!(
            matches!(error, AiUseCaseError::Validation(message) if message == "Unknown protocol: not_a_protocol")
        );
    }

    #[tokio::test]
    async fn set_active_profile_persists_before_reinitializing() {
        // 端口记录调用序：set 在 get（重新读取激活配置）之前。
        #[derive(Default)]
        struct OrderedStore {
            calls: std::sync::Mutex<Vec<&'static str>>,
        }

        #[async_trait::async_trait]
        impl AiConfigStore for OrderedStore {
            async fn get_ai_config(&self) -> Option<AiProviderConfig> {
                self.calls.lock().unwrap().push("get");
                None
            }
            async fn get_ai_settings_config(&self) -> AiSettingsConfig {
                AiSettingsConfig::default()
            }
            async fn save_ai_settings_config(
                &self,
                _config: &AiSettingsConfig,
            ) -> Result<(), AiUseCaseError> {
                self.calls.lock().unwrap().push("save_settings");
                Ok(())
            }
            async fn set_active_profile(&self, _id: &str) -> Result<(), AiUseCaseError> {
                self.calls.lock().unwrap().push("set");
                Ok(())
            }
            async fn save_ai_config(&self, _c: &AiProviderConfig) -> Result<(), AiUseCaseError> {
                Ok(())
            }
            async fn delete_ai_config(&self) -> Result<(), AiUseCaseError> {
                Ok(())
            }
            async fn settings_language(&self) -> String {
                "en".into()
            }
            async fn execution_settings(&self) -> crate::settings::ExecutionSettings {
                test_execution_settings()
            }
        }

        let registry = AiProviderRegistry::new();
        let store = OrderedStore::default();
        set_active_profile(&registry, &store, "default")
            .await
            .unwrap();
        assert_eq!(*store.calls.lock().unwrap(), vec!["set", "get"]);
    }

    /// 2026-10-31 自桌面 `commands/ai/tests.rs` 归位：`ProviderListItem`
    /// 的 camelCase 线格式是本模块的契约，不是宿主绑定。
    #[test]
    fn provider_list_item_serializes_camel_case() {
        let item = ProviderListItem {
            provider_type: AiProviderType::OpenAi,
            display_name: "OpenAI".into(),
            supports_streaming: true,
            supports_tools: true,
            default_endpoint: "https://api.openai.com/v1".into(),
            default_protocol: "open_ai_compatible".into(),
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"providerType\":\"open_ai\""));
        assert!(json.contains("\"displayName\":\"OpenAI\""));
        assert!(json.contains("\"supportsStreaming\":true"));
        assert!(json.contains("\"defaultEndpoint\":"));
        assert!(json.contains("\"defaultProtocol\":"));
    }
}
