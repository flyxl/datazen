//! WR-C3 IPC 腿：named_views 用例契约（同用例、Tauri IPC 绑定）。
//!
//! 共享规格本体住在 `datazen::use_case_contracts_test_support::named_views`（形态中立，
//! 不含任何 IPC 词汇；模块头登记验收口径与装配差异清单）。本文件只贡献
//! **传输装配**：
//!
//! - 宿主装配：`tauri::test::mock_app()` 托管真实 `AppState`（沿
//!   `connection_command_test_host` 模式），三条命令走**真实**
//!   `#[tauri::command]` 入口（B2 批次 4 后无 `_impl` 转发层）；
//! - 存储：`TestAppState` 的 tempdir `DesktopRepository`（生产路径同款
//!   仓储端口实现），setup 经 `set_named_views` 直写夹具——与 HTTP 腿
//!   的共享 `MemoryViewStore` 同位；
//! - 错误观测：`CommandError` 的 Display 文本按 C1 前缀契约抽回
//!   application 契约码。
//!
//! 七个 scenario 与 HTTP 腿（`server/tests/use_case_contracts.rs`）与
//! modules 无宿主基线**逐字同名**——「两腿同绿」核对到每条用例；形态
//! 差异只允许出现在本文件尾的装配层断言（信封是纯字符串、id 由宿主
//! ULID 铸造）。

use async_trait::async_trait;
use datazen::named_views::{NamedView, NamedViewScope};
use datazen::use_case_contracts_test_support::named_views::NamedViewsContractBinding;
use datazen::use_case_contracts_test_support::named_views as contract;
use datazen::use_case_contracts_test_support::{ContractFailure, ContractResult};
use tauri::Manager;

use super::{delete_named_view, list_named_views, save_named_view, AppState, CommandError};
use crate::testing::app_state_test_support::{TestAppState, TestStateKeepalive};

// ── 装配层：mock 宿主（与 connection_command_test_host 同形）──────────────

/// A mock tauri app hosting an [`AppState`] for direct command calls.
struct CommandApp {
    app: tauri::App<tauri::test::MockRuntime>,
}

/// A [`TestAppState`] handed to a [`CommandApp`], with the parts that must
/// outlive the test (tempdir, store, mock driver, registry) kept alive.
struct HostedCommandApp {
    app: CommandApp,
    keep: TestStateKeepalive,
}

/// Host a harness's state behind a mock command app.
fn host_command_app(test: TestAppState) -> HostedCommandApp {
    let (state, keep) = test.into_state();
    HostedCommandApp {
        app: CommandApp::new(state),
        keep,
    }
}

impl CommandApp {
    /// Manage `state` behind a fresh mock app.
    fn new(state: AppState) -> Self {
        let app = tauri::test::mock_app();
        app.manage(state);
        Self { app }
    }

    /// The managed state, as a tauri `State` handle for command parameters.
    fn state(&self) -> tauri::State<'_, AppState> {
        self.app.state::<AppState>()
    }
}

// ── binding：IPC 腿对共享规格端口的实现 ───────────────────────────────────

struct IpcBinding {
    host: HostedCommandApp,
}

/// `CommandError` → 规格错误观测（装配层映射）。
///
/// C1 线契约（commands-layer-review §1 / 00-contracts §8.2）：前端按消息
/// **首位前缀**分类，装配层把前缀抽回 application 契约码；无 `grid.*`
/// 前缀的失败（如存储故障）按无码观测——与 application 侧
/// `NamedViewError::code()` 的「Storage 无码」口径一致。
fn observed(error: CommandError) -> ContractFailure {
    let message = error.to_string();
    let code = message
        .split_once(": ")
        .map(|(prefix, _)| prefix.to_string())
        .filter(|prefix| prefix.starts_with("grid."));
    ContractFailure { code, message }
}

async fn harness() -> IpcBinding {
    IpcBinding {
        host: host_command_app(TestAppState::new().await),
    }
}

/// `?Send` 与共享规格端口同口径（见 modules `NamedViewsContractBinding`
/// 文档）：mock 宿主的 `tauri::App<MockRuntime>` 本就非 `Sync`，scenario
/// 顺序 await，binding 不跨线程。
#[async_trait(?Send)]
impl NamedViewsContractBinding for IpcBinding {
    async fn seed(&self, views: Vec<NamedView>) -> Result<(), String> {
        // setup 直写装配的存储（不经用例、不经 IPC）——生产路径同款
        // DesktopRepository，与 HTTP 腿的 MemoryViewStore 同位。
        self.host
            .keep
            .store
            .set_named_views(&views)
            .await
            .map_err(|error| error.to_string())
    }

    async fn list(&self, scope: Option<NamedViewScope>) -> ContractResult<Vec<NamedView>> {
        list_named_views(self.host.app.state(), scope)
            .await
            .map_err(observed)
    }

    async fn save(&self, view: NamedView) -> ContractResult<NamedView> {
        save_named_view(self.host.app.state(), view)
            .await
            .map_err(observed)
    }

    async fn delete(&self, id: String) -> ContractResult<Vec<NamedView>> {
        delete_named_view(self.host.app.state(), id)
            .await
            .map_err(observed)
    }
}

// ── 共享规格：七个 scenario，两腿逐字同名 ─────────────────────────────────

#[tokio::test]
async fn saves_a_new_view_and_returns_the_stored_record() {
    contract::saves_a_new_view_and_returns_the_stored_record(&harness().await).await;
}

#[tokio::test]
async fn listing_orders_default_first_then_by_name() {
    contract::listing_orders_default_first_then_by_name(&harness().await).await;
}

#[tokio::test]
async fn renaming_keeps_the_identity_and_the_creation_time() {
    contract::renaming_keeps_the_identity_and_the_creation_time(&harness().await).await;
}

#[tokio::test]
async fn the_one_default_per_scope_rule_is_enforced_in_one_write() {
    contract::the_one_default_per_scope_rule_is_enforced_in_one_write(&harness().await).await;
}

#[tokio::test]
async fn deleting_an_unknown_id_is_a_no_op_and_the_known_id_is_gone() {
    contract::deleting_an_unknown_id_is_a_no_op_and_the_known_id_is_gone(&harness().await).await;
}

#[tokio::test]
async fn duplicate_names_in_one_scope_are_refused() {
    contract::duplicate_names_in_one_scope_are_refused(&harness().await).await;
}

#[tokio::test]
async fn invalid_records_are_refused_before_the_repository() {
    contract::invalid_records_are_refused_before_the_repository(&harness().await).await;
}

// ── 装配层断言（形态差异唯一允许出现的位置）───────────────────────────────

/// 错误信封 pin：真实命令入口的拒绝以**纯字符串**出 IPC（不是对象），
/// 契约码在消息首位前缀——与 HTTP 腿的 JSON 对象 `{code, message}` 相对照，
/// 形态差异钉在各自装配层。
#[tokio::test]
async fn the_ipc_error_envelope_is_a_plain_string_with_the_code_first() {
    let binding = harness().await;
    binding
        .seed(vec![contract::stored_fixture(
            "view-1",
            "Users",
            &contract::contract_scope(),
            false,
        )])
        .await
        .expect("seed");

    let error = save_named_view(
        binding.host.app.state(),
        contract::draft_fixture("users", &contract::contract_scope()),
    )
    .await
    .expect_err("the duplicate name is refused");

    let payload = serde_json::to_string(&error).expect("the error serializes");
    assert!(
        payload.starts_with('"'),
        "the envelope is a plain string, not an object: {payload}"
    );
    let text = payload.trim_matches('"');
    assert!(
        text.starts_with("grid.view.nameExists: "),
        "the code is the first prefix: {text}"
    );
}

/// id 铸造 pin：铸造格式是宿主的事（端口契约），桌面装配的事实是
/// `HostViewIdMinter` 的 ULID——规格只断言「非空且稳定」，这里钉宿主
/// 自己的形状。
#[tokio::test]
async fn minted_ids_come_from_the_host_minter() {
    let binding = harness().await;
    binding.seed(Vec::new()).await.expect("seed an empty store");

    let saved = binding
        .save(contract::draft_fixture(
            "Monthly orders",
            &contract::contract_scope(),
        ))
        .await
        .expect("a valid draft saves fine");
    assert!(
        crate::store::favorites::ulid::Ulid::is_valid(&saved.id),
        "the host mints ULIDs: {}",
        saved.id
    );
}
