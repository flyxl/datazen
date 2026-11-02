//! WR-C3 IPC 腿：settings 用例契约（同用例、Tauri IPC 绑定）。
//!
//! 共享规格本体住在 `datazen::use_case_contracts_test_support::settings`
//! （形态中立，不含任何 IPC 词汇）。本文件只贡献**传输装配**：
//!
//! - 宿主装配：`tauri::test::mock_app()` 托管真实 `AppState`（沿
//!   `settings_command_test_host` 模式），两条命令走**真实**
//!   `#[tauri::command]` 入口（`get_settings` / `save_settings`，B2 后
//!   接线直接住在命令里）；
//! - 存储：`TestAppState` 的 tempdir `DesktopRepository`（生产路径同款
//!   `DesktopSettingsRepository` 端口实现），setup 经 `write_shared_settings`
//!   直写夹具——与 HTTP 腿的共享 `MemorySettingsStore` 同位；
//! - 错误观测：`CommandError` 的 Display 文本——settings 域没有契约码，
//!   观测恒为无码（与 application 侧口径一致）。
//!
//! 三个 scenario 与 HTTP 腿（`server/tests/use_case_contracts.rs`）与
//! modules 无宿主基线**逐字同名**——「两腿同绿」核对到每条用例；形态
//! 差异只允许出现在本文件尾的装配层断言（信封是纯字符串、存储细节不出
//! 宿主）。

use async_trait::async_trait;
use datazen::settings::SharedSettings;
use datazen::use_case_contracts_test_support::settings::{
    self as contract, SettingsContractBinding,
};
use datazen::use_case_contracts_test_support::{ContractFailure, ContractResult};

use super::{get_settings, save_settings, CommandError, SaveSettingsRequest};
use crate::testing::app_state_test_support::TestAppState;

use super::settings_command_test_host::{host_command_app, HostedCommandApp};

// ── binding：IPC 腿对共享规格端口的实现 ───────────────────────────────────

struct IpcBinding {
    host: HostedCommandApp,
}

/// `CommandError` → 规格错误观测（装配层映射）。
///
/// settings 域没有契约码：校验拒绝（`CommandError::Validation`）与存储
/// 失败（`CommandError::Internal`）的 IPC 信封都是纯字符串、无首位前缀
/// ——观测的 code 恒为 `None`，「无码」是两腿一致的契约事实。
fn observed(error: CommandError) -> ContractFailure {
    ContractFailure {
        code: None,
        message: error.to_string(),
    }
}

async fn harness() -> IpcBinding {
    IpcBinding {
        host: host_command_app(TestAppState::new().await),
    }
}

/// `?Send` 与共享规格端口同口径（见 modules `SettingsContractBinding`
/// 文档）：mock 宿主的 `tauri::App<MockRuntime>` 本就非 `Sync`，scenario
/// 顺序 await，binding 不跨线程。
#[async_trait(?Send)]
impl SettingsContractBinding for IpcBinding {
    async fn seed(&self, settings: SharedSettings) -> Result<(), String> {
        // setup 直写装配的存储（不经用例、不经 IPC）——生产路径同款
        // DesktopRepository，与 HTTP 腿的 MemorySettingsStore 同位。
        self.host
            .keep
            .store
            .write_shared_settings(settings)
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    async fn fail_writes(&self) -> Result<(), String> {
        // 桌面装配的写失败 = 文件系统事实：把 `settings.json` 换成同名
        // 目录——原子写的 rename 一步必败，缓存照常可读（读路径不受影响，
        // 与 HTTP 腿的写失败开关同位）。失败细节文本由仓储决定，规格
        // 不断言其内容。
        let path = self.host.keep.store.data_dir().join("settings.json");
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        std::fs::create_dir(&path).map_err(|error| error.to_string())
    }

    async fn get(&self) -> ContractResult<SharedSettings> {
        get_settings(self.host.app.state()).await.map_err(observed)
    }

    async fn save(&self, settings: SharedSettings) -> ContractResult<SharedSettings> {
        save_settings(
            self.host.app.state(),
            SaveSettingsRequest { settings },
        )
        .await
        .map_err(observed)
    }
}

// ── 共享规格：三个 scenario，三腿逐字同名 ─────────────────────────────────

#[tokio::test]
async fn saving_settings_normalizes_and_round_trips() {
    contract::saving_settings_normalizes_and_round_trips(&harness().await).await;
}

#[tokio::test]
async fn invalid_enum_values_are_refused_before_the_repository() {
    contract::invalid_enum_values_are_refused_before_the_repository(&harness().await).await;
}

#[tokio::test]
async fn a_storage_failure_surfaces_without_a_contract_code() {
    contract::a_storage_failure_surfaces_without_a_contract_code(&harness().await).await;
}

// ── 装配层断言（形态差异唯一允许出现的位置）───────────────────────────────

/// 错误信封 pin：settings 域的拒绝以**纯字符串**出 IPC（不是对象），且
/// **没有**契约码前缀——校验拒绝的文本是 application 原句逐字
/// （`Validation` 透传）。与 HTTP 腿的 JSON 对象 `{code: null, message}`
/// 相对照，形态差异钉在各自装配层。
#[tokio::test]
async fn the_ipc_settings_refusal_is_a_plain_string_without_a_contract_code() {
    let binding = harness().await;
    binding
        .seed(contract::contract_settings())
        .await
        .expect("seed valid settings");

    let mut invalid = contract::contract_settings();
    invalid.preferences.theme.mode = "neon".into();
    let error = save_settings(
        binding.host.app.state(),
        SaveSettingsRequest { settings: invalid },
    )
    .await
    .expect_err("an invalid enum is refused");

    let payload = serde_json::to_string(&error).expect("the error serializes");
    assert!(
        payload.starts_with('"'),
        "the envelope is a plain string, not an object: {payload}"
    );
    assert_eq!(
        payload.trim_matches('"'),
        "Invalid settings enum value",
        "the application sentence travels verbatim, with no contract-code prefix"
    );
}

/// 存储失败信封 pin：宿主把仓储细节换成固定句「Unable to persist
/// settings」（D1：宿主消息只说宿主知道的，不透传 `backend unavailable`
/// 一族的存储细节）——application 的「无码」事实照常成立。与 HTTP 腿的
/// application 原文相对照，差异钉在各自装配层。
#[tokio::test]
async fn the_ipc_storage_failure_carries_the_host_sentence_without_storage_details() {
    let binding = harness().await;
    binding
        .seed(contract::contract_settings())
        .await
        .expect("seed valid settings");
    binding.fail_writes().await.expect("writes fail");

    let error = save_settings(
        binding.host.app.state(),
        SaveSettingsRequest {
            settings: contract::contract_settings(),
        },
    )
    .await
    .expect_err("a failed storage refuses the write");

    let payload = serde_json::to_string(&error).expect("the error serializes");
    assert_eq!(
        payload.trim_matches('"'),
        "Unable to persist settings",
        "the host sentence replaces the storage detail"
    );
    assert!(
        !payload.contains("backend unavailable"),
        "the storage detail does not cross the host boundary: {payload}"
    );
}
