//! 执行身份解析：`IdentityResolver` 端口的桌面实现。
//!
//! ## 端口契约落到本文件的义务
//!
//! `packages/platform-api/src/ports/identity.rs` 的 `IdentityResolver` 是
//! `executionIdentityKey` 的**唯一生产者**（概要 §6.4 / CM §4.1）。本文件实现它，
//! 并把端口自带的五条语义逐条钉在代码里：
//!
//! 1. **身份键不得由客户端传入的 principal 派生**。派生输入只有
//!    `(organization_id, connection_id, policy_isolation_key)` 三项，外加服务端持有的
//!    派生盐。用户名、角色、密码、token 一律不进入（见 [`derive_identity_key`]）。
//! 2. **同一 `(organization, connection, isolation)` 稳定返回同一身份键**。派生是
//!    确定性的 HMAC-SHA256：不含随机数、不含时间戳、不含进程内计数器，因此
//!    跨进程重启不变——INV-02 的会话连续性与池复用都依赖这一点。
//! 3. **凭据轮换只推进 `credential_revision`，不动身份键**。身份键的输入里没有凭据
//!    版本；轮换只体现在 [`ExecutionIdentity::credential_revision`] 上。
//! 4. **`resolve` 与 `resolve_delegation` 是两条独立路径**，不是一个 flag。
//!    两条路径的域标签与输入集都不同（委托路径额外绑定委托 id 与授权作用域），
//!    因此委托身份**不可能**等于本人身份（CM-06）。
//! 5. **`ExecutionIdentity` 不含秘密材料**。身份键是 HMAC 摘要而不是密钥本身；
//!    `ExecutionIdentityKey` / `PolicyIsolationKey` 的 `Debug` 本就脱敏。
//!
//! ## 服务端固定盐从哪来
//!
//! 桌面形态的**服务端固定盐**取宿主主密钥（`{appData}/.key` 或系统钥匙串，见
//! `src-tauri/src/store/key_store.rs`），再用 HMAC-SHA256 做一次**域分离**派生
//! （HKDF-Extract 形态：`HMAC(master, label)`），得到一把**只**用于执行身份派生的子钥。
//!
//! * **稳定**：主密钥是安装级、跨进程重启不变的 ⇒ 身份键跨重启稳定。
//! * **不可伪造**：客户端拿不到主密钥就算不出身份键；身份键本身也不是客户端可传的字段。
//! * **不含材料**：主密钥与子钥都**不进入**身份键；身份键是子钥对输入字段做的 HMAC
//!   摘要，摘要不可逆推主密钥，也不可逆推密码。
//!
//! 派生盐与 AES 主密钥同源但**用途分离**：AES 侧用 `Aes256Gcm`，身份侧先用 HMAC
//! 派生出独立子钥，子钥只喂给身份键派生。两边不会拿同一把密钥跑两种原语。
//!
//! **主密钥轮换 ⇒ 全部身份键变化**：主密钥一换，所有 `(organization, connection, isolation)`
//! 的身份键全部变，连接池与缓存整体失效。当前仓库没有主密钥轮换路径
//! （`src-tauri/src/store/key_store.rs` 只有 load-or-create，跨后端迁移也保留原密钥字节），
//! 故暂无实际风险；将来若引入轮换，必须同时给身份派生一把按 `keyVersion` 轮换的独立密钥，
//! 否则一次轮换就会把全部连接池打空。
//!
//! ## 两条路径为什么必须分开
//!
//! 委托路径除了换域标签，还额外绑定**委托 id** 与**授权作用域**：换一个委托（或授权
//! 范围变了）身份键就变，池与缓存因此不会把两个权限范围混用。委托主体拿不到调用者
//! 本人的身份键，也就继承不了本人的全量权限（CM-06）。
//!
//! `resolve` 在**委托上下文**上一律拒绝：委托身份只能由 `resolve_delegation` 产出，
//! 否则两条路径会在同一个入口上产出同一族身份，「两条独立路径」就成了空话。
//!
//! ## 改这个文件之前先看变异
//!
//! 上面这些性质里，**长度前缀**、**域分离**、**跨组织委托的显式检查**、**多连接失败关闭**
//! 这几条，用例若只写「正常路径的行为」，是**钉不住的**：去掉其中任何一个，正常路径的
//! 输出完全不变，正常路径用例照绿。首版 16 条用例就漏了它们——是变异（去输入 /
//! 去长度前缀 / 去域分离 / 放宽失败关闭）把它们暴露出来的，随后补了 8 条针对性用例。
//!
//! **因此：改派生编码、域标签或失败关闭分支时，请连带跑变异，而不是只看用例是否全绿。**
//! 具体的变异点与对应用例见 `execution_identity_tests.rs` 的用例名。

use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use base64::{
    engine::general_purpose::STANDARD as BASE64_STANDARD,
    engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL, Engine,
};
use datazen_platform_api::{
    context::{DelegationRef, RequestContext},
    error::PortError,
    id::{
        ConnectionId, CredentialRevision, DelegationId, ExecutionIdentityKey, OrganizationId,
        PolicyIsolationKey,
    },
    ports::{
        identity::{ExecutionIdentity, IdentityResolver},
        policy::PolicyService,
        profile::{ProfileRecord, ProfileRepository},
        secret::SecretProvider,
    },
    target::ExecutionTarget,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::store::Store;

type HmacSha256 = Hmac<Sha256>;

/// 域分离标签：把宿主主密钥变成「只用于执行身份派生」的子钥。
const DERIVATION_KEY_LABEL: &[u8] = b"datazen.execution-identity.derivation-key.v1";
/// 域标签：**本人**路径。
const IDENTITY_KEY_DOMAIN: &[u8] = b"datazen.execution-identity.key.v1";
/// 域标签：**委托**路径。与本人路径刻意不同 ⇒ 两条路径的输出永不相等。
const DELEGATION_KEY_DOMAIN: &[u8] = b"datazen.execution-identity.delegation-key.v1";

/// 执行身份派生盐。
///
/// 只从宿主主密钥派生，**不落盘、不进日志、不进任何可序列化结构**。
/// `Debug` 刻意脱敏：它一旦被打印出去，等于把主密钥的一部分交给日志系统。
pub struct IdentityDerivationKey([u8; 32]);

impl IdentityDerivationKey {
    /// 从宿主主密钥派生（HMAC-SHA256 域分离，HKDF-Extract 形态）。
    ///
    /// 失败关闭：主密钥不可用时**不**退回任何可预测的常量盐——常量盐会让身份键
    /// 变成公开可算的值，等于把「不可伪造」这条丢掉。
    pub fn from_master_key(master_key: &[u8; 32]) -> Result<Self, PortError> {
        let mut mac = HmacSha256::new_from_slice(master_key).map_err(|_| {
            PortError::BackendUnavailable("identity derivation key rejected".into())
        })?;
        mac.update(DERIVATION_KEY_LABEL);
        let bytes = mac.finalize().into_bytes();
        let derived: [u8; 32] = bytes.as_slice().try_into().map_err(|_| {
            PortError::BackendUnavailable("identity derivation key has an unexpected length".into())
        })?;
        Ok(Self(derived))
    }

    /// 对一组字段做域分离 HMAC，返回 URL-safe base64 摘要。
    ///
    /// 输出是**摘要**：它不可逆推主密钥，也不携带任何明文秘密。
    fn mac(&self, domain: &[u8], parts: &[&str]) -> Result<String, PortError> {
        let mut mac = HmacSha256::new_from_slice(&self.0).map_err(|_| {
            PortError::BackendUnavailable("identity derivation key rejected".into())
        })?;
        mac.update(&encode_message(domain, parts));
        Ok(BASE64_URL.encode(mac.finalize().into_bytes()))
    }
}

impl fmt::Debug for IdentityDerivationKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("IdentityDerivationKey(<redacted>)")
    }
}

/// 把字段编码成**无歧义**的字节串：域标签 + 每字段的长度前缀 + 字段内容。
///
/// 长度前缀是必需的：直接用分隔符拼接会让 `("a", "b|c")` 与 `("a|b", "c")` 撞成
/// 同一条消息，于是两个不同的身份输入会派生出同一个身份键。
fn encode_message(domain: &[u8], parts: &[&str]) -> Vec<u8> {
    let mut message = Vec::with_capacity(domain.len() + 64);
    message.extend_from_slice(domain);
    for part in parts {
        let bytes = part.as_bytes();
        let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
        message.extend_from_slice(&len.to_be_bytes());
        message.extend_from_slice(bytes);
    }
    message
}

/// **本人路径**的身份键派生。
///
/// 输入**只有**三项：组织、连接、策略隔离键；再加服务端派生盐，别的一概不进。
///
/// 特别地：**没有 principal、没有密码、没有 token、没有连接串**。
/// 策略隔离键本身是 `PolicyService` 的产物（服务端判定），它按用户分桶是架构要求
/// （连接 §9.6：共享 DB 账号但权限不同的用户之间隔离键必须不同）——但它由服务端
/// 决定，客户端无法指定，也无从参与本函数的入参。
fn derive_identity_key(
    derivation: &IdentityDerivationKey,
    organization_id: &OrganizationId,
    connection_id: &ConnectionId,
    policy_isolation_key: &PolicyIsolationKey,
) -> Result<ExecutionIdentityKey, PortError> {
    let digest = derivation.mac(
        IDENTITY_KEY_DOMAIN,
        &[
            organization_id.as_str(),
            connection_id.as_str(),
            policy_isolation_key.as_str(),
        ],
    )?;
    Ok(ExecutionIdentityKey::new(digest))
}

/// **委托路径**的身份键派生。
///
/// 与 [`derive_identity_key`] 完全独立：域标签不同、输入集不同。委托身份额外绑定
/// **委托 id** 与**授权作用域**，因此：
///
/// * 委托身份 ≠ 本人身份（域标签不同 ⇒ HMAC 消息不同 ⇒ 摘要必不同）；
/// * 换一个委托、或授权范围变了 ⇒ 身份键变 ⇒ 池与缓存不会把两个权限范围混用；
/// * 委托主体拿不到调用者本人的身份键，也就继承不了本人的全量权限（CM-06）。
fn derive_delegation_identity_key(
    derivation: &IdentityDerivationKey,
    organization_id: &OrganizationId,
    connection_id: &ConnectionId,
    policy_isolation_key: &PolicyIsolationKey,
    delegation_id: &DelegationId,
    scope: &str,
) -> Result<ExecutionIdentityKey, PortError> {
    let digest = derivation.mac(
        DELEGATION_KEY_DOMAIN,
        &[
            organization_id.as_str(),
            connection_id.as_str(),
            policy_isolation_key.as_str(),
            delegation_id.as_str(),
            scope,
        ],
    )?;
    Ok(ExecutionIdentityKey::new(digest))
}

/// 取宿主主密钥的原始字节。
///
/// 桌面形态的**服务端固定盐**就是它。取字节而不是 base64 字符串，是为了让域分离
/// 派生作用在真正的密钥字节上。
fn master_key_bytes(store: &Store) -> Result<[u8; 32], PortError> {
    let encoded = store.encryption_key_b64();
    let decoded = BASE64_STANDARD
        .decode(encoded.as_bytes())
        .map_err(|_| PortError::BackendUnavailable("host master key is not decodable".into()))?;
    decoded.try_into().map_err(|_| {
        PortError::BackendUnavailable("host master key has an unexpected length".into())
    })
}

/// 委托作用域绑定：把一个已校验的委托落到它被授权的执行目标上。
///
/// 端口的 `resolve_delegation` **不接收** `ExecutionTarget`，而 `ExecutionIdentity`
/// 必须带 `connection_id`——端口层不替实现方决定这份事实从哪来，因此宿主把它显式
/// 做成一个协作者，而不是让实现方去猜或去填。
#[async_trait]
pub trait DelegationScopeBinding: Send + Sync + 'static {
    /// 该委托被授权的执行目标。委托没有绑定目标 ⇒ 失败关闭。
    async fn authorized_target(
        &self,
        ctx: &RequestContext,
        delegation: &DelegationRef,
    ) -> Result<ExecutionTarget, PortError>;
}

/// 桌面形态的执行身份解析器。
///
/// 依赖全部经端口注入（`ProfileRepository` / `SecretProvider` / `PolicyService`），
/// 因此多租户形态可以换掉其中任何一个而不必改本实现。
pub struct DesktopIdentityResolver {
    profiles: Arc<dyn ProfileRepository>,
    secrets: Arc<dyn SecretProvider>,
    policy: Arc<dyn PolicyService>,
    delegation_scope: Arc<dyn DelegationScopeBinding>,
    derivation: Arc<IdentityDerivationKey>,
}

impl DesktopIdentityResolver {
    /// 显式注入全部协作者。测试与「换一把派生盐」的形态走这条。
    pub fn new(
        profiles: Arc<dyn ProfileRepository>,
        secrets: Arc<dyn SecretProvider>,
        policy: Arc<dyn PolicyService>,
        delegation_scope: Arc<dyn DelegationScopeBinding>,
        derivation: IdentityDerivationKey,
    ) -> Self {
        Self {
            profiles,
            secrets,
            policy,
            delegation_scope,
            derivation: Arc::new(derivation),
        }
    }

    /// 桌面组装入口：派生盐取自宿主主密钥，委托作用域取自监控授权单。
    ///
    /// 失败关闭：主密钥取不到就**不**退回可预测的常量盐——那会让身份键变成公开
    /// 可算的值。调用方据此**让整个装配失败**，而不是接一个假的身份解析器。
    ///
    /// 失败半径：这个 `Err` 会被 `PlatformAdapter::inject` 的 `?` 放大成**整个平台
    /// 装配**的失败（`profiles` / `runtime` / `services` 一并装不上），不是
    /// 「仅身份解析能力缺席」。要缩小半径，得把这一步改成可降级的接线点。
    pub fn for_desktop(
        store: Arc<Store>,
        profiles: Arc<super::repositories::DesktopProfiles>,
        policy: Arc<dyn PolicyService>,
    ) -> Result<Self, PortError> {
        let derivation = IdentityDerivationKey::from_master_key(&master_key_bytes(&store)?)?;
        let profile_port: Arc<dyn ProfileRepository> = profiles.clone();
        let secret_port: Arc<dyn SecretProvider> = profiles;
        Ok(Self::new(
            profile_port,
            secret_port,
            policy,
            Arc::new(MonitorDelegationScope::new(store)),
            derivation,
        ))
    }

    /// 取连接配置。跨组织访问与「不存在」同样返回 NotFound（CM-05：不泄漏
    /// 「存在但无权」）。已停用的连接不发执行身份——身份是「可以发起执行」的凭证。
    async fn profile(
        &self,
        ctx: &RequestContext,
        connection_id: &ConnectionId,
    ) -> Result<ProfileRecord, PortError> {
        let record = self
            .profiles
            .get(ctx, connection_id.clone())
            .await?
            .ok_or_else(|| PortError::NotFound("profile".into()))?;
        if !record.enabled {
            return Err(PortError::NotFound("profile".into()));
        }
        Ok(record)
    }

    /// 凭据版本。**只**取版本号，不取材料。
    ///
    /// 有 `SecretRef` 时以 `SecretProvider` 的版本为准（端口分工：凭据版本由
    /// `SecretProvider` 持有并计算）；没有配置凭据的连接退回配置记录上的版本。
    /// 两条路都只给出版本号，材料一步也不碰。
    async fn credential_revision(
        &self,
        profile: &ProfileRecord,
    ) -> Result<CredentialRevision, PortError> {
        match profile.secret_ref.clone() {
            Some(secret_ref) => self.secrets.revision(secret_ref).await,
            None => Ok(profile.credential_revision),
        }
    }
}

#[async_trait]
impl IdentityResolver for DesktopIdentityResolver {
    async fn resolve(
        &self,
        ctx: &RequestContext,
        target: &ExecutionTarget,
    ) -> Result<ExecutionIdentity, PortError> {
        // 委托上下文必须走 `resolve_delegation`。在这里拒绝，是为了让「两条独立
        // 路径」不只是两个方法名：本人路径永远不会替委托主体产出身份。
        if ctx.is_delegated() {
            return Err(PortError::TokenInvalid);
        }
        let profile = self.profile(ctx, &target.connection_id).await?;
        let policy_isolation_key = self.policy.isolation_key(ctx)?;
        let execution_identity_key = derive_identity_key(
            &self.derivation,
            &ctx.organization_id,
            &target.connection_id,
            &policy_isolation_key,
        )?;
        Ok(ExecutionIdentity {
            organization_id: ctx.organization_id.clone(),
            connection_id: target.connection_id.clone(),
            execution_identity_key,
            policy_isolation_key,
            credential_revision: self.credential_revision(&profile).await?,
        })
    }

    async fn resolve_delegation(
        &self,
        ctx: &RequestContext,
        delegation: DelegationRef,
    ) -> Result<ExecutionIdentity, PortError> {
        // CM-06：委托先经策略端口校验（被委托主体、作用域、有效期三者都要匹配）。
        // 校验不过**不得**降级为调用者本人的权限。
        if delegation.organization_id != ctx.organization_id {
            // 跨组织的委托引用一律拒绝：委托凭据不得跨组织使用。
            return Err(PortError::TokenInvalid);
        }
        self.policy
            .verify_delegation(ctx, delegation.clone())
            .await?;
        // 委托被授权的目标（端口不接收目标，由宿主协作者给出权威事实）。
        let target = self
            .delegation_scope
            .authorized_target(ctx, &delegation)
            .await?;
        let profile = self.profile(ctx, &target.connection_id).await?;
        let policy_isolation_key = self.policy.isolation_key(ctx)?;
        // 独立派生：域标签与输入集都与本人路径不同，因此委托身份 ≠ 本人身份。
        let execution_identity_key = derive_delegation_identity_key(
            &self.derivation,
            &ctx.organization_id,
            &target.connection_id,
            &policy_isolation_key,
            &delegation.delegation_id,
            &delegation.scope,
        )?;
        Ok(ExecutionIdentity {
            organization_id: ctx.organization_id.clone(),
            connection_id: target.connection_id.clone(),
            execution_identity_key,
            policy_isolation_key,
            credential_revision: self.credential_revision(&profile).await?,
        })
    }
}

impl fmt::Debug for DesktopIdentityResolver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 派生盐刻意不出现在这里：Debug 会进日志。
        f.debug_struct("DesktopIdentityResolver")
            .field("derivation", &self.derivation)
            .finish_non_exhaustive()
    }
}

/// 桌面形态的委托作用域绑定：监控授权单（monitor grant）登记的 scope 目标。
///
/// 监控调度是桌面形态里唯一会产生**委托执行**的来源：用户把某个 widget 的定时
/// 刷新授权给 `monitor:<id>:<rev>` 服务身份，授权单里登记了它被允许触达的目标
/// （连接 + 命名空间 + 命令）。
pub struct MonitorDelegationScope {
    store: Arc<Store>,
}

impl MonitorDelegationScope {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl DelegationScopeBinding for MonitorDelegationScope {
    async fn authorized_target(
        &self,
        ctx: &RequestContext,
        delegation: &DelegationRef,
    ) -> Result<ExecutionTarget, PortError> {
        let grant = crate::monitor::grants::current(&self.store, ctx)
            .map_err(|_| PortError::TokenInvalid)?;
        if &grant.delegation() != delegation {
            return Err(PortError::TokenInvalid);
        }
        // 一个执行身份只对应**一个连接**。授权单若覆盖多个连接，它无法映射成单一
        // 身份 ⇒ 失败关闭，绝不取第一个了事。
        let mut connection: Option<ConnectionId> = None;
        for scope in &grant.scopes {
            match &connection {
                Some(existing) if *existing != scope.target.connection_id => {
                    return Err(PortError::TokenInvalid);
                }
                _ => connection = Some(scope.target.connection_id.clone()),
            }
        }
        let connection = connection.ok_or(PortError::TokenInvalid)?;
        let target = grant
            .scopes
            .into_iter()
            .next()
            .map(|scope| scope.target)
            .ok_or(PortError::TokenInvalid)?;
        Ok(ExecutionTarget {
            connection_id: connection,
            ..target
        })
    }
}

// 用例搬出本文件：生产半边要自己就落在 800 行以内，测试半边单独成文件。
// `#[path]` 让 `super::` 与 `include_str!("execution_identity.rs")` 的解析基准都不变。
#[cfg(test)]
#[path = "execution_identity_tests.rs"]
mod tests;
