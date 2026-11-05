//! Vendor adapters and shared transport machinery for the AI module.
//!
//! Each file implements [`crate::ai::domain::AiProvider`] for one vendor and
//! delegates wire formatting to [`protocol`]. The [`registry`] resolves an
//! `AiProviderType` to a concrete provider and restores persisted configs.

pub mod anthropic;
pub mod compatible;
pub mod custom;
pub mod deepseek;
pub mod http;
pub mod metadata;
pub mod ollama;
pub mod openai;
pub mod protocol;
pub mod registry;

/// 脚本化假体 LLM（test-harness 夹具门控，不进生产构建；B4 自桌面
/// `testing/mock_ai_provider_test_support.rs` 下沉——两种宿主形态共用同一 `AiProvider`
/// 假体，WR-C3「同用例双传输」的前提）。
#[cfg(feature = "test-harness")]
pub mod testing;
