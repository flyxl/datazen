//! 执行身份解析的宿主单测。
//!
//! 端口 `packages/platform-api/src/ports/identity.rs` 自带的 5 条语义在这里逐条重验。
//! 每条都写成**会自己变红**的断言：改坏实现（把 principal 拼进身份键、把凭据版本
//! 混进身份键、让委托路径转调本人路径）时，下面每一条都会点红。
//!
//! 替身只替换**策略端口**与**委托作用域绑定**：连接配置与凭据版本走真实的
//! `DesktopProfiles`（真 Store、真落盘），这样「轮换凭据只推进版本」测的是真实行为，
//! 不是替身的行为。

use std::collections::BTreeMap;
use std::sync::Arc;

use datazen_platform_api::{
    dto::profile::{ProfileDraft, ProfilePatch},
    id::{ClientInstanceId, ConfigRevision, IdempotencyKey, PrincipalId, RequestId, Timestamp},
    ports::policy::{
        AuthorizationAction, AuthorizationDecision, AuthorizationSubject, DelegationGrant,
        PolicyChangeStream,
    },
    target::NamespaceTarget,
};

use super::*;
use crate::platform::identity::LOCAL_ORGANIZATION_ID;
use crate::platform::repositories::DesktopProfiles;

/// 测试用密码。它只用来证明「凭据材料不进身份键」，本身是合成值。
const TEST_PASSWORD: &str = "hunter2-not-a-real-secret";

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

/// 一份真实的桌面装配：真 Store、真连接配置、真凭据版本。
struct Fixture {
    store: Arc<Store>,
    profiles: Arc<DesktopProfiles>,
    ctx: RequestContext,
    connection_id: ConnectionId,
    config_revision: u64,
    target: ExecutionTarget,
    /// 临时目录。放在最后声明 ⇒ 最后释放 ⇒ 不会在 Store 之前被删掉。
    _dir: tempfile::TempDir,
}

impl Fixture {
    /// 注入替身策略的解析器。派生盐取固定值，隔离键取固定值。
    fn resolver(&self) -> DesktopIdentityResolver {
        DesktopIdentityResolver::new(
            self.profiles.clone(),
            self.profiles.clone(),
            Arc::new(FixedPolicy::new(
                PolicyIsolationKey::new("test-isolation"),
                None,
            )),
            Arc::new(FixedScope::new(Some(self.target.clone()))),
            test_derivation_key(),
        )
    }

    /// 走桌面装配入口：派生盐取自**宿主主密钥**，因此同一份 Store 装出来的
    /// 解析器给出同一个身份键（安装级稳定、跨重启稳定）。
    fn desktop_resolver(&self) -> DesktopIdentityResolver {
        DesktopIdentityResolver::for_desktop(
            self.store.clone(),
            self.profiles.clone(),
            Arc::new(FixedPolicy::new(
                PolicyIsolationKey::new("test-isolation"),
                None,
            )),
        )
        .expect("桌面装配入口")
    }
}

fn test_derivation_key() -> IdentityDerivationKey {
    IdentityDerivationKey::from_master_key(&[7u8; 32]).expect("派生盐")
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("临时目录");
    let store = Arc::new(
        Store::init_with_path(dir.path())
            .await
            .expect("store 初始化"),
    );
    let profiles = Arc::new(DesktopProfiles::new(store.clone()));
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
    Fixture {
        store,
        profiles,
        ctx,
        connection_id: record.connection_id,
        config_revision: record.config_revision.get(),
        target,
        _dir: dir,
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

// ---------------------------------------------------------------------------
// 端口语义 1：身份键不得由客户端传入的 principal 派生
// ---------------------------------------------------------------------------

/// 同一组织、同一连接、同一隔离键下，**换一个请求主体身份键不变**。
///
/// 隔离键被固定，因此这条断言真正测的是「主体没有进派生输入」：
/// 若实现把 `principal` 拼进身份键，两个主体的身份键就会不同。
#[tokio::test]
async fn 身份键不随请求主体变化() {
    let fixture = fixture().await;
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
    let fixture = fixture().await;
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
    let fixture = fixture().await;
    let first = fixture
        .desktop_resolver()
        .resolve(&fixture.ctx, &fixture.target)
        .await
        .expect("第一次身份");
    let second = fixture
        .desktop_resolver()
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
    let fixture = fixture().await;
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

#[tokio::test]
async fn 轮换凭据只推进版本不动身份键() {
    let fixture = fixture().await;
    let resolver = fixture.resolver();

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

// ---------------------------------------------------------------------------
// 端口语义 4：resolve 与 resolve_delegation 是两条独立路径
// ---------------------------------------------------------------------------

#[tokio::test]
async fn 委托身份与本人身份不同且未授权委托失败关闭() {
    let fixture = fixture().await;
    let granted = delegation("del-1", LOCAL_ORGANIZATION_ID);
    let resolver = DesktopIdentityResolver::new(
        fixture.profiles.clone(),
        fixture.profiles.clone(),
        Arc::new(FixedPolicy::new(
            PolicyIsolationKey::new("test-isolation"),
            Some(granted.clone()),
        )),
        Arc::new(FixedScope::new(Some(fixture.target.clone()))),
        test_derivation_key(),
    );

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
    let fixture = fixture().await;
    let granted = delegation("del-1", LOCAL_ORGANIZATION_ID);
    let resolver = DesktopIdentityResolver::new(
        fixture.profiles.clone(),
        fixture.profiles.clone(),
        Arc::new(FixedPolicy::new(
            PolicyIsolationKey::new("test-isolation"),
            Some(granted.clone()),
        )),
        Arc::new(FixedScope::new(Some(fixture.target.clone()))),
        test_derivation_key(),
    );

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
    let fixture = fixture().await;
    let granted = delegation("del-1", LOCAL_ORGANIZATION_ID);
    let resolver = DesktopIdentityResolver::new(
        fixture.profiles.clone(),
        fixture.profiles.clone(),
        Arc::new(FixedPolicy::new(
            PolicyIsolationKey::new("test-isolation"),
            Some(granted.clone()),
        )),
        Arc::new(FixedScope::new(Some(fixture.target.clone()))),
        test_derivation_key(),
    );
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
    let fixture = fixture().await;
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
