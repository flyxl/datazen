//! In-process mock LLM for the desktop AI tests.
//!
//! B4：[`MockAiProvider`] 本体（tauri-free 脚本化假体）已下沉 modules
//! `datazen::ai::providers::testing`（test-harness 门控）——两种宿主形态
//! 共用同一 `AiProvider` 假体。这里只保留桌面装配（`TestAppState` 扩展：
//! 注册进 `AiProviderRegistry` 并落盘配置）。

pub use datazen::ai::providers::testing::{mock_ai_config, MockAiProvider};

use datazen::ai::AiProvider as _;
use std::sync::Arc;

use super::app_state_test_support::TestAppState;

impl TestAppState {
    /// Register in-process [`MockAiProvider`] as OpenAI and persist its config.
    pub async fn with_mock_ai() -> (Self, Arc<MockAiProvider>) {
        Self::with_mock_ai_options(super::mock_driver::MockDriverOptions::default()).await
    }

    pub async fn with_mock_ai_tables() -> (Self, Arc<MockAiProvider>) {
        Self::with_mock_ai_options(super::app_state_test_support::rich_mock_options()).await
    }

    pub async fn with_mock_ai_options(
        opts: super::mock_driver::MockDriverOptions,
    ) -> (Self, Arc<MockAiProvider>) {
        let test = Self::with_resource_provider(opts).await;
        // Register real builtins first, then overwrite OpenAI with the mock.
        test.state.ensure_ai_ready().await;

        let mock = Arc::new(MockAiProvider::new());
        test.state.ai_registry.register(mock.clone()).await;

        let cfg = mock_ai_config();
        mock.initialize(&cfg).await.expect("mock initialize");
        test.state
            .store
            .save_ai_config(&cfg)
            .await
            .expect("save mock ai config");

        (test, mock)
    }
}
