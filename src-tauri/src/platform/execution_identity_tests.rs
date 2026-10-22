//! 执行身份解析的宿主单测。
//!
//! 端口 `packages/platform-api/src/ports/identity.rs` 自带的 5 条语义在这里逐条重验。
//! 每条都写成**会自己变红**的断言：改坏实现（把 principal 拼进身份键、把凭据版本
//! 混进身份键、让委托路径转调本人路径）时，下面每一条都会点红。
//!
//! ## 为什么大部分用例走替身而不是真 Store
//!
//! 本模块要断言的是「身份键的派生输入」，不是 SQLite 落盘。真 `Store` 每个用例要
//! 建两份 SQLite（AppDb + HistoryDb），十几条用例就是几十份——那点并发 I/O 负载
//! 会挤占同仓用例的机器资源，让**判定类**用例的时序更敏感。
//! 替身让本模块零 I/O；**只有**「凭据轮换」与「安装级稳定」两条需要真 Store，
//! 且共用同一份（`OnceCell`），全模块只建一次。
//!
//! （历史注记：这里曾经还写着「真 Store 的 I/O 负载会撑大 `platform::live_source::tests::*`
//! 的**进程级 env 竞态**窗口」——那个家族曾有一个 `DesktopUserGuard` 在 Drop 时把
//! `DATAZEN_DESKTOP_USER` 还原成「之前没有」，把并行跑的兄弟用例的桌面主体读歪。
//! 该竞态已在 `platform::identity` 根治：桌面主体改成**进程内只解析一次**，
//! 授权基线不再在请求之间漂移；本模块的零 I/O 夹具只为自己的断言服务。）
//!
//! 替身只替换**策略端口**、**连接配置仓储**、**凭据版本仓储**与**委托作用域绑定**；
//! 被测的 `DesktopIdentityResolver` 本体、派生函数与端口契约一字未改。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use datazen_feature_core::dashboard::schedule_grant::{MonitorGrant, MonitorScope};
use datazen_platform_api::{
    dto::profile::{ProfileDraft, ProfilePatch},
    id::{
        ClientInstanceId, ConfigRevision, IdempotencyKey, PrincipalId, RequestId, SecretRef,
        Timestamp,
    },
    ports::policy::{
        AuthorizationAction, AuthorizationDecision, AuthorizationSubject, DelegationGrant,
        PolicyChangeStream,
    },
    ports::profile::ProfileScope,
    ports::secret::{ResolvedCredential, SecretPurpose},
    target::NamespaceTarget,
};

use super::*;
use crate::platform::identity::LOCAL_ORGANIZATION_ID;

/// 测试用密码。它只用来证明「凭据材料不进身份键」，本身是合成值。
const TEST_PASSWORD: &str = "hunter2-not-a-real-secret";

/// 替身用的秘密引用。它只是**一个引用**，不是材料。
const TEST_SECRET_REF: &str = "test-secret-ref-conn-1";

// ---------------------------------------------------------------------------
// 替身
// ---------------------------------------------------------------------------

/// 内存连接配置仓储替身。
///
/// 越组织访问返回 `None`（CM-05），与桌面实现同口径。
struct FakeProfiles {
    records: Mutex<BTreeMap<String, ProfileRecord>>,
}

impl FakeProfiles {
    fn new(records: Vec<ProfileRecord>) -> Self {
        Self {
            records: Mutex::new(
                records
                    .into_iter()
                    .map(|record| (record.connection_id.as_str().to_owned(), record))
                    .collect(),
            ),
        }
    }

    fn upsert(&self, record: ProfileRecord) {
        let mut records = self.records.lock().unwrap_or_else(|e| e.into_inner());
        records.insert(record.connection_id.as_str().to_owned(), record);
    }
}

#[async_trait]
impl ProfileRepository for FakeProfiles {
    async fn list(
        &self,
        _ctx: &RequestContext,
        _scope: ProfileScope,
    ) -> Result<Vec<ProfileRecord>, PortError> {
        let records = self.records.lock().unwrap_or_else(|e| e.into_inner());
        Ok(records.values().cloned().collect())
    }

    async fn get(
        &self,
        ctx: &RequestContext,
        connection_id: ConnectionId,
    ) -> Result<Option<ProfileRecord>, PortError> {
        let records = self.records.lock().unwrap_or_else(|e| e.into_inner());
        Ok(records
            .get(connection_id.as_str())
            .filter(|record| record.organization_id == ctx.organization_id)
            .cloned())
    }

    async fn create(
        &self,
        _ctx: &RequestContext,
        _draft: ProfileDraft,
        _idem: &IdempotencyKey,
    ) -> Result<ProfileRecord, PortError> {
        Err(PortError::BackendUnavailable(
            "test double: create is not part of the identity path".into(),
        ))
    }

    async fn compare_and_set(
        &self,
        _ctx: &RequestContext,
        _connection_id: ConnectionId,
        _expected: ConfigRevision,
        _patch: ProfilePatch,
    ) -> Result<ProfileRecord, PortError> {
        Err(PortError::BackendUnavailable(
            "test double: compare_and_set is not part of the identity path".into(),
        ))
    }

    async fn disable(
        &self,
        _ctx: &RequestContext,
        _connection_id: ConnectionId,
    ) -> Result<ProfileRecord, PortError> {
        Err(PortError::BackendUnavailable(
            "test double: disable is not part of the identity path".into(),
        ))
    }

    async fn advance_credential_revision(
        &self,
        _ctx: &RequestContext,
        _connection_id: ConnectionId,
    ) -> Result<CredentialRevision, PortError> {
        Err(PortError::BackendUnavailable(
            "test double: advance_credential_revision is not part of the identity path".into(),
        ))
    }
}

/// 内存凭据版本仓储替身。**只给版本号，不给材料**。
struct FakeSecrets {
    revisions: Mutex<BTreeMap<String, u64>>,
}

impl FakeSecrets {
    fn new(revisions: Vec<(String, u64)>) -> Self {
        Self {
            revisions: Mutex::new(revisions.into_iter().collect()),
        }
    }

    fn bump(&self, secret_ref: &str) {
        let mut revisions = self.revisions.lock().unwrap_or_else(|e| e.into_inner());
        let entry = revisions.entry(secret_ref.to_owned()).or_insert(0);
        *entry += 1;
    }
}

#[async_trait]
impl SecretProvider for FakeSecrets {
    async fn resolve(
        &self,
        _ctx: &RequestContext,
        _secret_ref: SecretRef,
        _purpose: SecretPurpose,
    ) -> Result<ResolvedCredential, PortError> {
        Err(PortError::BackendUnavailable(
            "test double: the identity path must never read credential material".into(),
        ))
    }

    async fn read_versioned(
        &self,
        _ctx: &RequestContext,
        _secret_ref: SecretRef,
        _revision: CredentialRevision,
    ) -> Result<ResolvedCredential, PortError> {
        Err(PortError::BackendUnavailable(
            "test double: the identity path must never read credential material".into(),
        ))
    }

    async fn rotate(
        &self,
        _ctx: &RequestContext,
        _secret_ref: SecretRef,
        _expected: CredentialRevision,
    ) -> Result<CredentialRevision, PortError> {
        Err(PortError::BackendUnavailable(
            "test double: rotate is not part of the identity path".into(),
        ))
    }

    async fn revision(&self, secret_ref: SecretRef) -> Result<CredentialRevision, PortError> {
        let revisions = self.revisions.lock().unwrap_or_else(|e| e.into_inner());
        let value = *revisions
            .get(secret_ref.as_str())
            .ok_or_else(|| PortError::NotFound("credential".into()))?;
        Ok(CredentialRevision::new(value))
    }
}

/// 固定隔离键的策略端口替身。
///
/// 隔离键刻意取**固定值**（桌面真实策略给的是 `desktop:{org}:{principal}`）：
/// 隔离键固定之后，能改变身份键的输入就只剩组织与连接，
/// 「客户端主体不参与派生」这条才测得到。
struct FixedPolicy {
    isolation: PolicyIsolationKey,
    /// 允许通过校验的委托。`None` ⇒ 任何委托都校验不过。
    authorized: Option<DelegationRef>,
}

impl FixedPolicy {
    fn new(isolation: PolicyIsolationKey, authorized: Option<DelegationRef>) -> Self {
        Self {
            isolation,
            authorized,
        }
    }
}

#[async_trait]
impl PolicyService for FixedPolicy {
    async fn authorize(
        &self,
        _ctx: &RequestContext,
        _subject: AuthorizationSubject,
        _action: AuthorizationAction,
    ) -> Result<AuthorizationDecision, PortError> {
        Ok(AuthorizationDecision::Deny {
            reason_code: "test_policy",
        })
    }

    async fn verify_delegation(
        &self,
        _ctx: &RequestContext,
        delegation: DelegationRef,
    ) -> Result<DelegationGrant, PortError> {
        match &self.authorized {
            Some(expected) if *expected == delegation => Ok(DelegationGrant {
                delegation,
                actions: vec![AuthorizationAction::Connect, AuthorizationAction::Query],
            }),
            _ => Err(PortError::TokenInvalid),
        }
    }

    fn isolation_key(&self, _ctx: &RequestContext) -> Result<PolicyIsolationKey, PortError> {
        Ok(self.isolation.clone())
    }

    fn subscribe_version_changes(&self) -> PolicyChangeStream {
        Box::new(std::iter::empty())
    }
}

/// 固定委托目标的替身。`None` ⇒ 委托没有绑定目标（失败关闭）。
struct FixedScope {
    target: Option<ExecutionTarget>,
}

impl FixedScope {
    fn new(target: Option<ExecutionTarget>) -> Self {
        Self { target }
    }
}

#[async_trait]
impl DelegationScopeBinding for FixedScope {
    async fn authorized_target(
        &self,
        _ctx: &RequestContext,
        _delegation: &DelegationRef,
    ) -> Result<ExecutionTarget, PortError> {
        self.target.clone().ok_or(PortError::TokenInvalid)
    }
}

// ---------------------------------------------------------------------------
// 夹具
// ---------------------------------------------------------------------------

/// 替身夹具：零 I/O。
struct Fixture {
    profiles: Arc<FakeProfiles>,
    secrets: Arc<FakeSecrets>,
    ctx: RequestContext,
    connection_id: ConnectionId,
    target: ExecutionTarget,
}

impl Fixture {
    fn resolver(&self) -> DesktopIdentityResolver {
        self.resolver_with(FixedPolicy::new(
            PolicyIsolationKey::new("test-isolation"),
            None,
        ))
    }

    fn resolver_with(&self, policy: FixedPolicy) -> DesktopIdentityResolver {
        DesktopIdentityResolver::new(
            self.profiles.clone(),
            self.secrets.clone(),
            Arc::new(policy),
            Arc::new(FixedScope::new(Some(self.target.clone()))),
            test_derivation_key(),
        )
    }
}

fn test_derivation_key() -> IdentityDerivationKey {
    IdentityDerivationKey::from_master_key(&[7u8; 32]).expect("派生盐")
}

fn profile_record(credential_revision: u64) -> ProfileRecord {
    let mut record = ProfileRecord::new(
        OrganizationId::new(LOCAL_ORGANIZATION_ID),
        ConnectionId::new("conn-1"),
        "postgresql",
    );
    record.secret_ref = Some(SecretRef::new(TEST_SECRET_REF));
    record.credential_revision = CredentialRevision::new(credential_revision);
    record
}

fn fixture() -> Fixture {
    let connection_id = ConnectionId::new("conn-1");
    Fixture {
        profiles: Arc::new(FakeProfiles::new(vec![profile_record(3)])),
        secrets: Arc::new(FakeSecrets::new(vec![(TEST_SECRET_REF.to_owned(), 3)])),
        ctx: RequestContext::new(
            OrganizationId::new(LOCAL_ORGANIZATION_ID),
            PrincipalId::new("alice"),
            None,
            ClientInstanceId::new("client-1"),
            RequestId::new("req-1"),
            None,
        ),
        target: ExecutionTarget::new(connection_id.clone(), NamespaceTarget::database("app")),
        connection_id,
    }
}

fn delegation(id: &str, organization: &str) -> DelegationRef {
    DelegationRef::new(
        DelegationId::new(id),
        OrganizationId::new(organization),
        PrincipalId::new("monitor-service"),
        ClientInstanceId::new("monitor:del-1:1"),
        "monitor:dash:widget:rev:1".to_owned(),
        Timestamp::new("2026-01-02T00:00:00Z"),
    )
}

fn delegated_ctx(delegation: &DelegationRef) -> RequestContext {
    RequestContext::new(
        delegation.organization_id.clone(),
        delegation.principal_id.clone(),
        None,
        delegation.client_instance_id.clone(),
        RequestId::new("req-delegated"),
        Some(delegation.delegation_id.clone()),
    )
}

/// 全模块**唯一**一份真 Store（懒建一次）。
///
/// 只有「凭据轮换」与「安装级稳定」两条用例需要它——前者要真的落一次轮换，
/// 后者要真的用宿主主密钥派生派生盐。其余用例零 I/O。
static REAL_STORE: tokio::sync::OnceCell<RealFixture> = tokio::sync::OnceCell::const_new();

struct RealFixture {
    store: Arc<Store>,
    profiles: Arc<crate::platform::repositories::DesktopProfiles>,
    ctx: RequestContext,
    connection_id: ConnectionId,
    config_revision: u64,
    target: ExecutionTarget,
    _dir: tempfile::TempDir,
}

async fn real_fixture() -> &'static RealFixture {
    REAL_STORE
        .get_or_init(|| async {
            let dir = tempfile::tempdir().expect("临时目录");
            let store = Arc::new(
                Store::init_with_path(dir.path())
                    .await
                    .expect("store 初始化"),
            );
            let profiles = Arc::new(crate::platform::repositories::DesktopProfiles::new(
                store.clone(),
            ));
            let ctx = RequestContext::new(
                OrganizationId::new(LOCAL_ORGANIZATION_ID),
                PrincipalId::new("alice"),
                None,
                ClientInstanceId::new("client-1"),
                RequestId::new("req-1"),
                None,
            );
            let draft = ProfileDraft {
                name: "测试连接".to_owned(),
                driver_id: "postgresql".to_owned(),
                initial_namespace: NamespaceTarget::database("app"),
                public_options: BTreeMap::new(),
                credentials: Some(BTreeMap::from([(
                    "password".to_owned(),
                    TEST_PASSWORD.to_owned(),
                )])),
            };
            let record = profiles
                .create(&ctx, draft, &IdempotencyKey::new("idem-1"))
                .await
                .expect("连接配置");
            let target = ExecutionTarget::new(
                record.connection_id.clone(),
                NamespaceTarget::database("app"),
            );
            RealFixture {
                store,
                profiles,
                ctx,
                connection_id: record.connection_id,
                config_revision: record.config_revision.get(),
                target,
                _dir: dir,
            }
        })
        .await
}

// ---------------------------------------------------------------------------
// 端口语义 1：身份键不得由客户端传入的 principal 派生
// ---------------------------------------------------------------------------

/// 同一组织、同一连接、同一隔离键下，**换一个请求主体身份键不变**。
///
/// 隔离键被固定，因此这条断言真正测的是「主体没有进派生输入」：
/// 若实现把 `principal` 拼进身份键，两个主体的身份键就会不同。
#[tokio::test]
async fn 身份键不随请求主体变化() {
    let fixture = fixture();
    let resolver = fixture.resolver();

    let alice = resolver
        .resolve(&fixture.ctx, &fixture.target)
        .await
        .expect("本人身份");
    let bob_ctx = RequestContext::new(
        OrganizationId::new(LOCAL_ORGANIZATION_ID),
        PrincipalId::new("bob"),
        None,
        ClientInstanceId::new("client-1"),
        RequestId::new("req-bob"),
        None,
    );
    let bob = resolver
        .resolve(&bob_ctx, &fixture.target)
        .await
        .expect("另一主体的身份");

    assert_eq!(
        alice.execution_identity_key, bob.execution_identity_key,
        "主体不得参与身份键派生"
    );
    assert_ne!(
        alice.execution_identity_key.as_str(),
        bob_ctx.principal_id.as_str(),
        "身份键不得就是主体 ID"
    );
}

/// 派生入口的**签名**里不得出现主体、密码、token、密钥引用。
///
/// 这是结构性断言：只测行为的话，把主体拼进派生再在某个分支里抵消掉也能过。
#[test]
fn 派生入口的入参里没有主体与凭据() {
    let production = crate::platform::production_source(include_str!("execution_identity.rs"));
    let lists = crate::platform::fn_parameter_lists(&production);
    let derivations: Vec<&str> = lists
        .iter()
        .filter(|list| list.contains("policy_isolation_key: &PolicyIsolationKey"))
        .map(|list| list.as_str())
        .collect();
    assert!(
        derivations.len() >= 2,
        "应当同时存在本人与委托两条派生入口，实际取到 {} 个",
        derivations.len()
    );
    for list in derivations {
        let lowered = list.to_ascii_lowercase();
        for forbidden in [
            "principal",
            "password",
            "token",
            "secret",
            "credential",
            "material",
        ] {
            assert!(
                !lowered.contains(forbidden),
                "派生入口不得接收 {forbidden}：fn({list})"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 端口语义 2：同一 (organization, connection, isolation) 稳定返回同一身份键
// ---------------------------------------------------------------------------

#[tokio::test]
async fn 同一输入两次解析得到同一身份键() {
    let fixture = fixture();
    let resolver = fixture.resolver();

    let first = resolver
        .resolve(&fixture.ctx, &fixture.target)
        .await
        .expect("第一次身份");
    let second = resolver
        .resolve(&fixture.ctx, &fixture.target)
        .await
        .expect("第二次身份");

    assert_eq!(
        first.execution_identity_key, second.execution_identity_key,
        "同一输入必须给出同一身份键"
    );
    assert_eq!(first, second);
    assert!(!first.execution_identity_key.as_str().is_empty());
}

/// 换一把解析器实例（派生盐取自宿主主密钥）身份键仍然相同。
///
/// 这条覆盖「安装级稳定」：主密钥跨进程重启不变 ⇒ 身份键跨重启不变，
/// 否则会话连续性（INV-02）与池复用都不成立。
#[tokio::test]
async fn 换一把解析器实例身份键仍然相同() {
    let fixture = real_fixture().await;
    let policy = Arc::new(FixedPolicy::new(
        PolicyIsolationKey::new("test-isolation"),
        None,
    ));
    let first = DesktopIdentityResolver::for_desktop(
        fixture.store.clone(),
        fixture.profiles.clone(),
        policy.clone(),
    )
    .expect("桌面装配入口")
    .resolve(&fixture.ctx, &fixture.target)
    .await
    .expect("第一次身份");
    let second = DesktopIdentityResolver::for_desktop(
        fixture.store.clone(),
        fixture.profiles.clone(),
        policy,
    )
    .expect("桌面装配入口")
    .resolve(&fixture.ctx, &fixture.target)
    .await
    .expect("第二次身份");

    assert_eq!(
        first.execution_identity_key, second.execution_identity_key,
        "派生盐取自宿主主密钥，身份键必须跨实例稳定"
    );
}

/// 不同组织 ⇒ 不同身份键（组织维度必须进派生）。
#[test]
fn 不同组织派生不同身份键() {
    let derivation = test_derivation_key();
    let connection = ConnectionId::new("conn-1");
    let isolation = PolicyIsolationKey::new("iso-1");

    let org_a = derive_identity_key(
        &derivation,
        &OrganizationId::new("org-a"),
        &connection,
        &isolation,
    )
    .expect("组织 A 的身份键");
    let org_b = derive_identity_key(
        &derivation,
        &OrganizationId::new("org-b"),
        &connection,
        &isolation,
    )
    .expect("组织 B 的身份键");

    assert_ne!(org_a, org_b, "跨组织必须得到不同身份键");
}

/// 不同连接 ⇒ 不同身份键。
#[test]
fn 不同连接派生不同身份键() {
    let derivation = test_derivation_key();
    let organization = OrganizationId::new("org-a");
    let isolation = PolicyIsolationKey::new("iso-1");

    let conn_a = derive_identity_key(
        &derivation,
        &organization,
        &ConnectionId::new("conn-a"),
        &isolation,
    )
    .expect("连接 A 的身份键");
    let conn_b = derive_identity_key(
        &derivation,
        &organization,
        &ConnectionId::new("conn-b"),
        &isolation,
    )
    .expect("连接 B 的身份键");

    assert_ne!(conn_a, conn_b, "不同连接必须得到不同身份键");
}

/// 不同隔离键 ⇒ 不同身份键（策略撤销必须立刻换键）。
#[test]
fn 不同隔离键派生不同身份键() {
    let derivation = test_derivation_key();
    let organization = OrganizationId::new("org-a");
    let connection = ConnectionId::new("conn-1");

    let iso_a = derive_identity_key(
        &derivation,
        &organization,
        &connection,
        &PolicyIsolationKey::new("iso-a"),
    )
    .expect("隔离键 A 的身份键");
    let iso_b = derive_identity_key(
        &derivation,
        &organization,
        &connection,
        &PolicyIsolationKey::new("iso-b"),
    )
    .expect("隔离键 B 的身份键");

    assert_ne!(iso_a, iso_b, "策略隔离变化必须换身份键");
}

/// 跨组织的连接查不到配置 ⇒ 不发执行身份（CM-05：不泄漏「存在但无权」）。
#[tokio::test]
async fn 跨组织的连接拿不到执行身份() {
    let fixture = fixture();
    let resolver = fixture.resolver();
    let foreign = RequestContext::new(
        OrganizationId::new("org-foreign"),
        PrincipalId::new("alice"),
        None,
        ClientInstanceId::new("client-1"),
        RequestId::new("req-foreign"),
        None,
    );

    let error = resolver
        .resolve(&foreign, &fixture.target)
        .await
        .expect_err("跨组织必须被拒");
    assert!(
        matches!(error, PortError::NotFound(_)),
        "跨组织访问应表现为不存在：{error:?}"
    );
}

// ---------------------------------------------------------------------------
// 端口语义 3：轮换凭据只推进 credential_revision，不动身份键
// ---------------------------------------------------------------------------

/// 真轮换：凭据版本推进，身份键不动。
///
/// 走真 `DesktopProfiles::compare_and_set`（真落盘），因此这条断言的是**真实**行为：
/// 轮换推进 `credential_revision`，而身份键的输入里没有凭据版本。
#[tokio::test]
async fn 轮换凭据只推进版本不动身份键() {
    let fixture = real_fixture().await;
    let policy = Arc::new(FixedPolicy::new(
        PolicyIsolationKey::new("test-isolation"),
        None,
    ));
    let resolver = DesktopIdentityResolver::for_desktop(
        fixture.store.clone(),
        fixture.profiles.clone(),
        policy,
    )
    .expect("桌面装配入口");

    let before = resolver
        .resolve(&fixture.ctx, &fixture.target)
        .await
        .expect("轮换前身份");
    fixture
        .profiles
        .compare_and_set(
            &fixture.ctx,
            fixture.connection_id.clone(),
            ConfigRevision::new(fixture.config_revision),
            ProfilePatch {
                credentials: Some(BTreeMap::from([(
                    "password".to_owned(),
                    "rotated-not-a-real-secret".to_owned(),
                )])),
                ..ProfilePatch::default()
            },
        )
        .await
        .expect("轮换凭据");
    let after = resolver
        .resolve(&fixture.ctx, &fixture.target)
        .await
        .expect("轮换后身份");

    assert_eq!(
        before.execution_identity_key, after.execution_identity_key,
        "凭据轮换不得改动身份键"
    );
    assert_ne!(
        before.credential_revision, after.credential_revision,
        "凭据轮换必须推进凭据版本"
    );
    assert_eq!(
        after.credential_revision,
        CredentialRevision::new(before.credential_revision.get() + 1)
    );
}

/// 替身层面的同一条性质：凭据版本推进时身份键不动。
///
/// 真轮换那条已经覆盖集成面；这条把「版本变了 ⇒ 键不变」单独钉在解析器上，
/// 读的是 `SecretProvider::revision` 这条通路。
#[tokio::test]
async fn 凭据版本推进时身份键不动() {
    let fixture = fixture();
    let resolver = fixture.resolver();

    let before = resolver
        .resolve(&fixture.ctx, &fixture.target)
        .await
        .expect("轮换前身份");
    fixture.secrets.bump(TEST_SECRET_REF);
    let mut record = profile_record(4);
    record.credential_revision = CredentialRevision::new(4);
    fixture.profiles.upsert(record);
    let after = resolver
        .resolve(&fixture.ctx, &fixture.target)
        .await
        .expect("轮换后身份");

    assert_eq!(
        before.execution_identity_key, after.execution_identity_key,
        "凭据版本不得进入身份键"
    );
    assert_ne!(before.credential_revision, after.credential_revision);
    assert_eq!(after.credential_revision, CredentialRevision::new(4));
}

// ---------------------------------------------------------------------------
// 端口语义 4：resolve 与 resolve_delegation 是两条独立路径
// ---------------------------------------------------------------------------

#[tokio::test]
async fn 委托身份与本人身份不同且未授权委托失败关闭() {
    let fixture = fixture();
    let granted = delegation("del-1", LOCAL_ORGANIZATION_ID);
    let resolver = fixture.resolver_with(FixedPolicy::new(
        PolicyIsolationKey::new("test-isolation"),
        Some(granted.clone()),
    ));

    let delegated = resolver
        .resolve_delegation(&delegated_ctx(&granted), granted.clone())
        .await
        .expect("委托身份");
    let personal = resolver
        .resolve(&fixture.ctx, &fixture.target)
        .await
        .expect("本人身份");

    assert_ne!(
        delegated.execution_identity_key, personal.execution_identity_key,
        "委托身份不得等于本人身份（CM-06）"
    );
    assert_eq!(delegated.connection_id, fixture.connection_id);

    // 未授权的委托：失败关闭，**不**降级成调用者本人的权限。
    let unauthorized = delegation("del-2", LOCAL_ORGANIZATION_ID);
    let error = resolver
        .resolve_delegation(&delegated_ctx(&granted), unauthorized)
        .await
        .expect_err("未授权委托必须被拒");
    assert!(
        matches!(error, PortError::TokenInvalid),
        "未授权委托应失败关闭：{error:?}"
    );
}

/// 委托上下文不得走本人路径。
///
/// 「两条独立路径」若只停在方法签名上，实现完全可以让 `resolve` 在委托上下文里
/// 照样产出身份——那正是 CM-06 要防的「委托主体继承本人权限」。
#[tokio::test]
async fn 委托上下文不得走本人路径() {
    let fixture = fixture();
    let granted = delegation("del-1", LOCAL_ORGANIZATION_ID);
    let resolver = fixture.resolver_with(FixedPolicy::new(
        PolicyIsolationKey::new("test-isolation"),
        Some(granted.clone()),
    ));

    let error = resolver
        .resolve(&delegated_ctx(&granted), &fixture.target)
        .await
        .expect_err("委托上下文必须被本人路径拒绝");
    assert!(
        matches!(error, PortError::TokenInvalid),
        "委托上下文应失败关闭：{error:?}"
    );
}

/// 跨组织的委托引用一律拒绝。
#[tokio::test]
async fn 跨组织的委托引用被拒绝() {
    let fixture = fixture();
    let granted = delegation("del-1", LOCAL_ORGANIZATION_ID);
    let resolver = fixture.resolver_with(FixedPolicy::new(
        PolicyIsolationKey::new("test-isolation"),
        Some(granted.clone()),
    ));
    let foreign = delegation("del-1", "org-foreign");

    let error = resolver
        .resolve_delegation(&delegated_ctx(&granted), foreign)
        .await
        .expect_err("跨组织委托必须被拒");
    assert!(
        matches!(error, PortError::TokenInvalid),
        "跨组织委托应失败关闭：{error:?}"
    );
}

/// 两条路径的域标签不同 ⇒ 同样的字段值也派生出不同的身份键。
#[test]
fn 两条路径的域标签不同() {
    assert_ne!(IDENTITY_KEY_DOMAIN, DELEGATION_KEY_DOMAIN);

    let derivation = test_derivation_key();
    let organization = OrganizationId::new("org-a");
    let connection = ConnectionId::new("conn-1");
    let isolation = PolicyIsolationKey::new("iso-1");

    let personal = derive_identity_key(&derivation, &organization, &connection, &isolation)
        .expect("本人身份键");
    let delegated = derive_delegation_identity_key(
        &derivation,
        &organization,
        &connection,
        &isolation,
        &DelegationId::new("del-1"),
        "",
    )
    .expect("委托身份键");

    assert_ne!(
        personal, delegated,
        "委托路径必须有自己的域，不能与本人路径同源"
    );
}

// ---------------------------------------------------------------------------
// 端口语义 5：ExecutionIdentity 的 debug 输出不含秘密材料
// ---------------------------------------------------------------------------

#[tokio::test]
async fn 执行身份不含秘密材料() {
    let fixture = fixture();
    let resolver = fixture.resolver();
    let identity = resolver
        .resolve(&fixture.ctx, &fixture.target)
        .await
        .expect("执行身份");

    let debug = format!("{identity:?}");
    let lowered = debug.to_ascii_lowercase();
    for marker in ["password", "material", "secret", "token", "hunter2"] {
        assert!(
            !lowered.contains(marker),
            "执行身份的 debug 输出不得含 {marker}：{debug}"
        );
    }
}

/// 派生盐本身也不得进日志：它的 `Debug` 必须脱敏。
#[test]
fn 派生盐的调试输出脱敏() {
    let derivation = test_derivation_key();
    let debug = format!("{derivation:?}");
    assert_eq!(debug, "IdentityDerivationKey(<redacted>)");
    assert!(!debug.contains("7"), "派生盐不得出现在调试输出里：{debug}");
}

// ===========================================================================
// 验证轨（identity-resolver 验证轨）补齐的覆盖
//
// 以下用例补的是**首版用例没钉住**的几条性质。每一���都对应一次具体的变异
// （见验证报告）：去掉输入、去掉长度前缀、去掉域分离、放宽失败关闭……
// 只要对应的那段生产代码被改坏，下面就会变红。
// ===========================================================================

/// **宽松**策略端口替身：任何委托都放行。
///
/// 用途：把「跨组织委托必须被拒」从策略端口的校验里剥出来，单独钉在解析器
/// 自己的跨组织检查上。否则那条检查被策略端口顺带覆盖——删掉它不会有任何
/// 用例变红。
struct PermissivePolicy {
    isolation: PolicyIsolationKey,
}

#[async_trait]
impl PolicyService for PermissivePolicy {
    async fn authorize(
        &self,
        _ctx: &RequestContext,
        _subject: AuthorizationSubject,
        _action: AuthorizationAction,
    ) -> Result<AuthorizationDecision, PortError> {
        Ok(AuthorizationDecision::Deny {
            reason_code: "test_policy",
        })
    }

    async fn verify_delegation(
        &self,
        _ctx: &RequestContext,
        delegation: DelegationRef,
    ) -> Result<DelegationGrant, PortError> {
        Ok(DelegationGrant {
            delegation,
            actions: vec![AuthorizationAction::Connect, AuthorizationAction::Query],
        })
    }

    fn isolation_key(&self, _ctx: &RequestContext) -> Result<PolicyIsolationKey, PortError> {
        Ok(self.isolation.clone())
    }

    fn subscribe_version_changes(&self) -> PolicyChangeStream {
        Box::new(std::iter::empty())
    }
}

fn permissive_resolver(fixture: &Fixture) -> DesktopIdentityResolver {
    DesktopIdentityResolver::new(
        fixture.profiles.clone(),
        fixture.secrets.clone(),
        Arc::new(PermissivePolicy {
            isolation: PolicyIsolationKey::new("test-isolation"),
        }),
        Arc::new(FixedScope::new(Some(fixture.target.clone()))),
        test_derivation_key(),
    )
}

/// **长度前缀是承重的**：去掉它，不同的字段切分会拼成同一条消息 ⇒ 同一个身份键。
///
/// 编码里每字段前面带 `u32` 长度；没有它，`("ab","c")` 与 `("a","bc")` 会拼成
/// 同样的字节串，于是两个不同的连接配置会拿到**同一个**身份键。
#[test]
fn 长度前缀让字段切分不撞车() {
    let derivation = test_derivation_key();

    // 消息层面：两种切分不得撞成同一条消息。
    let left = encode_message(b"datazen.test", &["ab", "c"]);
    let right = encode_message(b"datazen.test", &["a", "bc"]);
    assert_ne!(
        left, right,
        "长度前缀必须让两种字段切分产出不同的消息，否则身份键会撞车"
    );

    // 键层面：同样的切分落到真实派生入口上，必须给出不同的身份键。
    let first = derive_identity_key(
        &derivation,
        &OrganizationId::new("ab"),
        &ConnectionId::new("c"),
        &PolicyIsolationKey::new("iso-1"),
    )
    .expect("切分 A 的身份键");
    let second = derive_identity_key(
        &derivation,
        &OrganizationId::new("a"),
        &ConnectionId::new("bc"),
        &PolicyIsolationKey::new("iso-1"),
    )
    .expect("切分 B 的身份键");
    assert_ne!(
        first, second,
        "字段切分不同就必须得到不同的身份键（长度前缀是承重的）"
    );
}

/// **委托身份不可能等于本人身份**——不是「通常不同」，而是构造上不可能。
///
/// 两条断言合起来把这条性质钉死：
/// 1. 域标签不同（把域标签改成同一个 ⇒ 下面第一条变红）；
/// 2. 即使把域标签强行改成同一个，委托路径多出来的 `delegation_id` 与 `scope`
///    也让两条消息不同——所以「域标签不同」不是唯一的护栏。
#[test]
fn 委托路径的消息必然不同于本人路径() {
    let organization = OrganizationId::new("org-a");
    let connection = ConnectionId::new("conn-1");
    let isolation = PolicyIsolationKey::new("iso-1");

    let personal_parts = [
        organization.as_str(),
        connection.as_str(),
        isolation.as_str(),
    ];
    let delegation_parts = [
        organization.as_str(),
        connection.as_str(),
        isolation.as_str(),
        "del-1",
        "scope-1",
    ];

    // 1. 两条路径的域标签必须不同。
    assert_ne!(
        IDENTITY_KEY_DOMAIN, DELEGATION_KEY_DOMAIN,
        "两条路径不得共用同一个域标签"
    );

    // 2. 就算域标签相同，委托路径的输入集严格更大 ⇒ 消息必然不同。
    assert_ne!(
        encode_message(IDENTITY_KEY_DOMAIN, &personal_parts),
        encode_message(IDENTITY_KEY_DOMAIN, &delegation_parts),
        "委托路径的输入集必须严格大于本人路径，否则两条路径会产出同一条消息"
    );

    // 3. 实际使用的两个域下消息也不同。
    assert_ne!(
        encode_message(IDENTITY_KEY_DOMAIN, &personal_parts),
        encode_message(DELEGATION_KEY_DOMAIN, &delegation_parts)
    );
}

/// 委托身份键必须绑定**委托 id** 与**授权作用域**：换委托、换授权范围即换键。
///
/// 否则两个不同权限范围的委托会共用同一个身份键，池与缓存会把它们混用。
#[tokio::test]
async fn 委托身份键绑定委托标识与授权作用域() {
    let fixture = fixture();
    let resolver = permissive_resolver(&fixture);

    let base = delegation("del-1", LOCAL_ORGANIZATION_ID);
    let mut other_scope = base.clone();
    other_scope.scope = "monitor:dash:widget:rev:2".to_owned();
    let mut other_delegation = base.clone();
    other_delegation.delegation_id = DelegationId::new("del-2");

    let ctx = delegated_ctx(&base);
    let first = resolver
        .resolve_delegation(&ctx, base.clone())
        .await
        .expect("基准委托身份");
    let rescoped = resolver
        .resolve_delegation(&ctx, other_scope)
        .await
        .expect("换授权范围的委托身份");
    let redelegated = resolver
        .resolve_delegation(&ctx, other_delegation)
        .await
        .expect("换委托的委托身份");

    assert_ne!(
        first.execution_identity_key, rescoped.execution_identity_key,
        "授权作用域变了必须换身份键"
    );
    assert_ne!(
        first.execution_identity_key, redelegated.execution_identity_key,
        "委托 id 变了必须换身份键"
    );
}

/// 跨组织委托：即便策略端口放行，解析器自己也必须拒。
///
/// 这条专门钉解析器内部那道跨组织检查——首版用例里策略替身会顺带拒掉跨组织
/// 委托，于是那道检查删掉也没有任何用例变红。
#[tokio::test]
async fn 跨组织委托即便策略放行也被拒() {
    let fixture = fixture();
    let resolver = permissive_resolver(&fixture);

    let local = delegation("del-1", LOCAL_ORGANIZATION_ID);
    let foreign = delegation("del-foreign", "org-foreign");

    let error = resolver
        .resolve_delegation(&delegated_ctx(&local), foreign)
        .await
        .expect_err("跨组织委托必须被拒");
    assert!(
        matches!(error, PortError::TokenInvalid),
        "跨组织委托应失败关闭：{error:?}"
    );
}

/// 委托路径**不转调**本人路径。
///
/// 端口原文：「委托身份与本人身份不是同一条路径」。这条从源码结构上钉住它：
/// `resolve_delegation` 的函数体里不得出现 `self.resolve(`，也不得走本人路径
/// 的派生函数。
#[test]
fn 委托路径不转调本人路径() {
    let production = crate::platform::production_source(include_str!("execution_identity.rs"));
    let start = production
        .find("async fn resolve_delegation(")
        .expect("委托入口必须在实现里");
    let end = start
        + production[start..]
            .find("impl fmt::Debug for DesktopIdentityResolver")
            .expect("委托入口的结束位置");
    let body = &production[start..end];

    assert!(
        !body.contains("self.resolve("),
        "resolve_delegation 不得转调 resolve（端口：两条独立路径）"
    );
    assert!(
        body.contains("derive_delegation_identity_key("),
        "委托路径必须自己派生身份键"
    );
    assert!(
        !body.contains("derive_identity_key("),
        "委托路径不得走本人路径的派生函数"
    );
}

/// 派生链的**黄金值**：主密钥 → 子钥 → 身份键，整条链独立复算一遍。
///
/// 独立复算（不复用被测的 `IdentityDerivationKey::from_master_key` 当期望值）的
/// 意义：否则「派生盐来自宿主主密钥」「域分离真的换了密钥」「输出是 URL-safe
/// base64」这些性质都拿被测函数自己当尺子——把尺子换掉就看不见了。
///
/// 主密钥取不到时 `for_desktop` 直接返回 `Err`（失败关闭），装配层据此拒绝接线，
/// 没有兜底的常量盐。
#[tokio::test]
async fn 桌面装配的派生盐来自宿主主密钥() {
    let fixture = real_fixture().await;
    let isolation = PolicyIsolationKey::new("test-isolation");
    let policy = Arc::new(FixedPolicy::new(isolation.clone(), None));
    let resolver = DesktopIdentityResolver::for_desktop(
        fixture.store.clone(),
        fixture.profiles.clone(),
        policy,
    )
    .expect("桌面装配入口");

    // 独立复算子钥：HMAC-SHA256(主密钥, 域标签)。
    let master = master_key_bytes(&fixture.store).expect("宿主主密钥");
    let mut subkey_mac = HmacSha256::new_from_slice(&master).expect("HMAC");
    subkey_mac.update(DERIVATION_KEY_LABEL);
    let subkey: [u8; 32] = subkey_mac
        .finalize()
        .into_bytes()
        .as_slice()
        .try_into()
        .expect("32 字节子钥");

    // 独立复算身份键：URL-safe base64(HMAC-SHA256(子钥, 域标签||长度前缀字段))。
    let organization = OrganizationId::new(LOCAL_ORGANIZATION_ID);
    let mut identity_mac = HmacSha256::new_from_slice(&subkey).expect("HMAC");
    identity_mac.update(&encode_message(
        IDENTITY_KEY_DOMAIN,
        &[
            organization.as_str(),
            fixture.connection_id.as_str(),
            isolation.as_str(),
        ],
    ));
    let expected =
        ExecutionIdentityKey::new(BASE64_URL.encode(identity_mac.finalize().into_bytes()));

    let actual = resolver
        .resolve(&fixture.ctx, &fixture.target)
        .await
        .expect("身份");

    assert_eq!(
        actual.execution_identity_key, expected,
        "身份键必须由宿主主密钥经域分离派生的子钥算出"
    );
}

/// 身份键里不得出现主密钥或派生盐本身。
///
/// 身份键是子钥对输入字段的 HMAC 摘要：摘要不可逆推主密钥，也不可逆推密码。
/// 这条把「主密钥/子钥都不进入身份键」从注释里的断言变成可执行的断言。
#[test]
fn 身份键不含主密钥与派生盐() {
    let master = [7u8; 32];
    let derivation = IdentityDerivationKey::from_master_key(&master).expect("派生盐");
    let key = derive_identity_key(
        &derivation,
        &OrganizationId::new("org-a"),
        &ConnectionId::new("conn-1"),
        &PolicyIsolationKey::new("iso-1"),
    )
    .expect("身份键");
    let text = key.as_str();

    // 域分离确实换了密钥：子钥不等于主密钥。
    assert_ne!(
        derivation.0, master,
        "身份派生必须用独立的子钥，不能拿主密钥直接跑 HMAC"
    );

    for forbidden in [
        BASE64_STANDARD.encode(master),
        BASE64_STANDARD.encode(derivation.0),
    ] {
        assert!(
            !text.contains(forbidden.as_str()),
            "身份键不得包含主密钥或派生盐：{text}"
        );
    }
}

// ---------------------------------------------------------------------------
// 桌面授权单 → 委托目标（宿主协作者 `MonitorDelegationScope`）
// ---------------------------------------------------------------------------

/// 造一份监控授权单。
fn monitor_grant(
    id: &str,
    dashboard_id: &str,
    widget_id: &str,
    workflow_id: &str,
    workflow_yaml: &str,
    scopes: Vec<MonitorScope>,
) -> MonitorGrant {
    MonitorGrant {
        id: id.to_owned(),
        dashboard_id: dashboard_id.to_owned(),
        widget_id: widget_id.to_owned(),
        workflow_id: workflow_id.to_owned(),
        workflow_revision: crate::monitor::grants::revision(workflow_yaml),
        organization_id: LOCAL_ORGANIZATION_ID.to_owned(),
        service_principal: format!("monitor-service:{id}"),
        approved_by: "desktop-user".to_owned(),
        scopes,
        actions: vec!["connect".to_owned(), "query".to_owned()],
        read_only: true,
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        revocation_version: 1,
        revoked: false,
    }
}

/// 把授权单需要的仪表盘 / 部件 / 工作流落进 AppDb。
///
/// 复用 `real_fixture()` 的那一份 Store（全模块只建一次真库），不额外开库。
async fn seed_monitor_grant(store: &Store, grant: &MonitorGrant, workflow_yaml: &str) {
    let db = store.app_db();
    db.upsert_workflow(&crate::store::WorkflowRecord {
        id: grant.workflow_id.clone(),
        name: "Fixed".to_owned(),
        description: String::new(),
        visibility: crate::store::WorkflowVisibility::User,
        definition_yaml: workflow_yaml.to_owned(),
        created_at: String::new(),
        updated_at: String::new(),
    })
    .expect("工作流");
    db.upsert_dashboard(&crate::store::DashboardRecord {
        id: grant.dashboard_id.clone(),
        name: "Ops".to_owned(),
        created_at: String::new(),
        updated_at: String::new(),
        layout_cols: 12,
        layout_row_height: 80,
        enabled: true,
        refresh_paused: false,
    })
    .expect("仪表盘");
    db.upsert_widget(&crate::store::WidgetRecord {
        id: grant.widget_id.clone(),
        dashboard_id: grant.dashboard_id.clone(),
        title: "Report".to_owned(),
        workflow_id: grant.workflow_id.clone(),
        view_mode: "table".to_owned(),
        chart_config_json: None,
        layout_x: 0,
        layout_y: 0,
        layout_w: 4,
        layout_h: 3,
        refresh_mode: "interval".to_owned(),
        refresh_sec: Some(30),
        alert_json: None,
        enabled: true,
        sort_order: 0,
        created_at: String::new(),
        updated_at: String::new(),
    })
    .expect("部件");
    db.save_monitor_grant(grant).expect("授权单");
}

/// 授权单覆盖**多个连接**时必须失败关闭。
///
/// 一个执行身份只对应一个连接；一份授权单跨连接无法映射成单一身份 ⇒ 必须拒绝，
/// 绝不「取第一个了事」。首版用例全部走替身，这段生产逻辑没有任何断言。
#[tokio::test]
async fn 授权单覆盖多个连接时失败关闭() {
    let fixture = real_fixture().await;
    let yaml = "id: wf-monitor-identity\nname: Fixed\n";
    let scope_of = |connection: &str| MonitorScope {
        target: ExecutionTarget::new(
            ConnectionId::new(connection),
            NamespaceTarget::database("app"),
        ),
        config_revision: 1,
        command: "query".to_owned(),
    };

    let single = monitor_grant(
        "grant-identity-single",
        "dash-identity-single",
        "widget-identity-single",
        "wf-monitor-identity",
        yaml,
        vec![scope_of("conn-1")],
    );
    let multi = monitor_grant(
        "grant-identity-multi",
        "dash-identity-multi",
        "widget-identity-multi",
        "wf-monitor-identity",
        yaml,
        vec![scope_of("conn-1"), scope_of("conn-2")],
    );
    seed_monitor_grant(&fixture.store, &single, yaml).await;
    seed_monitor_grant(&fixture.store, &multi, yaml).await;

    let scope = MonitorDelegationScope::new(fixture.store.clone());

    // 单连接授权单：目标就是授权单登记的连接。
    let authorized = scope
        .authorized_target(&single.context(), &single.delegation())
        .await
        .expect("单连接授权单应给出目标");
    assert_eq!(authorized.connection_id, ConnectionId::new("conn-1"));

    // 多连接授权单：失败关闭。
    let error = scope
        .authorized_target(&multi.context(), &multi.delegation())
        .await
        .expect_err("多连接授权单必须失败关闭");
    assert!(
        matches!(error, PortError::TokenInvalid),
        "多连接授权单应失败关闭：{error:?}"
    );
}

// ---------------------------------------------------------------------------
// 复测轮补：把「文档写下的字面量」钉成断言
// ---------------------------------------------------------------------------

/// 共享边界 §4.3 把三条**字面量事实**写进了正式文档：
///
/// 1. 身份键的 HMAC 消息 = 域标签 + 每个字段的 `u32` 长度前缀 + 字段内容；
/// 2. 本人路径域标签 = `datazen.execution-identity.key.v1`，
///    委托路径域标签 = `datazen.execution-identity.delegation-key.v1`；
/// 3. 主密钥轮换 ⇒ 全部身份键变化。
///
/// 复测轮之前的用例只断言**关系**（「两条域标签彼此不同」「不同输入 ⇒ 不同键」），
/// 没有任何一条断言它们**是什么**。于是把域标签从 `v1` 改成 `v2`、或把子钥的
/// 域标签改掉，都会一次性改掉**全部**身份键 ⇒ 连接池与缓存整体失效（等价于一次
/// 主密钥轮换），而当时**一条用例会红**。下面几条把字面量本身钉住。
///
/// 这正是本模块头「改这个文件之前先看变异」那条要求的反面：性质要靠**针对性**
/// 用例钉，不能指望正常路径的用例顺带钉住。

/// 三条域标签的**字面值**必须与文档一致。
///
/// 域标签是派生链的输入，不是「只要彼此不同就行」：改一个字面量就是一次全量
/// 身份键轮换（连接池与缓存整体失效）。
#[test]
fn 域标签的字面值与文档一致() {
    assert_eq!(
        IDENTITY_KEY_DOMAIN,
        b"datazen.execution-identity.key.v1".as_slice(),
        "本人路径的域标签是文档写下的事实；改它等于全量轮换身份键"
    );
    assert_eq!(
        DELEGATION_KEY_DOMAIN,
        b"datazen.execution-identity.delegation-key.v1".as_slice(),
        "委托路径的域标签是文档写下的事实；改它等于全量轮换身份键"
    );
    assert_eq!(
        DERIVATION_KEY_LABEL,
        b"datazen.execution-identity.derivation-key.v1".as_slice(),
        "子钥派生的域标签是文档写下的事实；改它等于全量轮换身份键"
    );
}

/// 消息编码的**字节布局**：域标签 + 每字段 `u32` 大端长度前缀 + 字段内容。
///
/// 期望值在用例里**手写**，不复用被测的 `encode_message` 自己——否则等于拿尺子
/// 量尺子，换掉尺子就看不见了。
#[test]
fn 消息编码的字节布局是域标签加长度前缀加字段内容() {
    let mut expected = Vec::from(b"datazen.test".as_slice());
    expected.extend_from_slice(&2u32.to_be_bytes());
    expected.extend_from_slice(b"ab");
    expected.extend_from_slice(&1u32.to_be_bytes());
    expected.extend_from_slice(b"c");
    assert_eq!(
        encode_message(b"datazen.test", &["ab", "c"]),
        expected,
        "消息编码必须是「域标签 + 每字段 u32 长度前缀 + 字段内容」"
    );
}

/// **反事实**：没有长度前缀时，两种字段切分会拼成**同一条**消息 ⇒ 同一个身份键。
///
/// 这条把「长度前缀是承重的」从注释变成可执行的推理：先用**无前缀**的编码证明
/// 两条消息会撞成一样的字节，再用生产编码证明它们必须分开。少了前半段，这条
/// 用例就只是在复述生产代码。
#[test]
fn 没有长度前缀时两种字段切分会撞车() {
    let without_prefix = |parts: &[&str]| -> Vec<u8> {
        let mut message = Vec::from(b"datazen.test".as_slice());
        for part in parts {
            message.extend_from_slice(part.as_bytes());
        }
        message
    };
    assert_eq!(
        without_prefix(&["ab", "c"]),
        without_prefix(&["a", "bc"]),
        "无长度前缀时这两种切分必须撞成同一条消息（否则这条用例失去意义）"
    );
    assert_ne!(
        encode_message(b"datazen.test", &["ab", "c"]),
        encode_message(b"datazen.test", &["a", "bc"]),
        "生产编码必须靠长度前缀把两种切分分开"
    );
}

/// **黄金值**：主密钥 → 子钥 → 身份键，整条链的输出被钉死。
///
/// 期望值由**两套独立实现**算出（Python `hmac`/`hashlib` 与 Node `crypto`，
/// 两者一致），不是拿被测函数当尺子。主密钥固定为 `[7u8; 32]`，与
/// [`test_derivation_key`] 同一把。
///
/// 这条同时钉住「子钥域标签 + 本人路径域标签 + 消息编码 + 委托路径域标签」四处：
/// 任何一处变了，键值就变——也就是一次全量身份键轮换。
#[test]
fn 身份键的黄金值() {
    let derivation = test_derivation_key();

    let personal = derive_identity_key(
        &derivation,
        &OrganizationId::new("org-a"),
        &ConnectionId::new("conn-1"),
        &PolicyIsolationKey::new("iso-1"),
    )
    .expect("本人身份键");
    assert_eq!(
        personal.as_str(),
        "mZfi6omatc6ygld69Tr4GdFZr0VOQhcRzYwxO0EFOpA",
        "本人身份键的黄金值变了：要么派生链被改，要么编码/域标签被改——两者都要求全量轮换身份键"
    );

    let delegated = derive_delegation_identity_key(
        &derivation,
        &OrganizationId::new("org-a"),
        &ConnectionId::new("conn-1"),
        &PolicyIsolationKey::new("iso-1"),
        &DelegationId::new("del-1"),
        "",
    )
    .expect("委托身份键");
    assert_eq!(
        delegated.as_str(),
        "9aYFoFtEr9AKXB6rtfOtB-Qcveigx5Ga_msMWg8API4",
        "委托身份键的黄金值变了：同上"
    );
}

/// **主密钥轮换 ⇒ 全部身份键变化**（共享边界 §4.3 新写下的那条约束）。
///
/// 这条把「主密钥一换，所有 `(organization, connection, isolation)` 的身份键
/// 全部变」从文档句子变成可执行断言：同一组输入，只换主密钥，键必须全变。
///
/// 当前仓库没有主密钥轮换路径（`store/key_store.rs` 只有 load-or-create），
/// 所以这条不是「轮换功能」的测试，而是**派生链对主密钥敏感**的测试。
#[test]
fn 主密钥一换全部身份键都变() {
    let key_for = |master: [u8; 32]| {
        let derivation = IdentityDerivationKey::from_master_key(&master).expect("派生盐");
        derive_identity_key(
            &derivation,
            &OrganizationId::new("datazen-desktop-local"),
            &ConnectionId::new("conn-42"),
            &PolicyIsolationKey::new("iso-9"),
        )
        .expect("身份键")
    };
    assert_ne!(
        key_for([7u8; 32]),
        key_for([8u8; 32]),
        "主密钥一换，身份键必须全变（否则一次轮换不会打空连接池，文档就是错的）"
    );
}

// ---------------------------------------------------------------------------
// 复测轮补：把 D-02 的分类落成可执行断言
// ---------------------------------------------------------------------------

/// **换域标签只改键的「值」，不改键的「关系」**——D-02 那条分类的可执行版本。
///
/// 共享边界 §4.3 与实现文件模块头都写着这条分类：
/// 「**进 HMAC 输入的那些**（域标签、长度前缀、子钥域分离、派生盐的来源）：改它们 ⇒
/// **每个正常路径调用的身份键值都变** ⇒ 等价于一次**全量身份键轮换**；
/// 正常路径用例断言的**关系**（同输入同键、跨组织不同键、委托 ≠ 本人）**不变**。」
///
/// 这条用**同一把派生盐、同一组输入**，只把域标签换掉，然后分别断言：
///
/// * **值**变 —— 域标签确实进了 HMAC 输入（不是摆设），换掉它就是一次全量轮换；
/// * **关系**不变 —— 同输入同键 / 跨组织不同键 / 委托 ≠ 本人 三条照旧成立。
///
/// 用例写的是**关系**，所以这条分类对用例来说是「看不见」的：只有把「值」与
/// 「关系」分开断言，才能把分类本身钉住。
#[test]
fn 换域标签只改键的值不改键的关系() {
    let derivation = test_derivation_key();
    let org = OrganizationId::new("org-a");
    let org_b = OrganizationId::new("org-b");
    let conn = ConnectionId::new("conn-1");
    let iso = PolicyIsolationKey::new("iso-1");
    let alternative_domain = b"datazen.execution-identity.key.v2";

    let key = |domain: &[u8], organization: &OrganizationId| {
        derivation
            .mac(
                domain,
                &[organization.as_str(), conn.as_str(), iso.as_str()],
            )
            .expect("身份键")
    };

    // 1. 值变：换域标签 ⇒ 同一组输入的身份键**值**不同。
    let current = key(IDENTITY_KEY_DOMAIN, &org);
    let alternative = key(alternative_domain, &org);
    assert_ne!(
        current, alternative,
        "域标签进 HMAC 输入 ⇒ 换域标签必须让身份键的值变（等价于一次全量身份键轮换）"
    );

    // 2. 关系不变：同输入同键。
    assert_eq!(
        alternative,
        key(alternative_domain, &org),
        "换域标签之后「同输入同键」这条关系必须仍然成立"
    );

    // 3. 关系不变：跨组织不同键。
    assert_ne!(
        alternative,
        key(alternative_domain, &org_b),
        "换域标签之后「跨组织不同键」这条关系必须仍然成立"
    );

    // 4. 关系不变：委托身份 ≠ 本人身份。
    let delegated = derivation
        .mac(
            DELEGATION_KEY_DOMAIN,
            &[
                org.as_str(),
                conn.as_str(),
                iso.as_str(),
                "del-1",
                "scope-1",
            ],
        )
        .expect("委托身份键");
    assert_ne!(
        alternative, delegated,
        "换域标签之后「委托身份 ≠ 本人身份」这条关系必须仍然成立"
    );
}
