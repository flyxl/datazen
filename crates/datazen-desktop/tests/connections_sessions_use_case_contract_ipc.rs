#![cfg(feature = "test-harness")]

//! WR-C3 IPC 腿（重做刀）：connections / sessions 用例契约（同用例、Tauri IPC 绑定）。
//!
//! 共享规格本体住在 `datazen::use_case_contracts_test_support::{connections, sessions}`
//! （形态中立，不含任何 IPC 词汇；模块头登记验收口径与装配差异清单）。本文件只贡献
//! **传输装配**（域接入登记：WR-C3 余域第三刀的桌面腿重做——此前一版实现因沙箱
//! 回滚丢失，本文件按既有先例与规格重建）：
//!
//! - 宿主装配：`tauri::test::mock_app()` 托管 **`TestAppState.state` 的克隆**。
//!   `AppState` 不出 crate（`commands` 模块私有），但 `Clone` 共享同一束 Arc
//!   （store / platform adapter / driver registry）——命令看到的装配与 harness
//!   keepalive 持有的仓储是同一份实例。命令参数位是 `State<'_, AppState>`，
//!   因此 `Manager::state::<T>` 的 `T` 由参数位推断落位，本文件无需命名
//!   `AppState` 即可直调**真实** `#[tauri::command]` 入口（B2 后无 `_impl`
//!   转发层）；
//! - 存储：`TestAppState` 的 tempdir `DesktopRepository`（生产路径同款仓储端口
//!   实现，桌面腿的 profile 存储是 `DesktopProfiles` 而非共享内存假体）。setup
//!   经 `DesktopRepository::platform_update` 把规格夹具记录**直写**装配的存储
//!   （不经用例、不经 IPC）；每 scenario 独立 tempdir + adapter；
//! - 错误观测：会话域的用例失败本来就是 application 自己的 `ApiError`
//!   （code + 脱敏 message 两腿同一份，映射走共享规格的
//!   `sessions::application_failure`）；连接域拿到的是桌面宿主 `port_error`
//!   映射后的 `ApiError`——装配层只抽 `{code, message}`，message 保留桌面句
//!   （「profile revision changed」），由本文件尾的 pin 钉住。
//!
//! 七个 scenario 与 HTTP 腿（`crates/datazen-server/tests/use_case_contracts.rs`）与 modules
//! 无宿主基线**逐字同名**——「两腿同绿」核对到每条用例；形态差异只允许出现
//! 在本文件尾的装配层断言（pin）：
//!
//! 1. `the_ipc_revision_conflict_travels_under_the_desktop_sentence`——错误信封
//!    是 `ApiError` `{code:"configRevisionMismatch", message:"profile revision
//!    changed"}`（桌面装配句，与 HTTP 腿的重读指引句相对照）；
//! 2. `minted_connection_ids_are_host_uuids`——桌面 `DesktopProfiles::create`
//!    铸造宿主 uuid v4（与基线 `conn-contract-N` / HTTP 的规格铸造相对照）；
//! 3. `the_ipc_session_answer_is_the_kernel_wire_object`——会话命令回答是内核
//!    camelCase wire 对象（`session.handle.dbSessionId` / `attachmentToken`），
//!    与命令层 `encode` 的 kernel-wire 决定一致。
//!
//! # 三腿 scenario 对照（同名同绿）
//!
//! | scenario | modules 基线 | HTTP 腿（server/tests） | IPC 腿（本文件） |
//! |---|---|---|---|
//! | `a_created_connection_answers_a_purged_view_and_appears_in_the_list` | ✓ | ✓ | ✓ |
//! | `a_stale_expected_revision_is_refused_as_config_revision_mismatch` | ✓ | ✓ | ✓ |
//! | `a_disabled_connection_reports_disabled_and_leaves_the_default_list` | ✓ | ✓ | ✓ |
//! | `opening_a_session_answers_a_receipt_that_get_session_sees` | ✓ | ✓ | ✓ |
//! | `a_context_change_answers_a_receipt_the_new_session_answers_for` | ✓ | ✓ | ✓ |
//! | `closing_a_session_answers_the_same_receipt_twice` | ✓ | ✓ | ✓ |
//! | `a_stale_expected_context_revision_is_refused_as_context_conflict` | ✓ | ✓ | ✓ |
//!
//! # CI 收口
//!
//! 本文件是 `src-tauri/tests/` 集成测试：CI 由 `--test` 名单收口（主线名单已含
//! `--test connections_sessions_use_case_contract_ipc`，feature 形式与
//! `driver_command_ipc` 同款）：
//!
//! ```text
//! cargo test -p datazen_desktop --features test-harness \
//!   --test connections_sessions_use_case_contract_ipc
//! ```

use async_trait::async_trait;
use datazen::connection::{ProfileDraft, ProfilePatch, ProfileRecord, ProfileView};
use datazen::execution::application::requests::{
    CloseSessionRequest, OpenSessionRequest, SetSessionContextRequest,
};
use datazen::execution::{
    CloseReceipt, ContextChangeReceipt, IdempotentOperation, OpenSessionReceipt, SessionHandle,
    SessionView, SubmissionToken,
};
use datazen::shared::context::OwnerRef;
use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::shared::id::{ClientInstanceId, ConfigRevision, ConnectionId};
use datazen::use_case_contracts_test_support::connections::{
    self as connections_contract, ConnectionsContractBinding,
};
use datazen::use_case_contracts_test_support::sessions::{
    self as sessions_contract, SessionsContractBinding,
};
use datazen::use_case_contracts_test_support::{ContractFailure, ContractResult};
use datazen_desktop::platform::session_commands::{
    close_session, create_connection, disable_connection, get_platform_identity, get_session,
    issue_submission_token, list_connections, open_session, set_session_context, update_connection,
    CreateProfileRequest, HandleRequest, ProfileRequest, TokenRequest, UpdateProfileRequest,
};
use datazen_desktop::test_harness::TestAppState;
use datazen_driver_api::ConnectionConfig;
use tauri::Manager;

// ── 装配层：mock 宿主（AppState 不出 crate 的托管方式）────────────────────

/// A mock tauri app hosting a **clone** of the harness's `AppState`.
///
/// `harness` stays the keepalive: tempdir, store, mock driver and registry live
/// in its fields. Its `state` field is left untouched — the managed clone
/// answers the commands, and every Arc inside (store, platform adapter, driver
/// registry) is shared between the two copies, so commands and direct-store
/// setup see one assembled instance.
struct IpcHost {
    app: tauri::App<tauri::test::MockRuntime>,
    harness: TestAppState,
}

/// Host a fresh harness (tempdir + store + adapter, each per scenario) behind a
/// mock command app.
async fn host() -> IpcHost {
    let harness = TestAppState::new().await;
    let app = tauri::test::mock_app();
    // `AppState` 不出 crate：本文件从不命名它——克隆交 `manage`，命令参数位的
    // `State<'_, AppState>` 让 `Manager::state::<T>` 的 T 由推断落位。
    let state = harness.state.clone();
    app.manage(state);
    IpcHost { app, harness }
}

impl IpcHost {
    /// setup 直写：把规格夹具记录映射成桌面配置行落进装配的存储（不经用例、
    /// 不经 IPC）。
    ///
    /// 桌面仓储存的是 `ConnectionConfig` + 平台元数据，不是裸
    /// [`ProfileRecord`]——只映射桌面配置面携带的字段（namespace 数据库、
    /// enabled、版本计数）；其余夹具字段（如 `driver_options` 的自由键）在
    /// 桌面形状里没有落点，重建记录时由 `DesktopProfiles::record` 从配置面
    /// 投影回去。这是 IPC 腿 setup 的装配事实，规格对它零感知。
    async fn seed_profile(&self, record: &ProfileRecord) -> Result<(), String> {
        let id = record.connection_id.as_str().to_owned();
        // 表达式全部按引用插值：无论 `json!` 对插值表达式取值还是取引用都成立，
        // 且不会搬走稍后还要用的 `id`。
        let config: ConnectionConfig = serde_json::from_value(serde_json::json!({
            "id": record.connection_id.as_str(),
            "name": &record.name,
            "databaseType": &record.driver_id,
            "database": &record.initial_namespace.database,
        }))
        .map_err(|error| {
            format!("the seed record does not fit the desktop config shape: {error}")
        })?;
        self.harness
            .store
            .platform_update(&id, None, |_existing, metadata| {
                metadata.enabled = record.enabled;
                metadata.config_revision = record.config_revision.get();
                metadata.credential_revision = record.credential_revision.get();
                Ok(config.clone())
            })
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

// ── binding：IPC 腿对共享规格端口的实现 ───────────────────────────────────

/// `ApiError` → 规格错误观测（连接域装配映射）。
///
/// 命令层的失败已经是宿主映射后的 `ApiError`（`port_error`：CAS 冲突恒
/// `configRevisionMismatch`）。装配层只拆 `{code, message}`：code 是
/// application 契约码（与连接规格 `port_failure` 的裁决一致，宿主不得偷换），
/// message 保留桌面装配句——scenario 只断言「码一致 + 文本在场」，确切文本
/// 由本文件尾的 pin 钉住。
fn observed(error: ApiError) -> ContractFailure {
    ContractFailure {
        code: Some(error.code.as_str().to_owned()),
        message: error.message,
    }
}

/// 内核 wire 对象 → application 词汇（会话命令回答的解码）。
///
/// 会话命令以 `encode(<application DTO>)` 回答——wire 形状是内核 camelCase
/// 对象本身，解码即还原 application 词汇（pin 钉形状，规格拿词汇）。
fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> ContractResult<T> {
    serde_json::from_value(wire).map_err(|error| ContractFailure {
        code: None,
        message: format!("the kernel wire object did not decode: {error}"),
    })
}

// ── connections binding：六个方法走真实命令入口 ──────────────────────────

struct ConnectionsIpc {
    host: IpcHost,
}

async fn connections_harness() -> ConnectionsIpc {
    ConnectionsIpc { host: host().await }
}

/// `?Send` 与共享规格端口同口径（mock 宿主的 `tauri::App<MockRuntime>` 本就
/// 非 `Sync`；binding 由单个测试的栈持有、scenario 顺序 await，不跨线程）。
#[async_trait(?Send)]
impl ConnectionsContractBinding for ConnectionsIpc {
    async fn seed(&self, records: Vec<ProfileRecord>) -> Result<(), String> {
        for record in &records {
            self.host.seed_profile(record).await?;
        }
        Ok(())
    }

    async fn issue_create_profile_token(&self) -> ContractResult<SubmissionToken> {
        let wire = issue_submission_token(
            self.host.app.state(),
            TokenRequest {
                operation: IdempotentOperation::CreateProfile,
                handle: None,
                scope: None,
                connection_id: None,
                owner: None,
            },
        )
        .await
        .map_err(observed)?;
        decode(wire)
    }

    async fn create(
        &self,
        draft: ProfileDraft,
        token: SubmissionToken,
    ) -> ContractResult<ProfileView> {
        // kernel-wire 请求（form-split F4）：草稿平铺 + 内核自己的提交令牌，
        // 与 `BackendClient.createConnection` 同一条 wire。
        let wire = create_connection(
            self.host.app.state(),
            CreateProfileRequest {
                draft,
                submission_token: token,
            },
        )
        .await
        .map_err(observed)?;
        decode(wire)
    }

    async fn update(
        &self,
        connection_id: ConnectionId,
        expected_revision: ConfigRevision,
        patch: ProfilePatch,
    ) -> ContractResult<ProfileView> {
        let wire = update_connection(
            self.host.app.state(),
            UpdateProfileRequest {
                connection_id,
                expected_revision,
                patch,
            },
        )
        .await
        .map_err(observed)?;
        decode(wire)
    }

    async fn disable(&self, connection_id: ConnectionId) -> ContractResult<ProfileView> {
        // 桌面的 disable 走端口 `disable`（不带版本）——§6.3 wire 形状里的
        // `expectedRevision` 核对是 HTTP 宿主侧裁决，不进本端口词汇。
        let wire = disable_connection(self.host.app.state(), ProfileRequest { connection_id })
            .await
            .map_err(observed)?;
        decode(wire)
    }

    async fn list(&self) -> ContractResult<Vec<ProfileView>> {
        let wire = list_connections(self.host.app.state(), serde_json::Value::Null)
            .await
            .map_err(observed)?;
        decode(wire)
    }
}

// ── sessions binding：七个方法走真实命令入口 ─────────────────────────────

struct SessionsIpc {
    host: IpcHost,
}

async fn sessions_harness() -> SessionsIpc {
    SessionsIpc { host: host().await }
}

#[async_trait(?Send)]
impl SessionsContractBinding for SessionsIpc {
    async fn seed_profile(&self, record: ProfileRecord) -> Result<(), String> {
        self.host.seed_profile(&record).await
    }

    async fn client_owner(&self) -> OwnerRef {
        // 身份事实经 IPC 面自己回答（`get_platform_identity`）：`OwnerRef` 的
        // client 半边必须与该腿请求上下文的 client 一致（可采信性核对在用例
        // 层），而桌面 adapter 的 client 实例按进程稳定——问一次即是事实。
        let identity = get_platform_identity(self.host.app.state())
            .await
            .expect("the platform identity answers");
        let client = identity["clientInstanceId"]
            .as_str()
            .expect("the identity carries clientInstanceId");
        OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new(client),
            purpose: "contract".to_owned(),
        }
    }

    async fn issue_session_token(
        &self,
        operation: IdempotentOperation,
        handle: Option<&SessionHandle>,
    ) -> ContractResult<SubmissionToken> {
        let wire = issue_submission_token(
            self.host.app.state(),
            TokenRequest {
                operation,
                handle: handle.cloned(),
                scope: None,
                connection_id: None,
                owner: None,
            },
        )
        .await
        .map_err(sessions_contract::application_failure)?;
        decode(wire)
    }

    async fn open(&self, request: OpenSessionRequest) -> ContractResult<OpenSessionReceipt> {
        let wire = open_session(self.host.app.state(), request)
            .await
            .map_err(sessions_contract::application_failure)?;
        decode(wire)
    }

    async fn get(&self, handle: SessionHandle) -> ContractResult<SessionView> {
        let wire = get_session(self.host.app.state(), HandleRequest { handle })
            .await
            .map_err(sessions_contract::application_failure)?;
        decode(wire)
    }

    async fn set_context(
        &self,
        request: SetSessionContextRequest,
    ) -> ContractResult<ContextChangeReceipt> {
        let wire = set_session_context(self.host.app.state(), request)
            .await
            .map_err(sessions_contract::application_failure)?;
        decode(wire)
    }

    async fn close(&self, request: CloseSessionRequest) -> ContractResult<CloseReceipt> {
        let wire = close_session(self.host.app.state(), request)
            .await
            .map_err(sessions_contract::application_failure)?;
        decode(wire)
    }
}

// ── connections 共享规格：三个 scenario，三腿逐字同名 ─────────────────────

#[tokio::test]
async fn a_created_connection_answers_a_purged_view_and_appears_in_the_list() {
    connections_contract::a_created_connection_answers_a_purged_view_and_appears_in_the_list(
        &connections_harness().await,
    )
    .await;
}

#[tokio::test]
async fn a_stale_expected_revision_is_refused_as_config_revision_mismatch() {
    connections_contract::a_stale_expected_revision_is_refused_as_config_revision_mismatch(
        &connections_harness().await,
    )
    .await;
}

#[tokio::test]
async fn a_disabled_connection_reports_disabled_and_leaves_the_default_list() {
    connections_contract::a_disabled_connection_reports_disabled_and_leaves_the_default_list(
        &connections_harness().await,
    )
    .await;
}

// ── sessions 共享规格：四个 scenario，三腿逐字同名 ────────────────────────

#[tokio::test]
async fn opening_a_session_answers_a_receipt_that_get_session_sees() {
    sessions_contract::opening_a_session_answers_a_receipt_that_get_session_sees(
        &sessions_harness().await,
    )
    .await;
}

#[tokio::test]
async fn a_context_change_answers_a_receipt_the_new_session_answers_for() {
    sessions_contract::a_context_change_answers_a_receipt_the_new_session_answers_for(
        &sessions_harness().await,
    )
    .await;
}

#[tokio::test]
async fn closing_a_session_answers_the_same_receipt_twice() {
    sessions_contract::closing_a_session_answers_the_same_receipt_twice(&sessions_harness().await)
        .await;
}

#[tokio::test]
async fn a_stale_expected_context_revision_is_refused_as_context_conflict() {
    sessions_contract::a_stale_expected_context_revision_is_refused_as_context_conflict(
        &sessions_harness().await,
    )
    .await;
}

// ── 装配层断言（形态差异唯一允许出现的位置）───────────────────────────────

/// 错误信封 pin：CAS 冲突在桌面装配里以 `ApiError` `{code, message}` 出 IPC，
/// code 是两腿一致的 application 契约码，message 是**桌面装配句**——与 HTTP
/// 腿的重读指引句相对照，确切文本钉在本腿。
#[tokio::test]
async fn the_ipc_revision_conflict_travels_under_the_desktop_sentence() {
    let binding = connections_harness().await;
    binding.seed(Vec::new()).await.expect("seed an empty store");
    let token = binding
        .issue_create_profile_token()
        .await
        .expect("a createProfile token issues");
    let created = binding
        .create(connections_contract::contract_draft(), token)
        .await
        .expect("a valid draft creates");
    let stale = ConfigRevision::new(created.config_revision.get());
    let mut patch = ProfilePatch::default();
    patch.name = Some("Renamed".to_owned());
    let _updated = binding
        .update(created.connection_id.clone(), stale, patch.clone())
        .await
        .expect("the current revision updates");

    // 同一个 expectedRevision 现在已过期：直调真实命令入口重放。
    let error = update_connection(
        binding.host.app.state(),
        UpdateProfileRequest {
            connection_id: created.connection_id.clone(),
            expected_revision: stale,
            patch,
        },
    )
    .await
    .expect_err("the now-stale revision is refused");

    assert_eq!(
        error.code,
        ApiErrorCode::ConfigRevisionMismatch,
        "the CAS conflict keeps the application contract code"
    );
    assert_eq!(
        error.message, "profile revision changed",
        "the refusal travels under the desktop sentence"
    );
    let envelope = serde_json::to_value(&error).expect("the error envelope serializes");
    assert_eq!(
        envelope["code"], "configRevisionMismatch",
        "the wire code is the contract code: {envelope}"
    );
    assert_eq!(
        envelope["message"], "profile revision changed",
        "the wire message is the desktop sentence: {envelope}"
    );
}

/// id 铸造 pin：铸造格式是宿主的事（端口契约），桌面装配的事实是
/// `DesktopProfiles::create` 的 uuid v4——规格只断言「非空且稳定」，这里钉
/// 宿主自己的形状。
#[tokio::test]
async fn minted_connection_ids_are_host_uuids() {
    let binding = connections_harness().await;
    binding.seed(Vec::new()).await.expect("seed an empty store");
    let token = binding
        .issue_create_profile_token()
        .await
        .expect("a createProfile token issues");
    let created = binding
        .create(connections_contract::contract_draft(), token)
        .await
        .expect("a valid draft creates");

    let minted = uuid::Uuid::parse_str(created.connection_id.as_str())
        .expect("the desktop host mints connection ids as uuids");
    assert_eq!(
        minted.get_version_num(),
        4,
        "the host mints v4 uuids: {}",
        created.connection_id.as_str()
    );
}

/// 会话回答形状 pin：会话命令回答的是**内核 camelCase wire 对象**（命令层
/// `encode` 的 kernel-wire 决定）——`session.handle.dbSessionId` 与
/// `attachmentToken` 在场且非空，蛇形键不出现。规格对 wire 形状零感知；
/// 这里直调真实命令入口检原始 wire。
#[tokio::test]
async fn the_ipc_session_answer_is_the_kernel_wire_object() {
    let binding = sessions_harness().await;
    binding
        .seed_profile(sessions_contract::contract_profile_record())
        .await
        .expect("seed the target profile");
    let token = binding
        .issue_session_token(IdempotentOperation::OpenSession, None)
        .await
        .expect("an openSession token issues");
    let request = OpenSessionRequest::new(
        sessions_contract::contract_target(),
        binding.client_owner().await,
        token.idempotency_key,
    );

    let wire = open_session(binding.host.app.state(), request)
        .await
        .expect("the session opens");
    assert!(
        wire.is_object(),
        "the session answer is a kernel wire object: {wire}"
    );
    let db_session_id = wire["session"]["handle"]["dbSessionId"]
        .as_str()
        .expect("the receipt carries session.handle.dbSessionId");
    assert!(
        !db_session_id.trim().is_empty(),
        "the wire dbSessionId is non-empty: {db_session_id}"
    );
    let attachment_token = wire["attachmentToken"]
        .as_str()
        .expect("the receipt carries attachmentToken");
    assert!(
        !attachment_token.trim().is_empty(),
        "the wire attachmentToken is non-empty"
    );
    assert!(
        wire["session"]["handle"].get("db_session_id").is_none(),
        "the kernel wire is camelCase, not snake_case: {wire}"
    );
}
