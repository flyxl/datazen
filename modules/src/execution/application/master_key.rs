//! `MasterKeyProvider`：主密钥保管的窄端口。
//!
//! ## 为什么主密钥**不属** `DesktopRepository`
//!
//! 主密钥（系统钥匙串，或 `{appData}/.key`）是**密钥保管**职责，不是**数据存储**
//! 职责：桌面侧它由 `src-tauri/src/store/key_store.rs` 持有，而 `DesktopRepository` 持有的是
//! 连接档案、设置、历史等业务数据。把「取主密钥」挂在 `DesktopRepository` 上，等于让每一个只想
//! 派生一把子密钥的组件都拖着整个数据存储。
//!
//! 桌面现状：`src-tauri/src/platform/execution_identity.rs` 的 `for_desktop` 从
//! `DesktopRepository` 取主密钥字节，再用 HMAC-SHA256 做域分离派生出**只**用于执行身份派生的
//! 子钥——这正是本端口 `derive_subkey` 的形状。
//!
//! ## 边界
//!
//! * 端口只暴露「取主密钥 / 派生子密钥」，**不**暴露任何存储细节（钥匙串、文件、
//!   路径一律不进端口签名）。
//! * 材料**不可序列化**：[`KeyMaterial`] 刻意不实现 `Serialize` / `Deserialize`，
//!   `Debug` 只输出长度——材料一旦可序列化，就迟早被 `Debug`、事件载荷或错误上下文
//!   带出去（与 [`crate::execution::application::secret::ResolvedCredential`] 同一条纪律）。
//! * 派生是**域分离**的：同一 `label` 必须得到同一把子密钥，不同 `label` 必须不同。
//! * 端口**不重试、不鉴权、不缓存**：主密钥取不到就是取不到（失败关闭），**不**
//!   退回任何可预测的常量——常量盐会让派生结果变成公开可算的值。

use async_trait::async_trait;

use crate::shared::context::RequestContext;
use crate::shared::error::PortError;

/// 密钥材料。刻意**不实现** `Serialize` / `Deserialize`。
#[derive(Clone, PartialEq, Eq)]
pub struct KeyMaterial(Vec<u8>);

impl KeyMaterial {
    /// 构造。空串是否合法由各调用方校验，不在类型层拦截。
    pub fn new(material: impl Into<Vec<u8>>) -> Self {
        Self(material.into())
    }

    /// 借用材料字节。调用方负责消费后丢弃。
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// 消费并取回材料所有权，交由派生路径立刻使用。
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Debug for KeyMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "KeyMaterial(<redacted, {} bytes>)", self.0.len())
    }
}

/// 主密钥保管的窄端口。
#[async_trait]
pub trait MasterKeyProvider: Send + Sync + 'static {
    /// 取主密钥材料。
    ///
    /// 主密钥是**安装级**事实：`ctx` 在桌面形态只是占位（单租户），在多租户形态
    /// 可用来限定租户。端口本身**不做鉴权**——它只给事实。
    async fn master_key(&self, ctx: &RequestContext) -> Result<KeyMaterial, PortError>;

    /// 从主密钥派生一把子密钥。`label` 是**域分离标签**。
    ///
    /// 同一 `label` 必须得到**同一把**子密钥（否则跨进程重启的派生结果会漂）；
    /// 不同 `label` 必须得到不同的子密钥（否则两处用途共用一把钥匙）。
    async fn derive_subkey(
        &self,
        ctx: &RequestContext,
        label: &str,
    ) -> Result<KeyMaterial, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId};
    use std::sync::Arc;

    /// 端口必须**对象安全**：能以 `Arc<dyn Trait>` 注入（§4.1 的注入形态）。
    ///
    /// `?Sized` 是承重的：`dyn Trait` 本身不是 `Sized`，少了它这个断言退化成
    /// 「某个 `Sized` 类型满足这几个约束」，与对象安全无关。
    fn assert_object_safe<T: Send + Sync + 'static + ?Sized>() {}

    /// 去掉注释行后的源码。
    fn code_only(source: &str) -> String {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn context() -> RequestContext {
        RequestContext::new(
            OrganizationId::new("org-1"),
            PrincipalId::new("user-1"),
            None,
            ClientInstanceId::new("client-1"),
            RequestId::new("req-1"),
            None,
        )
    }

    struct StubKeyProvider {
        master: KeyMaterial,
    }

    #[async_trait]
    impl MasterKeyProvider for StubKeyProvider {
        async fn master_key(&self, _ctx: &RequestContext) -> Result<KeyMaterial, PortError> {
            Ok(self.master.clone())
        }

        async fn derive_subkey(
            &self,
            _ctx: &RequestContext,
            label: &str,
        ) -> Result<KeyMaterial, PortError> {
            // 域分离的最小实现：标签进摘要材料。
            let mut derived = self.master.as_bytes().to_vec();
            derived.extend_from_slice(label.as_bytes());
            Ok(KeyMaterial::new(derived))
        }
    }

    #[tokio::test]
    async fn the_port_is_object_safe_and_injectable_as_an_arc_trait_object() {
        assert_object_safe::<dyn MasterKeyProvider>();
        let provider: Arc<dyn MasterKeyProvider> = Arc::new(StubKeyProvider {
            master: KeyMaterial::new(vec![7u8; 32]),
        });
        let master = provider
            .master_key(&context())
            .await
            .expect("取主密钥失败必须是端口事实");
        assert_eq!(master.len(), 32);

        let subkey = provider
            .derive_subkey(&context(), "datazen.execution-identity.derivation-key.v1")
            .await
            .expect("派生子密钥失败必须是端口事实");
        assert!(!subkey.is_empty());
    }

    #[tokio::test]
    async fn the_same_label_derives_the_same_subkey_and_a_different_one_does_not() {
        let provider: Arc<dyn MasterKeyProvider> = Arc::new(StubKeyProvider {
            master: KeyMaterial::new(vec![7u8; 32]),
        });
        let first = provider
            .derive_subkey(&context(), "label-a")
            .await
            .expect("派生失败");
        let again = provider
            .derive_subkey(&context(), "label-a")
            .await
            .expect("派生失败");
        let other = provider
            .derive_subkey(&context(), "label-b")
            .await
            .expect("派生失败");
        assert_eq!(first, again);
        assert_ne!(first, other);
    }

    #[test]
    fn the_material_never_leaks_through_debug_or_serde() {
        let material = KeyMaterial::new(b"s3cret-key-material".to_vec());
        let debug = format!("{material:?}");
        assert_eq!(debug, "KeyMaterial(<redacted, 19 bytes>)");
        assert!(!debug.contains("s3cret"));

        // 刻意不实现 serde：材料一旦可序列化，就迟早被事件载荷或错误上下文带出去。
        let source = code_only(include_str!("master_key.rs"));
        let declaration = source
            .split("pub struct KeyMaterial")
            .nth(1)
            .expect("类型声明必须存在")
            .split(';')
            .next()
            .unwrap_or_default();
        assert!(
            !declaration.contains("Serialize"),
            "密钥材料不得带 Serialize：{declaration}"
        );
        assert!(
            !declaration.contains("Deserialize"),
            "密钥材料不得带 Deserialize：{declaration}"
        );
        // 也没有后置的 serde 实现。探针本身不得把被禁的字面量整段写进源码：
        // 拆成两段再拼，否则断言会命中自己。
        let direct_impl = ["impl Serialize for ", "KeyMaterial"].concat();
        let serde_impl = ["impl serde::Serialize for ", "KeyMaterial"].concat();
        assert!(!source.contains(&direct_impl));
        assert!(!source.contains(&serde_impl));
    }

    #[test]
    fn the_provider_declares_only_the_two_key_facts() {
        let source = code_only(include_str!("master_key.rs"));
        let declaration = source
            .split("pub trait MasterKeyProvider")
            .nth(1)
            .expect("trait 声明必须存在")
            .split("\n}")
            .next()
            .unwrap_or_default();
        assert_eq!(
            declaration.matches("async fn").count(),
            2,
            "主密钥端口只允许两个方法：{declaration}"
        );
        // 不暴露任何存储细节。
        let host_marker = ["taur", "i"].concat();
        for forbidden in ["crate::execution::ports::store", "keyring", "path", "Path"] {
            assert!(
                !declaration.contains(forbidden),
                "端口签名不得出现宿主类型 `{forbidden}`：{declaration}"
            );
        }
        assert!(
            !declaration.contains(&host_marker),
            "端口签名不得出现宿主类型 `{host_marker}`：{declaration}"
        );
    }
}
