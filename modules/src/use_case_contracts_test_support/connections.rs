//! connections 用例的双传输规格：binding 端口、共享夹具与三个 scenario。
//!
//! 规格本体（本文件除 `baseline` 外的一切）不感知 IPC/HTTP 词汇：scenario
//! 只经 [`ConnectionsContractBinding`] 触达用例，入参出参与断言全部落在
//! `datazen::connection` 的 Profile 词汇（§4.1/§6.3：`ProfileDraft` /
//! `ProfilePatch` / `ProfileView` / `ConfigRevision`）与
//! `datazen::execution` 的提交令牌词汇上。两条宿主腿与无宿主基线各自实现
//! 同一个 binding：
//!
//! - 基线（本文件 `baseline`，`cfg(test)`）：直调 `ProfileRepository` 端口
//!   + `SubmissionTokenIssuer` 假体——宿主整体换掉后规格仍原样过，是
//!   commands-layer-review §1 判据的本体；
//! - HTTP 腿：`server/tests/use_case_contracts.rs`（§6.3 端点形态：GET/POST
//!   `/api/v1/connections`、PATCH `/api/v1/connections/{id}` 一条路由双服务）；
//! - IPC 腿：桌面 `commands/connections_use_case_contract_tests.rs`
//!   （真实 `#[tauri::command]` 入口 `create_connection` / `update_connection`
//!   / `disable_connection` / `list_connections`）。
//!
//! 三腿的 scenario 测试与本文件 scenario 函数**逐字同名**，绿名单逐条
//! 对得上——「两腿同绿」因此可核对到每条用例。
//!
//! # 本域的错误词汇：有码（CAS 冲突族）
//!
//! 与 settings 域的「恒无码」相对照：本域的 CAS 冲突**带契约码**
//! `configRevisionMismatch`——两腿的宿主映射表（桌面 `session_commands`
//! 的 `port_error`、server `http::domains::connections` 的 `profile_error`）
//! 各自从 `PortError` 映射，但 code 一致；**message 是各腿装配句**（桌面
//! 固定句「profile revision changed」，HTTP 携带实体与 id 的重读指引），
//! scenario 只断言「码一致 + 文本在场」，确切文本由各腿装配层 pin 钉住
//! （与 settings 先例同一纪律）。
//!
//! # feature 门
//!
//! 规格消费 `connection`（Profile 词汇）与 `execution`（提交令牌端口）两个
//! feature；`execution` 蕴含 `connection`，所以 CI 的单 feature 形式是
//! `--features datazen/execution,datazen/test-harness`。

use async_trait::async_trait;

use crate::connection::{
    NamespaceTarget, ProfileDraft, ProfilePatch, ProfileRecord, ProfileRepository, ProfileScope,
    ProfileView,
};
use crate::execution::{IdempotentOperation, SubmissionPresentation, SubmissionToken, SubmissionTokenIssuer, VerifiedSubmission};
use crate::shared::context::RequestContext;
use crate::shared::error::PortError;
use crate::shared::id::{
    ClientInstanceId, ConfigRevision, ConnectionId, CredentialRevision, IdempotencyKey, KeyVersion,
    OrganizationId, PrincipalId, RequestId, SecretRef, Timestamp,
};
use crate::use_case_contracts_test_support::{ContractFailure, ContractResult};

// ── 传输装配端口 ─────────────────────────────────────────────────────────

/// connections 用例的传输装配端口（binding harness）。
///
/// 每条腿实现一次：`seed` 是 setup——把夹具记录**直写**进本腿装配的仓储
/// （不经用例，对应生产的「存储里已有/没有这些行」）；五个用例方法则经
/// 本腿的真实传输（tauri command / axum handler）触达 application。规格对
/// 实现方式零假设。
///
/// 身份不出现在任何方法签名上（INV-01）：每条腿各自持有 adapter 构造的
/// `RequestContext`，令牌校验的声称身份（claimed identities）由各腿从自己的
/// 上下文组装。`?Send` 与 named_views/settings 域同一裁决：binding 由单个
/// 测试的栈持有、scenario 顺序 await，规格不假设跨线程。
#[async_trait(?Send)]
pub trait ConnectionsContractBinding {
    /// setup：夹具记录直写本腿存储。
    async fn seed(&self, records: Vec<ProfileRecord>) -> Result<(), String>;

    /// `issueSubmissionToken(createProfile)`（§6.3 令牌列 ✓ 的签发半边）
    /// 经本腿传输。
    async fn issue_create_profile_token(&self) -> ContractResult<SubmissionToken>;

    /// `createConnection` 用例半边（POST /api/v1/connections 与
    /// `create_connection` 的用例半边）：令牌出示 + 草稿 → 回显视图。
    async fn create(
        &self,
        draft: ProfileDraft,
        token: SubmissionToken,
    ) -> ContractResult<ProfileView>;

    /// `updateConnection` 用例半边（PATCH 的 update 语义）：CAS 守卫
    /// `expectedRevision` + 补丁 → 回显视图。
    async fn update(
        &self,
        connection_id: ConnectionId,
        expected_revision: ConfigRevision,
        patch: ProfilePatch,
    ) -> ContractResult<ProfileView>;

    /// `disableConnection` 用例半边（PATCH 的 disable 语义）。§6.3 的
    /// `{expectedRevision, enabled: false}` wire 形状里 revision 的核对是
    /// **宿主侧**裁决（端口 `disable` 不带版本），不进本端口词汇。
    async fn disable(&self, connection_id: ConnectionId) -> ContractResult<ProfileView>;

    /// `listConnections` 用例半边：净化投影 + enabled 过滤（默认列表）。
    async fn list(&self) -> ContractResult<Vec<ProfileView>>;
}

// ── 共享夹具（setup 材料）─────────────────────────────────────────────────

/// 内存仓储假体：整集合读写 + 创建键幂等，别无其它（与桌面
/// `DesktopProfiles` / 服务端 `ServerProfiles` 的端口语义同源：创建铸造
/// `connectionId`、CAS 按 revision 守卫、禁用翻 `enabled`；创建键重放
/// 返回既有行）。
#[derive(Default)]
pub struct MemoryProfileStore {
    records: std::sync::Mutex<Vec<ProfileRecord>>,
    creation_keys: std::sync::Mutex<std::collections::HashMap<String, ConnectionId>>,
    minted: std::sync::Mutex<u32>,
}

impl MemoryProfileStore {
    /// setup 直写：替换整集合（创建键登记随之清空）。
    pub async fn seed(&self, records: Vec<ProfileRecord>) {
        *self.records.lock().expect("memory profile store lock") = records;
        *self
            .creation_keys
            .lock()
            .expect("memory profile store lock") = std::collections::HashMap::new();
    }

    fn lock_records(&self) -> std::sync::MutexGuard<'_, Vec<ProfileRecord>> {
        self.records.lock().expect("memory profile store lock")
    }
}

fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().expect("memory profile store lock")
}

#[async_trait]
impl ProfileRepository for MemoryProfileStore {
    async fn list(
        &self,
        ctx: &RequestContext,
        scope: ProfileScope,
    ) -> Result<Vec<ProfileRecord>, PortError> {
        // 越组织访问返回空而不是报错（CM-05）。
        if scope.organization_id != ctx.organization_id {
            return Ok(Vec::new());
        }
        Ok(self
            .lock_records()
            .iter()
            .filter(|record| record.organization_id == ctx.organization_id)
            .cloned()
            .collect())
    }

    async fn get(
        &self,
        ctx: &RequestContext,
        connection_id: ConnectionId,
    ) -> Result<Option<ProfileRecord>, PortError> {
        Ok(self
            .lock_records()
            .iter()
            .find(|record| {
                record.organization_id == ctx.organization_id && record.connection_id == connection_id
            })
            .cloned())
    }

    async fn create(
        &self,
        ctx: &RequestContext,
        draft: ProfileDraft,
        idem: &IdempotencyKey,
    ) -> Result<ProfileRecord, PortError> {
        let key = format!("{}:{}", ctx.principal_id.as_str(), idem.as_str());
        {
            let creation_keys = lock(&self.creation_keys);
            if let Some(existing) = creation_keys.get(&key) {
                // 幂等重放：同令牌返回既有行（`ServerProfiles::create` 的
                // creation_key 语义）。
                return self
                    .lock_records()
                    .iter()
                    .find(|record| record.connection_id == *existing)
                    .cloned()
                    .ok_or_else(|| PortError::NotFound("profile".into()));
            }
        }
        let mut minted = lock(&self.minted);
        *minted += 1;
        let serial = *minted;
        let connection_id = ConnectionId::new(format!("conn-contract-{serial}"));
        let mut record = ProfileRecord::new(
            ctx.organization_id.clone(),
            connection_id.clone(),
            draft.driver_id.clone(),
        );
        record.name = draft.name.clone();
        record.initial_namespace = draft.initial_namespace.clone();
        record.driver_options = draft.public_options.clone();
        if draft.has_credentials() {
            // 凭据走 SecretRef 通道：明文在此被丢弃（write-only），仓储只持引用。
            record.secret_ref = Some(SecretRef::new(format!(
                "contract-secret://{}",
                connection_id.as_str()
            )));
        }
        record.updated_at = Timestamp::new("2026-01-01T00:00:00.000Z");
        self.lock_records().push(record.clone());
        lock(&self.creation_keys).insert(key, connection_id);
        Ok(record)
    }

    async fn compare_and_set(
        &self,
        ctx: &RequestContext,
        connection_id: ConnectionId,
        expected: ConfigRevision,
        patch: ProfilePatch,
    ) -> Result<ProfileRecord, PortError> {
        let mut records = self.lock_records();
        let record = records
            .iter_mut()
            .find(|record| {
                record.organization_id == ctx.organization_id
                    && record.connection_id == connection_id
            })
            .ok_or_else(|| PortError::NotFound("profile".into()))?;
        if record.config_revision != expected {
            return Err(PortError::cas_conflict(
                "profile",
                connection_id.as_str().to_owned(),
            ));
        }
        if let Some(name) = &patch.name {
            record.name = name.clone();
        }
        if let Some(namespace) = &patch.initial_namespace {
            record.initial_namespace = namespace.clone();
        }
        if let Some(options) = &patch.public_options {
            record.driver_options = options.clone();
        }
        if patch.touches_credentials() {
            record.secret_ref = Some(SecretRef::new(format!(
                "contract-secret://{}",
                record.connection_id.as_str()
            )));
            record.credential_revision =
                CredentialRevision::new(record.credential_revision.counter().get() + 1);
        }
        record.config_revision = ConfigRevision::new(expected.counter().get() + 1);
        record.updated_at = Timestamp::new("2026-01-01T00:00:00.000Z");
        Ok(record.clone())
    }

    async fn disable(
        &self,
        ctx: &RequestContext,
        connection_id: ConnectionId,
    ) -> Result<ProfileRecord, PortError> {
        let mut records = self.lock_records();
        let record = records
            .iter_mut()
            .find(|record| {
                record.organization_id == ctx.organization_id
                    && record.connection_id == connection_id
            })
            .ok_or_else(|| PortError::NotFound("profile".into()))?;
        record.enabled = false;
        record.config_revision = ConfigRevision::new(record.config_revision.counter().get() + 1);
        record.updated_at = Timestamp::new("2026-01-01T00:00:00.000Z");
        Ok(record.clone())
    }

    async fn advance_credential_revision(
        &self,
        ctx: &RequestContext,
        connection_id: ConnectionId,
    ) -> Result<CredentialRevision, PortError> {
        let mut records = self.lock_records();
        let record = records
            .iter_mut()
            .find(|record| {
                record.organization_id == ctx.organization_id
                    && record.connection_id == connection_id
            })
            .ok_or_else(|| PortError::NotFound("profile".into()))?;
        let next = CredentialRevision::new(record.credential_revision.counter().get() + 1);
        record.credential_revision = next;
        Ok(next)
    }
}

/// 令牌签发假体：内存登记 + 身份交叉核对，别无其它（与
/// `SubmissionTokenIssuer` 端口语义同源：`issue` 产出不透明键并登记签发
/// 身份；`verify` 只认登记过的键、声称身份与签发身份不一致即
/// `TokenInvalid`；签名/过期的密码学细节属真实现，规格假体不复制）。
#[derive(Default)]
pub struct ContractTokenIssuer {
    issued: std::sync::Mutex<
        std::collections::HashMap<String, (IdempotentOperation, String, String, String)>,
    >,
    counter: std::sync::Mutex<u32>,
}

#[async_trait]
impl SubmissionTokenIssuer for ContractTokenIssuer {
    async fn issue(
        &self,
        ctx: &RequestContext,
        operation: IdempotentOperation,
        _handle: Option<&crate::execution::SessionHandle>,
    ) -> Result<SubmissionToken, PortError> {
        let mut counter = lock(&self.counter);
        *counter += 1;
        let key = format!("contract-token-{counter}");
        lock(&self.issued).insert(
            key.clone(),
            (
                operation,
                ctx.organization_id.as_str().to_owned(),
                ctx.principal_id.as_str().to_owned(),
                ctx.client_instance_id.as_str().to_owned(),
            ),
        );
        Ok(SubmissionToken {
            idempotency_key: IdempotencyKey::new(key),
            expires_at: Timestamp::new("9999-12-31T23:59:59Z"),
        })
    }

    async fn verify(
        &self,
        presented: &SubmissionPresentation,
    ) -> Result<VerifiedSubmission, PortError> {
        let stored = lock(&self.issued)
            .get(presented.token.idempotency_key.as_str())
            .cloned()
            .ok_or(PortError::TokenInvalid)?;
        let (operation, organization, principal, client) = stored;
        if organization != presented.claimed_organization_id.as_str()
            || principal != presented.claimed_principal_id.as_str()
            || client != presented.claimed_client_instance_id.as_str()
        {
            return Err(PortError::TokenInvalid);
        }
        Ok(VerifiedSubmission {
            operation,
            organization_id: presented.claimed_organization_id.clone(),
            principal_id: presented.claimed_principal_id.clone(),
            client_instance_id: presented.claimed_client_instance_id.clone(),
            idempotency_key: presented.token.idempotency_key.clone(),
            issued_at: Timestamp::new("2026-01-01T00:00:00.000Z"),
            expires_at: presented.token.expires_at.clone(),
            owner_runtime_epoch: None,
            key_version: KeyVersion::new(1),
        })
    }

    fn key_version(&self) -> Result<KeyVersion, PortError> {
        Ok(KeyVersion::new(1))
    }
}

/// 规格主组织：两腿与基线共用同一组织事实（越组织断言若需对照组织，另起
/// 常量，不复用本值）。
pub const CONTRACT_ORGANIZATION: &str = "org-contract";

/// 基线腿的 adapter 构造上下文（宿主腿的身份来自各自装配：HTTP 认证链
/// 扩展 / 桌面 platform adapter）。
pub fn contract_context() -> RequestContext {
    RequestContext::new(
        OrganizationId::new(CONTRACT_ORGANIZATION),
        PrincipalId::new("user-contract"),
        None,
        ClientInstanceId::new("client-contract"),
        RequestId::new("req-contract"),
        None,
    )
}

/// 夹具**草稿**：公开选项 + 初始命名空间，凭据留空（需要凭据形状的
/// scenario 用 [`draft_with_credentials`]）。
pub fn contract_draft() -> ProfileDraft {
    ProfileDraft::new("Production", "postgres")
        .with_initial_namespace(NamespaceTarget::database("app"))
        .with_public_option("host", serde_json::json!("db.internal"))
        .with_public_option("sslMode", serde_json::json!("require"))
}

/// 夹具**带凭据草稿**：凭据形状（D1 的观测对象——永不回显）。
pub fn draft_with_credentials() -> ProfileDraft {
    let mut draft = contract_draft();
    draft.credentials = Some(std::collections::BTreeMap::from([(
        "password".to_owned(),
        "s3cret-contract".to_owned(),
    )]));
    draft
}

/// 夹具**已存**记录：带稳定 id 与 revision 起点，供 `seed` 直写存储。
pub fn stored_fixture(id: &str, name: &str, enabled: bool) -> ProfileRecord {
    let mut record = ProfileRecord::new(
        OrganizationId::new(CONTRACT_ORGANIZATION),
        ConnectionId::new(id),
        "postgres",
    );
    record.name = name.to_owned();
    record.initial_namespace = NamespaceTarget::database("app");
    record.enabled = enabled;
    record
}

/// 端口错误 → 形态中立观测（基线腿的装配映射；两条宿主腿各自的映射必须
/// 产出与这里相同的 code——CAS 冲突恒 `configRevisionMismatch`，端口模块头
/// 的 server host 映射裁决）。message 仍是端口原句（各腿装配可替换为自己的
/// 固定句，scenario 不断言其逐字内容）。
pub fn port_failure(error: PortError) -> ContractFailure {
    let code = match &error {
        PortError::CasConflict { .. } => Some("configRevisionMismatch".to_owned()),
        PortError::NotFound(_) => Some("notFound".to_owned()),
        _ => Some("serviceUnavailable".to_owned()),
    };
    ContractFailure::from_application(code, error.to_string())
}

// ── scenarios（规格本体：setup → request → 断言）──────────────────────────

/// 正常路径：令牌签发 → 创建 → 回答**净化视图**（凭据只表达布尔，永不
/// 回显），且连接进默认列表（enabled 过滤后可见）——返回值即存储事实，
/// 两腿一致。
pub async fn a_created_connection_answers_a_purged_view_and_appears_in_the_list(
    binding: &dyn ConnectionsContractBinding,
) {
    binding
        .seed(Vec::new())
        .await
        .expect("seed an empty store");

    let token = binding
        .issue_create_profile_token()
        .await
        .expect("a createProfile token issues");
    let created = binding
        .create(draft_with_credentials(), token)
        .await
        .expect("a valid draft creates");

    assert!(
        !created.connection_id.as_str().trim().is_empty(),
        "the backend mints a non-empty connection id"
    );
    assert_eq!(created.name, "Production", "the draft name travels");
    assert_eq!(
        created.driver_id, "postgres",
        "the draft driver travels"
    );
    assert!(
        created.credential_configured,
        "the submitted credentials are expressed as the boolean only"
    );
    assert_eq!(
        created.public_options.get("host"),
        Some(&serde_json::json!("db.internal")),
        "visible public options travel"
    );
    let wire = serde_json::to_string(&created).expect("the view serializes");
    assert!(
        !wire.contains("s3cret-contract"),
        "the secret never echoes back: {wire}"
    );
    assert!(
        !wire.contains("credentials") && !wire.contains("secretRef"),
        "the write-only credential channel has no view shape: {wire}"
    );

    let list = binding.list().await.expect("list after create");
    assert_eq!(list.len(), 1, "the created profile is the only one");
    assert_eq!(
        list[0].connection_id, created.connection_id,
        "the minted id is stable"
    );
    assert!(list[0].enabled, "a fresh profile is enabled");
}

/// 错误路径（CAS 冲突族）：过期 `expectedRevision` 的更新被拒，拒绝携带
/// 契约码 `configRevisionMismatch`（两腿宿主映射一致的 application 码）；
/// 文本在场（确切文本属各腿装配句，由各自 pin 钉住）；存量不受牵连。
pub async fn a_stale_expected_revision_is_refused_as_config_revision_mismatch(
    binding: &dyn ConnectionsContractBinding,
) {
    binding
        .seed(Vec::new())
        .await
        .expect("seed an empty store");
    let token = binding
        .issue_create_profile_token()
        .await
        .expect("a createProfile token issues");
    let created = binding
        .create(contract_draft(), token)
        .await
        .expect("a valid draft creates");
    let base_revision = created.config_revision;

    let mut patch = ProfilePatch::default();
    patch.name = Some("Renamed".to_owned());
    let updated = binding
        .update(
            created.connection_id.clone(),
            ConfigRevision::new(base_revision.get()),
            patch.clone(),
        )
        .await
        .expect("the current revision updates");
    assert_eq!(updated.name, "Renamed", "the patch lands");
    assert_eq!(
        updated.config_revision.get(),
        base_revision.get() + 1,
        "the answer is the next revision"
    );

    // 同一个 expectedRevision 现在已过期：重放被 CAS 拒绝。
    let failure = binding
        .update(
            created.connection_id.clone(),
            ConfigRevision::new(base_revision.get()),
            patch,
        )
        .await
        .expect_err("the now-stale revision is refused");
    assert_eq!(
        failure.code.as_deref(),
        Some("configRevisionMismatch"),
        "the CAS conflict carries the contract code on both legs"
    );
    assert!(
        !failure.message.trim().is_empty(),
        "a refusal text still travels: {}",
        failure.message
    );

    let list = binding.list().await.expect("list after the refusal");
    assert_eq!(list.len(), 1, "the refusal happened before the write");
    assert_eq!(
        list[0].name, "Renamed",
        "the refused retry did not touch the stored facts"
    );
}

/// 正常路径：disable 翻 `enabled`——视图报告禁用态，且**默认列表**不再
/// 可见（列表的 enabled 过滤是用例面语义，不是存储顺序）。
pub async fn a_disabled_connection_reports_disabled_and_leaves_the_default_list(
    binding: &dyn ConnectionsContractBinding,
) {
    binding
        .seed(Vec::new())
        .await
        .expect("seed an empty store");
    let token = binding
        .issue_create_profile_token()
        .await
        .expect("a createProfile token issues");
    let created = binding
        .create(contract_draft(), token)
        .await
        .expect("a valid draft creates");

    let disabled = binding
        .disable(created.connection_id.clone())
        .await
        .expect("disable answers the view");
    assert_eq!(
        disabled.connection_id, created.connection_id,
        "the identity is kept"
    );
    assert!(
        !disabled.enabled,
        "the view reports the disabled state"
    );

    let list = binding.list().await.expect("list after disable");
    assert!(
        list.is_empty(),
        "the default list keeps disabled profiles out"
    );
}

// ── 无宿主基线（判据本体：宿主整体换掉，规格原样过）───────────────────────

#[cfg(test)]
mod baseline {
    use super::*;
    use std::sync::Arc;

    /// 无传输绑定：直调 `ProfileRepository` 端口 + 令牌假体。实现与本文件
    /// scenario 用的是同一端口、同一夹具——这就是「把宿主从 Tauri 换成
    /// axum，测试还能原样过」的极限形态（连宿主都没有）。
    struct DirectPortBinding {
        store: Arc<MemoryProfileStore>,
        issuer: Arc<ContractTokenIssuer>,
        ctx: RequestContext,
    }

    impl DirectPortBinding {
        fn presentation(&self, token: SubmissionToken) -> SubmissionPresentation {
            SubmissionPresentation {
                token,
                claimed_organization_id: self.ctx.organization_id.clone(),
                claimed_principal_id: self.ctx.principal_id.clone(),
                claimed_client_instance_id: self.ctx.client_instance_id.clone(),
                claimed_delegation_id: self.ctx.delegation_id.clone(),
            }
        }
    }

    #[async_trait(?Send)]
    impl ConnectionsContractBinding for DirectPortBinding {
        async fn seed(&self, records: Vec<ProfileRecord>) -> Result<(), String> {
            self.store.seed(records).await;
            Ok(())
        }

        async fn issue_create_profile_token(&self) -> ContractResult<SubmissionToken> {
            self.issuer
                .issue(&self.ctx, IdempotentOperation::CreateProfile, None)
                .await
                .map_err(port_failure)
        }

        async fn create(
            &self,
            draft: ProfileDraft,
            token: SubmissionToken,
        ) -> ContractResult<ProfileView> {
            let verified = self
                .issuer
                .verify(&self.presentation(token))
                .await
                .map_err(port_failure)?;
            let record = self
                .store
                .create(&self.ctx, draft, &verified.idempotency_key)
                .await
                .map_err(port_failure)?;
            Ok(ProfileView::from_record(&record))
        }

        async fn update(
            &self,
            connection_id: ConnectionId,
            expected_revision: ConfigRevision,
            patch: ProfilePatch,
        ) -> ContractResult<ProfileView> {
            let record = self
                .store
                .compare_and_set(&self.ctx, connection_id, expected_revision, patch)
                .await
                .map_err(port_failure)?;
            Ok(ProfileView::from_record(&record))
        }

        async fn disable(&self, connection_id: ConnectionId) -> ContractResult<ProfileView> {
            let record = self
                .store
                .disable(&self.ctx, connection_id)
                .await
                .map_err(port_failure)?;
            Ok(ProfileView::from_record(&record))
        }

        async fn list(&self) -> ContractResult<Vec<ProfileView>> {
            let records = self
                .store
                .list(&self.ctx, ProfileScope::from_context(&self.ctx))
                .await
                .map_err(port_failure)?;
            // 镜像用例面 `list_connections` 的双过滤（组织 + enabled）。
            Ok(records
                .iter()
                .filter(|record| {
                    record.organization_id == self.ctx.organization_id && record.enabled
                })
                .map(ProfileView::from_record)
                .collect())
        }
    }

    fn harness() -> DirectPortBinding {
        DirectPortBinding {
            store: Arc::new(MemoryProfileStore::default()),
            issuer: Arc::new(ContractTokenIssuer::default()),
            ctx: contract_context(),
        }
    }

    #[tokio::test]
    async fn a_created_connection_answers_a_purged_view_and_appears_in_the_list() {
        super::a_created_connection_answers_a_purged_view_and_appears_in_the_list(&harness())
            .await;
    }

    #[tokio::test]
    async fn a_stale_expected_revision_is_refused_as_config_revision_mismatch() {
        super::a_stale_expected_revision_is_refused_as_config_revision_mismatch(&harness()).await;
    }

    #[tokio::test]
    async fn a_disabled_connection_reports_disabled_and_leaves_the_default_list() {
        super::a_disabled_connection_reports_disabled_and_leaves_the_default_list(&harness())
            .await;
    }

    /// 基线装配 pin：CAS 冲突的契约码来自端口模块头的 server host 映射
    /// 裁决（`configRevisionMismatch`）。宿主腿的映射不得偷换这个码。
    #[test]
    fn the_cas_conflict_travels_under_the_server_host_code() {
        let failure = port_failure(PortError::cas_conflict("profile", "conn-1"));
        assert_eq!(failure.code.as_deref(), Some("configRevisionMismatch"));
    }
}
