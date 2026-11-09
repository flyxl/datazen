//! id_token claims 校验的**手工收尾段**与 claims → 身份映射（web-server-host §4.2）。
//!
//! §4.2 校验表由两层闭合：签名 / `iss` / `aud` / `exp` / 时钟偏移交给
//! `jsonwebtoken` crate（[`super::oidc`] 里构造 `Validation`）；本模块补
//! crate 不覆盖的四项——`nonce`（重放的 id_token 因此不可用）、`azp`
//! （多 audience 时必须等值于本服务 client id）、`sub` 非空、`iat` 区间
//! （不得超出时钟偏移容差出现在未来）。
//!
//! ## 身份映射（§4.2 membership 行）
//!
//! 外部 subject（`sub`）必须映射到本组织内**已启用**的 membership，否则
//! 拒绝登录——**不在登录时隐式开户**。查询走管理库 `memberships` 表
//! （`PolicyService::create_membership` 的落库处，`external_subject` 列即
//! 为本用途设计）；`Role` 解码用 [`crate::policy::Role::from_db`] 的既有
//! 词表，不另立角色名。对外一律使用 `principalId`，不转发 IdP 原始标识。
//!
//! ## `client_binding`（§4.3）
//!
//! v1 形态 = `sha256(User-Agent) 的 hex | "web-v1"`。完整的「浏览器实例
//! 指纹（UA 摘要 + 客户端实例标识）」需要持久的实例 cookie，属于 §4.5
//! 风险信号切片；本形态保证列非空（存储层要求），并保留 UA 突变检测的
//! 输入位。

use rusqlite::params;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::policy::Role;
use crate::storage::{AdminStore, StorageError};

/// §4.2 手工校验项的拒绝原因。**仅供日志与审计**：HTTP 层把全部变体统一
/// 映射为 401 脱敏文案（§9.2 的「OIDC 校验失败」行），不区分细节。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimsRejection {
    /// `sub` 缺失或为空。
    MissingSubject,
    /// `nonce` 与登录事务记录不一致（含缺失）。
    NonceMismatch,
    /// `aud`/`azp` 与本服务 client id 不一致（多 audience 缺 `azp` 同罪）。
    AudienceMismatch,
    /// `iat` 超出时钟偏移容差出现在未来。
    IssuedInFuture,
}

/// id_token 的 claims 投影：只取本刀消费的字段，未知字段由 serde 忽略
/// （IdP 自由扩展自己的 claim，RP 不得因未知字段拒绝）。
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct IdTokenClaims {
    pub iss: String,
    pub sub: String,
    /// 字符串或字符串数组（OIDC Core 两种 wire 形态都合法）。
    #[serde(default)]
    pub aud: serde_json::Value,
    pub exp: i64,
    #[serde(default)]
    pub iat: Option<i64>,
    #[serde(default)]
    pub nonce: Option<String>,
    #[serde(default)]
    pub azp: Option<String>,
    /// IdP 认证时刻（unix 秒；§4.3 `auth_time` 的来源，缺失回落到当前）。
    #[serde(default)]
    pub auth_time: Option<i64>,
}

impl IdTokenClaims {
    /// `aud` 的取值集合（两种 wire 形态归一）。
    pub fn audience_values(&self) -> Vec<&str> {
        match &self.aud {
            serde_json::Value::String(single) => vec![single.as_str()],
            serde_json::Value::Array(items) => items
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        }
    }
}

/// 常量时间相等比较（§5.1 同一原语：`state` 与 `nonce` 都经它比较——
/// 手写 XOR 折叠不进安全面，与 `crate::csrf` 同一条纪律）。
pub(crate) fn constant_time_eq(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

/// §4.2 手工校验四项（`nonce` / `azp` / `sub` / `iat`）。`now_secs` 与
/// `leeway_secs` 由调用方注入（部署配置的时钟偏移容差，§4.2「允许 60 秒」
/// 是首版建议值而非协议常量）。
pub fn verify_nonce_azp_subject_and_iat(
    claims: &IdTokenClaims,
    expected_nonce: &str,
    client_id: &str,
    now_secs: u64,
    leeway_secs: u64,
) -> Result<(), ClaimsRejection> {
    // sub：稳定主体标识，缺失即无法映射（映射结果是否在册由 membership 决定）。
    if claims.sub.trim().is_empty() {
        return Err(ClaimsRejection::MissingSubject);
    }
    // nonce：与登录事务记录一致（重放的 id_token 携带别的 nonce 或缺 nonce）。
    match claims.nonce.as_deref() {
        Some(nonce) if constant_time_eq(nonce, expected_nonce) => {}
        _ => return Err(ClaimsRejection::NonceMismatch),
    }
    // azp：出现即必须等值于本服务 client id；多 audience 时必须出现。
    let audience = claims.audience_values();
    match claims.azp.as_deref() {
        Some(azp) if !constant_time_eq(azp, client_id) => {
            return Err(ClaimsRejection::AudienceMismatch);
        }
        Some(_) => {}
        None if audience.len() > 1 => return Err(ClaimsRejection::AudienceMismatch),
        None => {}
    }
    // iat：签名时刻不得「来自未来」（超出偏移容差）——时钟漂移的 IdP
    // 与伪造的 exp/iat 都被这一条拦下；缺失合法（OIDC Core 中 iat 可选）。
    if let Some(iat) = claims.iat {
        if iat < 0 || iat > (now_secs as i64) + (leeway_secs as i64) {
            return Err(ClaimsRejection::IssuedInFuture);
        }
    }
    Ok(())
}

/// claims → 身份映射的结果（§4.2 membership 行的通过面）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedIdentity {
    pub organization_id: String,
    pub principal_id: String,
    pub role: Role,
}

/// 外部 subject → 本组织已启用 membership 的映射（§4.2：不在登录时隐式
/// 开户）。无启用行 → `Ok(None)`（HTTP 层 403）；`role_id` 越出词表是
/// 存储事实错误（与 [`crate::policy`] 的读侧同款处理）。
pub fn resolve_membership(
    store: &AdminStore,
    organization_id: &str,
    external_subject: &str,
) -> Result<Option<ResolvedIdentity>, StorageError> {
    if external_subject.trim().is_empty() {
        return Ok(None);
    }
    let conn = store.lock_for_adapter()?;
    let rows = conn
        .prepare(
            "SELECT principal_id, role_id, enabled FROM memberships
             WHERE organization_id = ?1 AND external_subject = ?2",
        )
        .and_then(|mut stmt| {
            stmt.query_map(params![organization_id, external_subject], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? != 0,
                ))
            })
            .and_then(|mapped| mapped.collect::<Result<Vec<_>, _>>())
        })
        .map_err(|source| StorageError::Query(format!("claims: membership lookup: {source}")))?;

    let mut enabled: Vec<(String, Role)> = Vec::new();
    for (principal_id, role_raw, is_enabled) in rows {
        if !is_enabled {
            continue; // 禁用的 membership 不参与映射（§4.2 / §4.5）
        }
        let role = Role::from_db(&role_raw).ok_or_else(|| {
            StorageError::Query(format!(
                "claims: memberships has an unknown role `{role_raw}`"
            ))
        })?;
        enabled.push((principal_id, role));
    }
    // 多个不同 principal 命中同一 external_subject = 部署配置歧义：对请求方
    // 一律按「无启用 membership」回 403（不泄漏配置内部事实），运维事实由
    // 调用方落日志。
    let mut distinct = enabled.clone();
    distinct.dedup_by(|a, b| a.0 == b.0);
    if distinct.len() > 1 {
        tracing::warn!(
            organization = %organization_id,
            "external_subject maps to multiple enabled principals; refusing the login"
        );
        return Ok(None);
    }
    Ok(distinct
        .into_iter()
        .next()
        .map(|(principal_id, role)| ResolvedIdentity {
            organization_id: organization_id.to_owned(),
            principal_id,
            role,
        }))
}

/// §4.3 `client_binding` 的 v1 形态：UA 摘要（hex）+ 实例标识占位。永远
/// 非空（存储层 `client_binding must not be blank`）。
pub fn user_agent_digest(user_agent: Option<&str>) -> String {
    let digest = Sha256::digest(user_agent.unwrap_or("").as_bytes());
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("ua:{hex}|web-v1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn store() -> Arc<AdminStore> {
        let store = AdminStore::open_in_memory().expect("in-memory store");
        store.migrate().expect("migrations apply");
        Arc::new(store)
    }

    fn claims(nonce: Option<&str>, azp: Option<&str>, aud: serde_json::Value) -> IdTokenClaims {
        IdTokenClaims {
            iss: "https://idp.example".into(),
            sub: "idp-sub-1".into(),
            aud,
            exp: 1_800_000_000,
            iat: Some(1_700_000_000),
            nonce: nonce.map(str::to_owned),
            azp: azp.map(str::to_owned),
            auth_time: None,
        }
    }

    #[test]
    fn nonce_azp_and_subject_checks_reject_mismatches() {
        let good = claims(
            Some("n-1"),
            Some("datazen-web"),
            serde_json::Value::String("datazen-web".into()),
        );
        assert_eq!(
            verify_nonce_azp_subject_and_iat(&good, "n-1", "datazen-web", 1_700_000_000, 60),
            Ok(())
        );
        // nonce 不匹配 / 缺失。
        assert_eq!(
            verify_nonce_azp_subject_and_iat(&good, "n-2", "datazen-web", 1_700_000_000, 60),
            Err(ClaimsRejection::NonceMismatch)
        );
        let no_nonce = claims(
            None,
            Some("datazen-web"),
            serde_json::Value::String("datazen-web".into()),
        );
        assert_eq!(
            verify_nonce_azp_subject_and_iat(&no_nonce, "n-1", "datazen-web", 1_700_000_000, 60),
            Err(ClaimsRejection::NonceMismatch)
        );
        // azp 与 client id 不一致。
        let wrong_azp = claims(
            Some("n-1"),
            Some("other-client"),
            serde_json::Value::String("datazen-web".into()),
        );
        assert_eq!(
            verify_nonce_azp_subject_and_iat(&wrong_azp, "n-1", "datazen-web", 1_700_000_000, 60),
            Err(ClaimsRejection::AudienceMismatch)
        );
        // sub 为空。
        let mut no_sub = good.clone();
        no_sub.sub = "   ".into();
        assert_eq!(
            verify_nonce_azp_subject_and_iat(&no_sub, "n-1", "datazen-web", 1_700_000_000, 60),
            Err(ClaimsRejection::MissingSubject)
        );
    }

    #[test]
    fn multi_audience_requires_an_azp_equal_to_the_client_id() {
        let multi = claims(
            Some("n-1"),
            Some("datazen-web"),
            serde_json::json!(["datazen-web", "other-party"]),
        );
        assert_eq!(
            verify_nonce_azp_subject_and_iat(&multi, "n-1", "datazen-web", 1_700_000_000, 60),
            Ok(())
        );
        // 多 audience 缺 azp → 拒绝（§4.2 azp 行）。
        let multi_no_azp = claims(
            Some("n-1"),
            None,
            serde_json::json!(["datazen-web", "other-party"]),
        );
        assert_eq!(
            verify_nonce_azp_subject_and_iat(
                &multi_no_azp,
                "n-1",
                "datazen-web",
                1_700_000_000,
                60
            ),
            Err(ClaimsRejection::AudienceMismatch)
        );
        // 多 audience 但 azp 指向别家 → 拒绝。
        let multi_wrong_azp = claims(
            Some("n-1"),
            Some("other-party"),
            serde_json::json!(["datazen-web", "other-party"]),
        );
        assert_eq!(
            verify_nonce_azp_subject_and_iat(
                &multi_wrong_azp,
                "n-1",
                "datazen-web",
                1_700_000_000,
                60
            ),
            Err(ClaimsRejection::AudienceMismatch)
        );
    }

    #[test]
    fn an_iat_beyond_the_leeway_in_the_future_is_rejected() {
        let mut claims = claims(
            Some("n-1"),
            Some("datazen-web"),
            serde_json::Value::String("datazen-web".into()),
        );
        claims.iat = Some(1_700_000_061);
        // now=1_700_000_000，容差 60 → iat 恰好越界 1 秒。
        assert_eq!(
            verify_nonce_azp_subject_and_iat(&claims, "n-1", "datazen-web", 1_700_000_000, 60),
            Err(ClaimsRejection::IssuedInFuture)
        );
        claims.iat = Some(1_700_000_060);
        assert_eq!(
            verify_nonce_azp_subject_and_iat(&claims, "n-1", "datazen-web", 1_700_000_000, 60),
            Ok(()),
            "容差边界内（含）必须通过"
        );
        claims.iat = None;
        assert_eq!(
            verify_nonce_azp_subject_and_iat(&claims, "n-1", "datazen-web", 1_700_000_000, 60),
            Ok(()),
            "iat 缺失合法（OIDC Core 可选）"
        );
    }

    #[test]
    fn membership_mapping_resolves_enabled_rows_only() {
        let store = store();
        let policy = crate::policy::PolicyService::new(
            store.clone(),
            b"test-server-secret-32-bytes-minimum!!".to_vec(),
        )
        .expect("policy");
        policy
            .create_membership("org-a", "user-1", Role::OrgMember, "idp-sub-1")
            .expect("membership");
        policy
            .create_membership("org-a", "user-2", Role::OrgAdmin, "idp-sub-2")
            .expect("membership");
        policy
            .set_membership_enabled(
                "org-a",
                "user-2",
                false,
                crate::storage::login_sessions::RevokeReason::Admin,
            )
            .expect("disable");

        // 启用行 → principalId + 角色。
        let resolved = resolve_membership(&store, "org-a", "idp-sub-1")
            .expect("query")
            .expect("resolved");
        assert_eq!(resolved.principal_id, "user-1");
        assert_eq!(resolved.role, Role::OrgMember);
        assert_eq!(resolved.organization_id, "org-a");
        // 禁用行 → None（§4.2：不在登录时隐式开户）。
        assert_eq!(
            resolve_membership(&store, "org-a", "idp-sub-2").expect("query"),
            None
        );
        // 未登记 subject → None。
        assert_eq!(
            resolve_membership(&store, "org-a", "idp-sub-ghost").expect("query"),
            None
        );
        // 跨组织查询不命中（组织隔离）。
        assert_eq!(
            resolve_membership(&store, "org-b", "idp-sub-1").expect("query"),
            None
        );
        // 空 subject → None（不触碰存储语义）。
        assert_eq!(
            resolve_membership(&store, "org-a", "  ").expect("query"),
            None
        );
    }

    #[test]
    fn audience_values_normalize_the_string_and_array_wire_forms() {
        let single = claims(None, None, serde_json::Value::String("a".into()));
        assert_eq!(single.audience_values(), vec!["a"]);
        let multi = claims(None, None, serde_json::json!(["a", "b"]));
        assert_eq!(multi.audience_values(), vec!["a", "b"]);
        let weird = claims(None, None, serde_json::json!(1));
        assert!(weird.audience_values().is_empty());
    }

    #[test]
    fn user_agent_digest_is_stable_hex_and_never_blank() {
        let first = user_agent_digest(Some("Mozilla/5.0"));
        let again = user_agent_digest(Some("Mozilla/5.0"));
        assert_eq!(first, again, "同 UA 的摘要必须稳定");
        assert!(first.starts_with("ua:") && first.ends_with("|web-v1"));
        assert_ne!(first, user_agent_digest(Some("Other/1.0")));
        assert_eq!(
            user_agent_digest(None),
            user_agent_digest(Some("")),
            "缺头与空串同形（摘要仍非空）"
        );
    }
}
