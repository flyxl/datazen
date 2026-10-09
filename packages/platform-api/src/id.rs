//! 平台契约层的身份 newtype。
//!
//! `docs/architecture/naming.md` 的 ID 术语规范在这里落地：`connectionId` 是持久化连接配置 ID，
//! `dbSessionId` 是运行时会话 ID（永不落盘）。两者、乃至与其它 ID 之间**不能互换**，而这一条
//! 由类型系统保证：不同语义的 ID 是不同 newtype，本模块**不提供任何跨类型 `From` 实现**。
//!
//! 序列化统一 `#[serde(transparent)]` + camelCase 字段名（见各 DTO 模块），
//! JSON 形态与 `connection-management.md` §4 的 TypeScript 契约逐字段一致。
//!
//! 迁移提示（不属于本 crate 的范围）：`packages/runtime/src/connection/types.rs` 目前自带一份
//! 同名 ID 定义。等 runtime 迁移完成后应改为 `pub use datazen_platform_api::*`；在此之前两份定义
//! 不可互换，因此**不得跨 crate 传递**这两套 ID 的值。

use serde::de::{Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Borrow;
use std::fmt;

/// 定义一个字符串 ID newtype。刻意不实现 `From` 到其它 ID 类型。
macro_rules! string_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// 构造。空串是否合法由各 ID 的调用方校验，不在类型层拦截。
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl Borrow<str> for $name {
            fn borrow(&self) -> &str {
                &self.0
            }
        }
    };
}

/// 定义一个**不透明**字符串 ID：内容是密钥材料或签名材料，`Debug` 必须脱敏，
/// 防止 `{:?}` 进入日志（`connection-management.md` §4.3 的 `executionIdentityKey`、§13.1 的幂等令牌）。
macro_rules! opaque_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// 构造权属于**端口实现方**。用例层不得凭空构造执行身份或幂等键，
            /// 只能经 `IdentityResolver` / `SubmissionTokenIssuer` 取得。
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "(<redacted>)"))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }
    };
}

pub(crate) use opaque_id;
pub(crate) use string_id;

/// 定义一个语义不同的版本号 newtype，底层仍是 [`Counter`]。
///
/// 「不同语义不可互换」与 ID 一致：`ConfigRevision` 与 `CredentialRevision` 都是 `u64`，
/// 但混用会让配置版本与凭据版本互相顶替，因此必须各自独立成类型。
macro_rules! version_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Counter);

        impl $name {
            pub const fn new(value: u64) -> Self {
                Self(Counter::new(value))
            }

            pub const fn counter(self) -> Counter {
                self.0
            }

            pub const fn get(self) -> u64 {
                self.0.get()
            }
        }

        impl From<Counter> for $name {
            fn from(value: Counter) -> Self {
                Self(value)
            }
        }

        impl From<$name> for Counter {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

pub(crate) use version_id;

/// 64 位计数器。JSON 编码为**十进制字符串**（CM-01）：
/// JavaScript 的 `number` 超过 2^53 会丢精度，计数器必须原样穿过前端。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Counter(pub u64);

impl Counter {
    pub const ZERO: Counter = Counter(0);

    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn saturating_increment(self) -> Counter {
        Counter(self.0.saturating_add(1))
    }

    pub const fn saturating_increment_by(self, delta: u64) -> Counter {
        Counter(self.0.saturating_add(delta))
    }
}

impl fmt::Display for Counter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl From<u64> for Counter {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl Serialize for Counter {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Counter {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct CounterVisitor;

        impl Visitor<'_> for CounterVisitor {
            type Value = Counter;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a 64-bit counter encoded as a decimal string")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Counter, E> {
                value
                    .parse::<u64>()
                    .map(Counter)
                    .map_err(|_| E::invalid_value(Unexpected::Str(value), &self))
            }

            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Counter, E> {
                Ok(Counter(value))
            }

            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Counter, E> {
                u64::try_from(value)
                    .map(Counter)
                    .map_err(|_| E::invalid_value(Unexpected::Signed(value), &self))
            }
        }

        deserializer.deserialize_any(CounterVisitor)
    }
}

/// ISO-8601（或 driver 可解析的等价形式）时间戳字符串。
///
/// 契约层**不解析时刻**：比较与算术由各端口实现方负责。`Ord` 是字符串序，
/// 因此凡是参与排序（`sweep_expired(now)`、`delete_expired(now)`、单调 deadline）
/// 的时间戳都必须由生产方写成定宽规范化形式。
string_id!(
    /// 时刻标记。
    Timestamp
);

// ---------------------------------------------------------------------------------------------
// 身份
// ---------------------------------------------------------------------------------------------

/// 租户组织。
string_id!(
    /// 租户组织 ID。
    OrganizationId
);
/// 发起请求的自然人 / 服务主体。
string_id!(
    /// 主体 ID。
    PrincipalId
);
/// 认证会话（OIDC 登录会话等）；未认证上下文为 `None`。
string_id!(
    /// 认证会话 ID。
    AuthenticationSessionId
);
/// 客户端实例。**不是授权依据**（概要 §6.1），只用于定位 owner 与事件流。
string_id!(
    /// 客户端实例 ID。
    ClientInstanceId
);
/// 委托关系 ID（CM-06）。
string_id!(
    /// 委托 ID。
    DelegationId
);
/// 单次请求的关联 ID，落进 `ApiError.requestId` 与事件。
string_id!(
    /// 请求 ID。
    RequestId
);

// ---------------------------------------------------------------------------------------------
// 连接与会话（两种语义严格分离）
// ---------------------------------------------------------------------------------------------

/// 持久化连接配置 ID（落盘）。配置/归属/调度语义一律用它。
string_id!(
    /// 持久化连接配置 ID。
    ConnectionId
);
/// 运行时会话 ID（内存态，**永不落盘**）。操作已建立连接时一律用它。
string_id!(
    /// 运行时数据库会话 ID。
    DbSessionId
);
/// 会话纪元；资源丢失/替换后递增，使旧 `SessionHandle` 立即失效（INV-07/INV-08）。
string_id!(
    /// 运行时纪元 ID。
    RuntimeEpoch
);
/// 编辑器页签会话 ID（`OwnerRef::Editor` 的归属）。
string_id!(
    /// 编辑器会话 ID。
    EditorSessionId
);

// ---------------------------------------------------------------------------------------------
// 执行与资源
// ---------------------------------------------------------------------------------------------

string_id!(
    /// 执行 ID。
    ExecutionId
);
string_id!(
    /// 物理资源句柄 ID。
    ResourceId
);
string_id!(
    /// 租约 ID（物理会话连续性的凭据，INV-02）。
    LeaseId
);
string_id!(
    /// 事件流 ID。
    StreamId
);
string_id!(
    /// 结果块写入句柄 ID。
    HandleId
);

// ---------------------------------------------------------------------------------------------
// 工作流与任务
// ---------------------------------------------------------------------------------------------

string_id!(
    /// 任务 ID。
    JobId
);
string_id!(
    /// 任务阶段 ID。
    StageId
);
string_id!(
    /// 工作流块 ID。
    BlockId
);
string_id!(
    /// 工作流轮次 ID。
    RunId
);
string_id!(
    /// worker 节点 ID。
    WorkerId
);

// ---------------------------------------------------------------------------------------------
// 产物与秘密
// ---------------------------------------------------------------------------------------------

string_id!(
    /// 结果产物 ID。
    ArtifactId
);
opaque_id!(
    /// 秘密引用。`SecretProvider` 只认它，不认 `ConnectionId`；
    /// `ConnectionId → SecretRef` 的映射由 `ProfileRecord` 持有。
    SecretRef
);
opaque_id!(
    /// 出站路由引用。
    NetworkRouteRef
);
opaque_id!(
    /// 幂等键。签名令牌的载荷，客户端不得自造语义。
    IdempotencyKey
);
opaque_id!(
    /// 服务端生成、不可伪造的执行身份键；PoolKey 的组件，不使用客户端传入用户名（CM §4）。
    ExecutionIdentityKey
);
opaque_id!(
    /// 策略隔离键。授权与缓存淘汰按它分桶，订阅者据此淘汰缓存键。
    PolicyIsolationKey
);
opaque_id!(
    /// 驱动资源键。`describeResource` 对 CanonicalTarget / 初始化基线规范化后的产物，
    /// 是 PoolKey 的组件（CM §9.6）。**禁止**用连接串、主机名或任何凭据散列充当它——
    /// 数据库绑定资源按 database 分 key，同一连接的不同 database 因此天然落到不同池。
    DriverResourceKey
);
opaque_id!(
    /// attachment 令牌原文。只在创建/替换时返回给原 owner，存储侧只留哈希。
    AttachmentToken
);

// ---------------------------------------------------------------------------------------------
// 版本号（语义不同的计数器，互不可换）
// ---------------------------------------------------------------------------------------------

version_id!(
    /// 连接配置版本。CAS 更新与 `executeAtTarget.expectedConfigRevision` 使用。
    ConfigRevision
);
version_id!(
    /// 凭据版本。由 `SecretProvider` 持有并计算；**禁止用密码散列充当 PoolKey 组件**。
    CredentialRevision
);
version_id!(
    /// 出站路由版本。改动路由语义（白名单、隧道）时递增。
    NetworkRouteRevision
);
version_id!(
    /// 任务状态版本，任务状态 CAS 的比较基准。
    JobStateVersion
);
version_id!(
    /// 幂等令牌签名密钥版本。未知版本一律拒绝（概要 §6.4）。
    KeyVersion
);
version_id!(
    /// 能力快照版本（驱动能力声明的修订号）。
    CapabilityRevision
);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn counter_serializes_as_a_decimal_string() {
        let counter = Counter::new(9_007_199_254_740_993); // 2^53 + 1
        let encoded = serde_json::to_value(counter).expect("counter serializes");
        assert_eq!(encoded, json!("9007199254740993"));
        assert!(encoded.is_string());
    }

    #[test]
    fn counter_round_trips_without_precision_loss() {
        let original = Counter::new(u64::MAX);
        let text = serde_json::to_string(&original).expect("serialize");
        let decoded: Counter = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(decoded, original);
        assert_eq!(decoded.get(), u64::MAX);
    }

    #[test]
    fn counter_rejects_a_non_numeric_string() {
        let err = serde_json::from_str::<Counter>("\"abc\"").expect_err("must fail");
        assert!(err.to_string().contains("counter"), "{err}");
    }

    #[test]
    fn version_newtypes_are_distinct_from_counter() {
        let config = ConfigRevision::new(7);
        let credential = CredentialRevision::new(7);
        // 语义不同的版本号数值相同也不可互换：这里只能靠断言类型系统的事实说明。
        assert_eq!(config.counter(), credential.counter());
        assert_ne!(
            std::any::TypeId::of::<ConfigRevision>(),
            std::any::TypeId::of::<CredentialRevision>()
        );
        // 版本 newtype 也不是 `Counter` 的别名：否则 `Counter` 会成为它们的公共可变入口。
        assert_ne!(
            std::any::TypeId::of::<ConfigRevision>(),
            std::any::TypeId::of::<Counter>()
        );
        assert_ne!(
            std::any::TypeId::of::<Counter>(),
            std::any::TypeId::of::<CredentialRevision>()
        );
    }

    #[test]
    fn connection_id_and_db_session_id_are_distinct_types() {
        let connection = ConnectionId::new("conn-1");
        let session = DbSessionId::new("sess-1");
        assert_eq!(connection.as_str(), "conn-1");
        assert_eq!(session.as_str(), "sess-1");
        // 类型不同即不可互换：`ConnectionId` 无法当作 `DbSessionId` 传入，
        // 这条保证由缺失的 `From` 实现给出，测试只能固定两者值不相同。
        assert_ne!(connection.as_str(), session.as_str());
        assert_ne!(
            std::any::TypeId::of::<ConnectionId>(),
            std::any::TypeId::of::<DbSessionId>()
        );
    }

    #[test]
    fn opaque_ids_redact_their_debug_output() {
        let key = IdempotencyKey::new("super-secret-key");
        assert_eq!(format!("{key:?}"), "IdempotencyKey(<redacted>)");
        // 序列化仍需无损往返，机密性只靠日志脱敏，不靠序列化变形。
        let text = serde_json::to_string(&key).expect("serialize");
        assert_eq!(text, "\"super-secret-key\"");
    }

    #[test]
    fn string_ids_are_serde_transparent() {
        let id = OrganizationId::new("org-1");
        assert_eq!(serde_json::to_value(&id).expect("v"), json!("org-1"));
        let back: OrganizationId = serde_json::from_str("\"org-1\"").expect("d");
        assert_eq!(back, id);
        assert_eq!(id.to_string(), "org-1");
    }
}
