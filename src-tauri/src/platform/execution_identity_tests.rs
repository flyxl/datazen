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
//! 会把 `platform::live_source::tests::*` 的**进程级 env 竞态**窗口撑大
//! （`hub.md` §8 记录的既有 flake：那个家族的 `DesktopUserGuard` 在 Drop 时把
//! `DATAZEN_DESKTOP_USER` 还原成「之前没有」，会把并行跑的兄弟用例的桌面主体读歪）。
//! 替身让本模块零 I/O；**只有**「凭据轮换」与「安装级稳定」两条需要真 Store，
//! 且共用同一份（`OnceCell`），全模块只建一次。
//!
//! 替身只替换**策略端口**、**连接配置仓储**、**凭据版本仓储**与**委托作用域绑定**；
//! 被测的 `DesktopIdentityResolver` 本体、派生函数与端口契约一字未改。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

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
