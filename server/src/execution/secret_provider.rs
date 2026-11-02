//! `SecretProvider` 的服务端适配器 + 版本化秘密材料的存储半边（WR-D4）。
//!
//! [team-server §10.2](../../../../../docs/architecture/platform/team-server-and-auth.md)
//! 的「服务端密钥管理适配」：桌面钥匙串路径在服务端不适用（无用户钥匙串），
//! 材料落管理库的 `stored_secret` / `stored_secret_version` 两表（v7），
//! **行内只有密文**。加密原语沿用本 crate 既有依赖（sha2 + hmac，与
//! `csrf.rs` / `policy` 的 server_secret 纪律一致）：
//!
//! - **KDF**：`enc_key = HMAC-SHA256(master, "datazen.server-secret.encryption-key.v1")`
//!   与 `mac_key = HMAC-SHA256(master, "…integrity-key.v1")` 两次域分离派生
//!   （HKDF-Extract 形态；master 由构造参数注入，不落库、不进日志——
//!   `Debug` 手写脱敏）。加密与完整性用两把独立子钥，同一把密钥不跑两种原语。
//! - **Sealed box**：随机 16 字节 nonce；keystream 块
//!   `HMAC(enc_key, nonce ‖ u32be(counter))` 与明文 XOR（HMAC 展开即
//!   HKDF-Expand 的合法用法）；tag = `HMAC(mac_key, nonce ‖ ciphertext)`
//!   **encrypt-then-MAC**，校验用常量时间比较（`subtle`）。AES-GCM 不在本
//!   crate 依赖闭包里，v1 用该构造承接「禁止明文落库」——生产化切片换
//!   KMS/AES 时只动本文件的 seal/open 两个函数。
//! - **版本语义**（端口注释逐条落地）：`resolve` **无缓存**、每次落库；
//!   `resolve` 只答**当前版本**；`read_versioned` 按指定历史版本回读且不改
//!   当前版本，版本不存在（含被 rotate 清掉的）一律 `CasConflict`——
//!   不得回退到当前版本；`rotate(expected)` CAS 递增并**删除全部旧版本行**
//!   （「旧版本材料立即失效」的字面实现）；`revision` 是 PoolKey 凭据分量的
//!   唯一来源，绝不用哈希或明文充当。
//!
//! ## 存储形状裁决（登记）
//!
//! * `SecretRef` 是**不透明** newtype，端口方法（`revision`）又不带
//!   `RequestContext`——因此本实现铸造的引用**自带组织段**：
//!   `server-vault:{kind}:{organization_id}:{id}`，跨组织解析在引用解析层
//!   就被拒（`NotFound`，CM-05 不可区分口径）。v1 `connection_profile` 铸造的
//!   `server-credential:` 形状不带组织段（profiles 刀的形状），本适配器对它
//!   回答 `NotFound`；profiles 侧材料落库随其切片接上。
//! * `SecretPurpose` 在 v1 只被接受、不参与差异：每个引用存**一份**材料 blob
//!   （JSON 形状的敏感字段集），用途级字段切分随生产化切片。端口测试钉住的
//!   约束不受影响（`ResolvedCredential` 无 serde、材料不进 Debug）。
//! * 材料的**写入**不在端口上（端口只有 resolve/read_versioned/rotate/revision
//!   四个读面）：写入走本适配器的固有方法
//!   （[`ServerSecretVault::sync_store_material`]），消费者是 WR-D2 的两仓储
//!   （连接目录 / 隧道定义在 save 时把敏感字段密封进来）与未来的管理用例。
//!   每次 store 分配**新 revision** 并保留旧版本行供 `read_versioned` 回读；
//!   rotate 是唯一删除旧材料的入口。

use std::sync::Arc;

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use rusqlite::{params, OptionalExtension};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use datazen::execution::application::secret::{ResolvedCredential, SecretProvider, SecretPurpose};
use datazen::shared::context::RequestContext;
use datazen::shared::error::PortError;
use datazen::shared::id::{CredentialRevision, SecretRef};

use crate::storage::AdminStore;

type HmacSha256 = Hmac<Sha256>;

/// KDF 域标签：加密子钥。
const ENCRYPTION_KEY_LABEL: &[u8] = b"datazen.server-secret.encryption-key.v1";
/// KDF 域标签：完整性子钥（与加密子钥分离）。
const INTEGRITY_KEY_LABEL: &[u8] = b"datazen.server-secret.integrity-key.v1";

/// 本适配器铸造的引用前缀。第二段是用途 kind（catalog / tunnel / …），
/// 第三段是组织，第四段起是宿主 id。
pub(crate) const VAULT_REF_PREFIX: &str = "server-vault:";

/// sealed box 的 nonce 字节数（128 位，与 AES-GCM 建议同级）。
const NONCE_BYTES: usize = 16;

/// 敏感字段键集合（serde camelCase 字面量）。连接表单与隧道定义共用这一份：
/// 与桌面 `connections.json` / `tunnels.json` 的逐字段加密口径对齐
/// （persistence-model §4.1），ssh `jump` 嵌套由递归自然覆盖。
pub(crate) const SENSITIVE_FIELD_KEYS: [&str; 3] = ["password", "passphrase", "authToken"];

/// 从 `config_json` 形状的值里**摘走**敏感字段，返回同构的敏感值树。
///
/// 递归进对象；数组不进（本词汇表里敏感字段不出现在数组元素里——driver-api
/// 的连接/隧道形状没有数组凭据字段）。被摘走的槽位**置 null**，剩余投影
/// 即可安全落 `config_json`。
pub(crate) fn extract_sensitive_fields(value: &mut serde_json::Value) -> serde_json::Value {
    let mut out = serde_json::Value::Object(serde_json::Map::new());
    extract_into(value, &mut out);
    out
}

fn extract_into(value: &mut serde_json::Value, out: &mut serde_json::Value) {
    let serde_json::Value::Object(map) = value else {
        return;
    };
    let Some(out_map) = out.as_object_mut() else {
        return;
    };
    for (key, slot) in map.iter_mut() {
        if SENSITIVE_FIELD_KEYS.contains(&key.as_str()) {
            if !slot.is_null() {
                out_map.insert(key.clone(), slot.take());
            }
        } else if slot.is_object() {
            let branch = out_map
                .entry(key.clone())
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            extract_into(slot, branch);
        }
    }
}

/// 把密封前摘走的敏感值树**拼回**净化投影（`get` 组装明文全量的另一半）。
///
/// 两侧同构：敏感树里的对象键递归下探，叶值直接覆盖（把 save 时的 null
/// 换回明文）。存储侧密文被篡改时 `open` 已先行失败，这里只做形状拼接。
pub(crate) fn splice_sensitive_fields(target: &mut serde_json::Value, secrets: &serde_json::Value) {
    let (Some(target_map), Some(secret_map)) = (target.as_object_mut(), secrets.as_object()) else {
        return;
    };
    for (key, secret_value) in secret_map {
        match target_map.get_mut(key) {
            Some(branch) if branch.is_object() && secret_value.is_object() => {
                splice_sensitive_fields(branch, secret_value);
            }
            _ => {
                target_map.insert(key.clone(), secret_value.clone());
            }
        }
    }
}

/// sealed box 的行内三元组（base64 标准形，与 `stored_secret_version` 的
/// 三列一一对应）。
struct SealedMaterial {
    nonce_b64: String,
    ciphertext_b64: String,
    tag_b64: String,
}

/// 版本化秘密材料的服务端保险库（`SecretProvider` 端口适配器 + 材料存储）。
pub struct ServerSecretVault {
    store: Arc<AdminStore>,
    enc_key: [u8; 32],
    mac_key: [u8; 32],
}

impl std::fmt::Debug for ServerSecretVault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 密钥材料永不进 Debug（与 ServerSecret / CsrfSecret 同一条纪律）。
        f.debug_struct("ServerSecretVault")
            .field("store", &"AdminStore")
            .field("enc_key", &"[redacted]")
            .field("mac_key", &"[redacted]")
            .finish()
    }
}

impl ServerSecretVault {
    /// 构造。`key_material` 是宿主注入的密钥原料（`ServerSecret::as_bytes`），
    /// **空原料一律拒绝**（fail-closed：空密钥的 HMAC 是公开常量）。
    pub fn new(store: Arc<AdminStore>, key_material: Vec<u8>) -> Result<Self, PortError> {
        if key_material.is_empty() {
            return Err(PortError::BackendUnavailable(
                "secret vault key material is empty".into(),
            ));
        }
        Ok(Self {
            store,
            enc_key: Self::derive(&key_material, ENCRYPTION_KEY_LABEL)?,
            mac_key: Self::derive(&key_material, INTEGRITY_KEY_LABEL)?,
        })
    }

    fn derive(material: &[u8], label: &[u8]) -> Result<[u8; 32], PortError> {
        let mut mac = HmacSha256::new_from_slice(material)
            .map_err(|_| PortError::BackendUnavailable("secret vault key rejected".into()))?;
        mac.update(label);
        let bytes = mac.finalize().into_bytes();
        bytes
            .as_slice()
            .try_into()
            .map_err(|_| PortError::BackendUnavailable("derived key length mismatch".into()))
    }

    /// 铸造自带组织段的引用：`server-vault:{kind}:{organization_id}:{id}`。
    pub(crate) fn mint_ref(kind: &str, organization_id: &str, id: &str) -> SecretRef {
        SecretRef::new(format!("{VAULT_REF_PREFIX}{kind}:{organization_id}:{id}"))
    }

    /// 从引用解析组织段与完整原文；非本适配器铸造的形状返回 `None`。
    fn ref_organization(secret_ref: &SecretRef) -> Option<(String, String)> {
        let raw = secret_ref.as_str().strip_prefix(VAULT_REF_PREFIX)?;
        let mut parts = raw.splitn(3, ':');
        let kind = parts.next()?;
        let organization = parts.next()?;
        let id = parts.next()?;
        if kind.is_empty() || organization.is_empty() || id.is_empty() {
            return None;
        }
        Some((organization.to_string(), raw.to_string()))
    }

    // ── 密封原语 ──

    fn seal(&self, material: &[u8]) -> Result<SealedMaterial, PortError> {
        use rand::RngCore;
        let mut nonce = [0u8; NONCE_BYTES];
        rand::rngs::OsRng.fill_bytes(&mut nonce);

        let mut ciphertext = Vec::with_capacity(material.len());
        let mut counter: u32 = 0;
        for chunk in material.chunks(32) {
            let mut mac = HmacSha256::new_from_slice(&self.enc_key)
                .map_err(|_| PortError::BackendUnavailable("keystream key rejected".into()))?;
            mac.update(&nonce);
            mac.update(&counter.to_be_bytes());
            counter = counter.wrapping_add(1);
            let keystream = mac.finalize().into_bytes();
            for (byte, pad) in chunk.iter().zip(keystream.iter()) {
                ciphertext.push(byte ^ pad);
            }
        }

        let mut mac = HmacSha256::new_from_slice(&self.mac_key)
            .map_err(|_| PortError::BackendUnavailable("integrity key rejected".into()))?;
        mac.update(&nonce);
        mac.update(&ciphertext);
        let tag = mac.finalize().into_bytes();

        Ok(SealedMaterial {
            nonce_b64: BASE64_STANDARD.encode(nonce),
            ciphertext_b64: BASE64_STANDARD.encode(&ciphertext),
            tag_b64: BASE64_STANDARD.encode(tag),
        })
    }

    fn open(&self, sealed: &SealedMaterial) -> Result<Vec<u8>, PortError> {
        let nonce = BASE64_STANDARD
            .decode(&sealed.nonce_b64)
            .map_err(|_| PortError::BackendUnavailable("sealed nonce is not decodable".into()))?;
        let ciphertext = BASE64_STANDARD
            .decode(&sealed.ciphertext_b64)
            .map_err(|_| {
                PortError::BackendUnavailable("sealed ciphertext is not decodable".into())
            })?;
        let tag = BASE64_STANDARD
            .decode(&sealed.tag_b64)
            .map_err(|_| PortError::BackendUnavailable("sealed tag is not decodable".into()))?;
        if nonce.len() != NONCE_BYTES {
            return Err(PortError::BackendUnavailable(
                "sealed nonce has an unexpected length".into(),
            ));
        }

        let mut mac = HmacSha256::new_from_slice(&self.mac_key)
            .map_err(|_| PortError::BackendUnavailable("integrity key rejected".into()))?;
        mac.update(&nonce);
        mac.update(&ciphertext);
        let expected = mac.finalize().into_bytes();
        if !bool::from(expected.as_slice().ct_eq(&tag)) {
            return Err(PortError::BackendUnavailable(
                "sealed material failed integrity verification".into(),
            ));
        }

        let mut material = Vec::with_capacity(ciphertext.len());
        let mut counter: u32 = 0;
        for chunk in ciphertext.chunks(32) {
            let mut block = HmacSha256::new_from_slice(&self.enc_key)
                .map_err(|_| PortError::BackendUnavailable("keystream key rejected".into()))?;
            block.update(&nonce);
            block.update(&counter.to_be_bytes());
            counter = counter.wrapping_add(1);
            let keystream = block.finalize().into_bytes();
            for (byte, pad) in chunk.iter().zip(keystream.iter()) {
                material.push(byte ^ pad);
            }
        }
        Ok(material)
    }

    // ── 同步核心（trait 实现只做 spawn_blocking 包装；WR-D2 仓储直接复用） ──

    pub(crate) fn sync_current_revision(
        &self,
        organization_id: &str,
        secret_ref: &SecretRef,
    ) -> Result<Option<u64>, PortError> {
        let conn = lock(&self.store)?;
        conn.query_row(
            "SELECT current_revision FROM stored_secret
             WHERE organization_id = ?1 AND secret_ref = ?2",
            params![organization_id, secret_ref.as_str()],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(backend)
        .map(|value| value.map(|v| v as u64))
    }

    pub(crate) fn sync_open_revision(
        &self,
        organization_id: &str,
        secret_ref: &SecretRef,
        revision: u64,
    ) -> Result<Option<Vec<u8>>, PortError> {
        let conn = lock(&self.store)?;
        let sealed = conn
            .query_row(
                "SELECT nonce_b64, ciphertext_b64, tag_b64 FROM stored_secret_version
                 WHERE organization_id = ?1 AND secret_ref = ?2 AND revision = ?3",
                params![organization_id, secret_ref.as_str(), revision as i64],
                |row| {
                    Ok(SealedMaterial {
                        nonce_b64: row.get(0)?,
                        ciphertext_b64: row.get(1)?,
                        tag_b64: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(backend)?;
        match sealed {
            Some(sealed) => Ok(Some(self.open(&sealed)?)),
            None => Ok(None),
        }
    }

    /// 写入一份新材料：分配**新 revision** 并保留旧版本行（回读语义）。
    /// 引用不存在时建引用行（首版 revision = 1）。
    pub(crate) fn sync_store_material(
        &self,
        organization_id: &str,
        secret_ref: &SecretRef,
        material: &[u8],
    ) -> Result<CredentialRevision, PortError> {
        let sealed = self.seal(material)?;
        let at = now_string();
        let mut conn = lock(&self.store)?;
        let tx = conn.transaction().map_err(backend)?;

        let next: u64 = tx
            .query_row(
                "SELECT current_revision FROM stored_secret
                 WHERE organization_id = ?1 AND secret_ref = ?2",
                params![organization_id, secret_ref.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(backend)?
            .map(|v| v as u64)
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| PortError::BackendUnavailable("credential revision exhausted".into()))?;

        if next == 1 {
            tx.execute(
                "INSERT INTO stored_secret
                   (organization_id, secret_ref, current_revision, created_at, updated_at)
                 VALUES (?1, ?2, 1, ?3, ?3)",
                params![organization_id, secret_ref.as_str(), at],
            )
            .map_err(backend)?;
        } else {
            tx.execute(
                "UPDATE stored_secret SET current_revision = ?3, updated_at = ?4
                 WHERE organization_id = ?1 AND secret_ref = ?2",
                params![organization_id, secret_ref.as_str(), next as i64, at],
            )
            .map_err(backend)?;
        }

        tx.execute(
            "INSERT INTO stored_secret_version
               (organization_id, secret_ref, revision, nonce_b64, ciphertext_b64, tag_b64, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                organization_id,
                secret_ref.as_str(),
                next as i64,
                sealed.nonce_b64,
                sealed.ciphertext_b64,
                sealed.tag_b64,
                at,
            ],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)?;
        Ok(CredentialRevision::new(next))
    }

    /// 删除一份引用的全部材料（连接/隧道删除路径的卫生动作）。
    pub(crate) fn sync_purge(
        &self,
        organization_id: &str,
        secret_ref: &SecretRef,
    ) -> Result<(), PortError> {
        let mut conn = lock(&self.store)?;
        let tx = conn.transaction().map_err(backend)?;
        // 版本行先删（子表），引用行随后；CASCADE 亦兜底。
        tx.execute(
            "DELETE FROM stored_secret_version WHERE organization_id = ?1 AND secret_ref = ?2",
            params![organization_id, secret_ref.as_str()],
        )
        .map_err(backend)?;
        tx.execute(
            "DELETE FROM stored_secret WHERE organization_id = ?1 AND secret_ref = ?2",
            params![organization_id, secret_ref.as_str()],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)?;
        Ok(())
    }

    fn sync_resolve(
        &self,
        ctx: &RequestContext,
        secret_ref: &SecretRef,
    ) -> Result<Vec<u8>, PortError> {
        let Some((ref_org, _)) = Self::ref_organization(secret_ref) else {
            return Err(PortError::NotFound("secret".into()));
        };
        if ref_org != ctx.organization_id.as_str() {
            // CM-05：越组织按「不存在」处理。
            return Err(PortError::NotFound("secret".into()));
        }
        let Some(revision) = self.sync_current_revision(&ref_org, secret_ref)? else {
            return Err(PortError::NotFound("secret".into()));
        };
        self.sync_open_revision(&ref_org, secret_ref, revision)?
            .ok_or_else(|| PortError::NotFound("secret material".into()))
    }

    fn sync_read_versioned(
        &self,
        ctx: &RequestContext,
        secret_ref: &SecretRef,
        revision: u64,
    ) -> Result<Vec<u8>, PortError> {
        let Some((ref_org, _)) = Self::ref_organization(secret_ref) else {
            return Err(PortError::NotFound("secret".into()));
        };
        if ref_org != ctx.organization_id.as_str() {
            return Err(PortError::NotFound("secret".into()));
        }
        self.sync_open_revision(&ref_org, secret_ref, revision)?
            .ok_or_else(|| {
                // 版本不存在（从未有过 / 已被 rotate 清除）：调用方持有的版本
                // 与存储不一致，一律 CasConflict，绝不回退到当前版本。
                PortError::cas_conflict("secret", secret_ref.as_str())
            })
    }

    fn sync_rotate(
        &self,
        ctx: &RequestContext,
        secret_ref: &SecretRef,
        expected: u64,
    ) -> Result<u64, PortError> {
        let Some((ref_org, _)) = Self::ref_organization(secret_ref) else {
            return Err(PortError::NotFound("secret".into()));
        };
        if ref_org != ctx.organization_id.as_str() {
            return Err(PortError::NotFound("secret".into()));
        }
        let mut conn = lock(&self.store)?;
        let tx = conn.transaction().map_err(backend)?;
        let current: u64 = tx
            .query_row(
                "SELECT current_revision FROM stored_secret
                 WHERE organization_id = ?1 AND secret_ref = ?2",
                params![ref_org, secret_ref.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(backend)?
            .ok_or_else(|| PortError::NotFound("secret".into()))? as u64;
        if current != expected {
            return Err(PortError::cas_conflict("secret", secret_ref.as_str()));
        }
        let next = current
            .checked_add(1)
            .ok_or_else(|| PortError::BackendUnavailable("credential revision exhausted".into()))?;
        // 旧版本材料立即失效：全部历史版本行删除，当前槽位暂无材料
        //（新材料的写入走 sync_store_material，随后的 resolve 才有值）。
        tx.execute(
            "DELETE FROM stored_secret_version WHERE organization_id = ?1 AND secret_ref = ?2",
            params![ref_org, secret_ref.as_str()],
        )
        .map_err(backend)?;
        tx.execute(
            "UPDATE stored_secret SET current_revision = ?3, updated_at = ?4
             WHERE organization_id = ?1 AND secret_ref = ?2 AND current_revision = ?5",
            params![
                ref_org,
                secret_ref.as_str(),
                next as i64,
                now_string(),
                current as i64
            ],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)?;
        Ok(next)
    }
}

/// 管理库连接锁（`storage::profiles` 同款）。
fn lock(store: &AdminStore) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, PortError> {
    store
        .lock_for_adapter()
        .map_err(|source| PortError::BackendUnavailable(source.to_string()))
}

fn backend(source: impl std::fmt::Display) -> PortError {
    PortError::BackendUnavailable(source.to_string())
}

fn now_string() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[async_trait]
impl SecretProvider for ServerSecretVault {
    async fn resolve(
        &self,
        ctx: &RequestContext,
        secret_ref: SecretRef,
        purpose: SecretPurpose,
    ) -> Result<ResolvedCredential, PortError> {
        // v1 裁决：purpose 被接受但不参与字段切分（每个引用一份材料 blob）。
        let _ = purpose;
        let ctx = ctx.clone();
        let this = self.clone_for_blocking();
        tokio::task::spawn_blocking(move || this.sync_resolve(&ctx, &secret_ref))
            .await
            .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
            .map(ResolvedCredential::new)
    }

    async fn read_versioned(
        &self,
        ctx: &RequestContext,
        secret_ref: SecretRef,
        revision: CredentialRevision,
    ) -> Result<ResolvedCredential, PortError> {
        let ctx = ctx.clone();
        let this = self.clone_for_blocking();
        tokio::task::spawn_blocking(move || {
            this.sync_read_versioned(&ctx, &secret_ref, revision.get())
        })
        .await
        .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
        .map(ResolvedCredential::new)
    }

    async fn rotate(
        &self,
        ctx: &RequestContext,
        secret_ref: SecretRef,
        expected: CredentialRevision,
    ) -> Result<CredentialRevision, PortError> {
        let ctx = ctx.clone();
        let this = self.clone_for_blocking();
        tokio::task::spawn_blocking(move || {
            this.sync_rotate(&ctx, &secret_ref, expected.get())
                .map(CredentialRevision::new)
        })
        .await
        .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
    }

    async fn revision(&self, secret_ref: SecretRef) -> Result<CredentialRevision, PortError> {
        // 端口签名不带 ctx：组织隔离由引用内的组织段承载。
        let Some((ref_org, _)) = Self::ref_organization(&secret_ref) else {
            return Err(PortError::NotFound("secret".into()));
        };
        let this = self.clone_for_blocking();
        tokio::task::spawn_blocking(move || {
            this.sync_current_revision(&ref_org, &secret_ref)?
                .map(CredentialRevision::new)
                .ok_or_else(|| PortError::NotFound("secret".into()))
        })
        .await
        .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
    }
}

impl ServerSecretVault {
    /// `spawn_blocking` 需要 `'static` 克隆：`AdminStore` 本就是 Arc 共享，
    /// 派生密钥按字节复制（32 字节 ×2，无引用共享）。
    fn clone_for_blocking(&self) -> ServerSecretVault {
        ServerSecretVault {
            store: Arc::clone(&self.store),
            enc_key: self.enc_key,
            mac_key: self.mac_key,
        }
    }
}
