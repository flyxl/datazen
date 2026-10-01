//! 连接运行时的传输中立类型：newtype、DTO、enum、schema 校验。
//!
//! 来源：connection-management.md §4（DTO）/ §5.1（资源端口返回形状）/ §6.5（句柄登记）。
//! fake-runtime-fixtures.md §2 把本文件标为「`testing/` 之下唯一允许被夹具依赖的上游模块」。
//!
//! ID 术语纪律（AGENTS.md「ID 术语规范」）：`connectionId` 是持久化配置 id，
//! `dbSessionId` 是内存态运行时会话 id（**永不落盘**）。两者是不同的 newtype，
//! 互相赋值不可能通过编译，因此**结构上不存在双模回退**。

use serde::{Deserialize, Serialize};

use crate::connection::error::{ApiError, ApiErrorCode};

/// 声明一个字符串 ID newtype。不同 ID 类型之间不可互换，编译期即拒绝混用。
macro_rules! string_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
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

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

string_id!(
    /// 持久化连接配置 id（原 `configId`，落盘）。
    ConnectionId
);
string_id!(
    /// 运行时数据库会话 id（内存态，**永不落盘**）。
    DbSessionId
);
string_id!(
    /// 不透明资源 id；只有 provider 校验，宿主不得解析其内容。
    ResourceId
);
string_id!(
    /// 租约 id；actor 承担与 Lease 同等的句柄责任。
    LeaseId
);
string_id!(
    /// 执行 id。
    ExecutionId
);
string_id!(
    /// 会话级句柄 id（事务 / 游标 / 服务端预处理对象）。
    HandleId
);
string_id!(
    /// 事件流 id。
    StreamId
);
string_id!(
    /// Job id。
    JobId
);
string_id!(
    /// 产物 id。
    ArtifactId
);
string_id!(
    /// 组织 id。
    OrganizationId
);
string_id!(
    /// 主体（用户）id。
    PrincipalId
);
string_id!(
    /// 客户端实例 id。
    ClientInstanceId
);
string_id!(
    /// 编辑器会话 id。
    EditorSessionId
);
string_id!(
    /// worker id；夹具 ID 生成式里的 `<workerId>`。
    WorkerId
);
string_id!(
    /// workflow run id。
    RunId
);
string_id!(
    /// workflow block id。
    BlockId
);
string_id!(
    /// 对外 UTC 时间投影（ISO-8601）。只用于 `expiresAt` 一类投影，**不用于期限判定**。
    Timestamp
);

/// 64 位计数器。
///
/// 线上形状是**十进制字符串**而不是 JSON number：JavaScript 的 `number` 在 `2^53` 以上丢精度，
/// CM-01 要求计数往返序列化后不丢精度，因此序列化侧固定走字符串。
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

    /// 递增并返回新值。
    pub fn increment(&mut self) -> Counter {
        self.0 = self.0.saturating_add(1);
        *self
    }
}

impl std::fmt::Display for Counter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for Counter {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for Counter {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl serde::de::Visitor<'_> for V {
            type Value = Counter;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("十进制计数字符串或无符号整数")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Counter, E> {
                v.parse::<u64>().map(Counter).map_err(serde::de::Error::custom)
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Counter, E> {
                Ok(Counter(v))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Counter, E> {
                u64::try_from(v).map(Counter).map_err(serde::de::Error::custom)
            }
        }
        deserializer.deserialize_any(V)
    }
}

/// 配置版本号。`PROFILE_P` = 7、`PROFILE_P_V2` = 8 时必须换 `poolKeyFingerprint`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ConfigRevision(pub u64);

impl ConfigRevision {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for ConfigRevision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for ConfigRevision {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for ConfigRevision {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Counter::deserialize(deserializer).map(|Counter(v)| Self(v))
    }
}

/// FNV-1a 64 位哈希，十六进制输出。夹具里所有「稳定指纹」都走它，保证跨运行完全确定。
pub fn fnv1a64_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// 命名空间层。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NamespaceLayer {
    Database,
    Catalog,
    Schema,
    Path,
}

impl NamespaceLayer {
    pub const ALL: [NamespaceLayer; 4] = [
        NamespaceLayer::Database,
        NamespaceLayer::Catalog,
        NamespaceLayer::Schema,
        NamespaceLayer::Path,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            NamespaceLayer::Database => "database",
            NamespaceLayer::Catalog => "catalog",
            NamespaceLayer::Schema => "schema",
            NamespaceLayer::Path => "path",
        }
    }
}

/// 驱动声明的目标形状：哪些层必填、哪些层可选（connection-management.md §5.1 `namespaceShape`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceShape {
    pub required: Vec<NamespaceLayer>,
    #[serde(default)]
    pub optional: Vec<NamespaceLayer>,
}

impl NamespaceShape {
    pub fn new(required: impl IntoIterator<Item = NamespaceLayer>) -> Self {
        Self { required: required.into_iter().collect(), optional: Vec::new() }
    }

    /// Postgres / MySQL 形态：database + schema 必填。
    pub fn database_and_schema() -> Self {
        Self::new([NamespaceLayer::Database, NamespaceLayer::Schema])
    }

    pub fn requires(&self, layer: NamespaceLayer) -> bool {
        self.required.contains(&layer)
    }
}

/// 「观测不可判定」时的**结构性占位**常量。
///
/// §4.2 规定 `NamespaceTarget` 四层必填且**空串非法**，所以「观测为 unknown」
/// 不能靠空串表达，只能靠一个显式的、绝不可能与真实命名空间相等的哨兵值。
/// 任何持有该值的上下文必须同时带 `ContextConfidence::Unknown`，
/// 因此它**永远不可能**被当成「已确认目标」使用（§3.2 L137 / §5.1 L417）。
pub const UNKNOWN_SENTINEL: &str = "dz_unknown";

/// 目标命名空间。四个字段全部必填，空串非法（connection-management.md §4.2）。
///
/// `Default` 得到四个空串层，即「**未指定任何层**」的形状 —— 与 `from_shape` 里
/// `String::new()` 的初始值同义。它**不是**一个合法目标：`from_shape` 仍会按
/// `NamespaceShape` 的必填层校验并对空串报 `TargetRequired`，因此 derive 只是
/// 提供构造起点，不构成绕过 §4.2 校验的捷径。
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceTarget {
    pub database: String,
    pub catalog: String,
    pub schema: String,
    /// driver 的命名空间 id，**不是文件系统路径**（§4.2）。
    pub path: String,
}

impl NamespaceTarget {
    /// 观测不可判定时的占位目标。**不是** `initialTarget` 的回填。
    pub fn unknown_placeholder() -> Self {
        Self {
            database: UNKNOWN_SENTINEL.to_owned(),
            catalog: UNKNOWN_SENTINEL.to_owned(),
            schema: UNKNOWN_SENTINEL.to_owned(),
            path: UNKNOWN_SENTINEL.to_owned(),
        }
    }

    pub fn get(&self, layer: NamespaceLayer) -> &str {
        match layer {
            NamespaceLayer::Database => &self.database,
            NamespaceLayer::Catalog => &self.catalog,
            NamespaceLayer::Schema => &self.schema,
            NamespaceLayer::Path => &self.path,
        }
    }

    fn set(&mut self, layer: NamespaceLayer, value: String) {
        match layer {
            NamespaceLayer::Database => self.database = value,
            NamespaceLayer::Catalog => self.catalog = value,
            NamespaceLayer::Schema => self.schema = value,
            NamespaceLayer::Path => self.path = value,
        }
    }

    /// 按驱动形状校验：必填层缺失或为空 → `TargetRequired`。
    pub fn validate(&self, shape: &NamespaceShape) -> Result<(), ApiError> {
        for layer in NamespaceLayer::ALL {
            if shape.requires(layer) && self.get(layer).is_empty() {
                return Err(ApiError::new(
                    ApiErrorCode::TargetRequired,
                    format!("目标缺少必填层 {}", layer.as_str()),
                ));
            }
        }
        Ok(())
    }
}

/// 原始（未经规范化）的命名空间输入，用于表达「省略某层」与「显式传 null」两种不同的缺陷形态。
///
/// `None` = 字段未出现；`Some(None)` = 字段出现但为 `null`。两者都判 `TargetRequired`，
/// 但形态必须能被区分，否则 CM-07 的两个步骤会塌缩成一个。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceInput {
    pub database: Option<Option<String>>,
    pub catalog: Option<Option<String>>,
    pub schema: Option<Option<String>>,
    pub path: Option<Option<String>>,
}

/// 缺陷层在原始输入中的形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingLayerForm {
    /// 字段未出现。
    Absent,
    /// 字段出现，值为 `null`。
    Null,
}

impl NamespaceInput {
    pub fn layer(&self, layer: NamespaceLayer) -> Option<Option<&str>> {
        let raw = match layer {
            NamespaceLayer::Database => &self.database,
            NamespaceLayer::Catalog => &self.catalog,
            NamespaceLayer::Schema => &self.schema,
            NamespaceLayer::Path => &self.path,
        };
        raw.as_ref().map(|inner| inner.as_deref())
    }

    /// 按驱动形状构造目标。缺失层 / 空串 → `TargetRequired`。
    pub fn resolve(&self, shape: &NamespaceShape) -> Result<NamespaceTarget, ApiError> {
        let mut target = NamespaceTarget {
            database: String::new(),
            catalog: String::new(),
            schema: String::new(),
            path: String::new(),
        };
        for layer in NamespaceLayer::ALL {
            let raw = self.layer(layer);
            let missing_form = match raw {
                None => Some(MissingLayerForm::Absent),
                Some(None) => Some(MissingLayerForm::Null),
                Some(Some(_)) => None,
            };
            if let Some(form) = missing_form {
                if shape.requires(layer) {
                    return Err(ApiError::new(
                        ApiErrorCode::TargetRequired,
                        format!(
                            "目标缺少必填层 {}（{}）",
                            layer.as_str(),
                            match form {
                                MissingLayerForm::Absent => "字段未出现",
                                MissingLayerForm::Null => "显式 null",
                            }
                        ),
                    ));
                }
                continue;
            }
            let value = match raw {
                Some(Some(value)) => (*value).to_owned(),
                _ => String::new(),
            };
            if value.is_empty() && shape.requires(layer) {
                return Err(ApiError::new(
                    ApiErrorCode::TargetRequired,
                    format!("目标缺少必填层 {}（空串）", layer.as_str()),
                ));
            }
            target.set(layer, value);
        }
        Ok(target)
    }
}

/// 对象目标。对象操作必须传完整身份，宿主**不得**猜测 `public` / `dbo`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectTarget {
    pub kind: String,
    pub name: String,
    pub signature: String,
}

/// 执行目标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionTarget {
    pub connection_id: ConnectionId,
    pub namespace: NamespaceTarget,
    #[serde(default)]
    pub object: Option<ObjectTarget>,
}

/// 归属主体。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OwnerRef {
    Editor {
        organization_id: OrganizationId,
        principal_id: PrincipalId,
        connection_id: ConnectionId,
        client_instance_id: ClientInstanceId,
        editor_session_id: EditorSessionId,
    },
    Job {
        organization_id: OrganizationId,
        job_id: JobId,
        stage_id: String,
    },
    WorkflowBlock {
        run_id: RunId,
        block_id: BlockId,
    },
    ClientSession {
        client_instance_id: ClientInstanceId,
    },
}

impl OwnerRef {
    /// 稳定指纹输入。`runtimeEpoch` 形如 `<ownerHash>.<counter>`，换 owner 必增（§8.2）。
    pub fn hash(&self) -> String {
        let mut buf = String::new();
        match self {
            OwnerRef::Editor { organization_id, principal_id, connection_id, client_instance_id, editor_session_id } => {
                buf.push_str("editor|");
                buf.push_str(organization_id.as_str());
                buf.push('|');
                buf.push_str(principal_id.as_str());
                buf.push('|');
                buf.push_str(connection_id.as_str());
                buf.push('|');
                buf.push_str(client_instance_id.as_str());
                buf.push('|');
                buf.push_str(editor_session_id.as_str());
            }
            OwnerRef::Job { organization_id, job_id, stage_id } => {
                buf.push_str("job|");
                buf.push_str(organization_id.as_str());
                buf.push('|');
                buf.push_str(job_id.as_str());
                buf.push('|');
                buf.push_str(stage_id);
            }
            OwnerRef::WorkflowBlock { run_id, block_id } => {
                buf.push_str("workflowBlock|");
                buf.push_str(run_id.as_str());
                buf.push('|');
                buf.push_str(block_id.as_str());
            }
            OwnerRef::ClientSession { client_instance_id } => {
                buf.push_str("clientSession|");
                buf.push_str(client_instance_id.as_str());
            }
        }
        fnv1a64_hex(buf.as_bytes())
    }
}

/// 池键输入。`execution_identity_key` 由**后端生成且不可伪造**，不接受客户端提供的用户名（§4.3 L268）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolKeyInputs {
    pub connection_id: ConnectionId,
    pub config_revision: ConfigRevision,
    pub driver_id: String,
    pub namespace: NamespaceTarget,
    pub execution_identity_key: String,
    pub policy_isolation_key: String,
}

/// 池键指纹。共享 DB 账号但权限不同的用户必须落在不同指纹上（connection-management.md §9.6）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PoolKeyFingerprint(String);

impl PoolKeyFingerprint {
    pub fn derive(inputs: &PoolKeyInputs) -> Self {
        let mut buf = String::new();
        buf.push_str("poolkey|");
        buf.push_str(inputs.connection_id.as_str());
        buf.push('|');
        buf.push_str(&inputs.config_revision.to_string());
        buf.push('|');
        buf.push_str(&inputs.driver_id);
        buf.push('|');
        for layer in NamespaceLayer::ALL {
            buf.push_str(layer.as_str());
            buf.push('=');
            buf.push_str(inputs.namespace.get(layer));
            buf.push(';');
        }
        buf.push_str("|exec=");
        buf.push_str(&inputs.execution_identity_key);
        buf.push_str("|policy=");
        buf.push_str(&inputs.policy_isolation_key);
        Self(fnv1a64_hex(buf.as_bytes()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PoolKeyFingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_survive_json_roundtrip_beyond_2_pow_53() {
        // CM-01：Counter 使用大于 2^53 的十进制值，Rust/TS 往返不得丢精度。
        let big = Counter(9_007_199_254_740_993);
        let json = serde_json::to_string(&big).expect("serialize");
        assert_eq!(json, "\"9007199254740993\"", "Counter 必须序列化成十进制字符串");
        let back: Counter = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.get(), 9_007_199_254_740_993);
    }

    #[test]
    fn config_revision_is_also_a_decimal_string() {
        let json = serde_json::to_string(&ConfigRevision::new(7)).expect("serialize");
        assert_eq!(json, "\"7\"");
        let back: ConfigRevision = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.get(), 7);
    }

    #[test]
    fn absent_and_null_required_layers_are_distinct_but_both_rejected() {
        // CM-07 步骤：省略必需层 / 传 schema=null。
        let shape = NamespaceShape::database_and_schema();
        let absent: NamespaceInput =
            serde_json::from_str(r#"{"database":"dz_ns_a"}"#).expect("parse");
        let nulled: NamespaceInput =
            serde_json::from_str(r#"{"database":"dz_ns_a","schema":null}"#).expect("parse");
        assert_eq!(absent.layer(NamespaceLayer::Schema), None);
        assert_eq!(absent.layer(NamespaceLayer::Schema).is_none(), true);
        assert_eq!(nulled.layer(NamespaceLayer::Schema), Some(None));

        let from_absent = absent.resolve(&shape).expect_err("缺 schema 必须被拒");
        let from_null = nulled.resolve(&shape).expect_err("null schema 必须被拒");
        assert_eq!(from_absent.code, ApiErrorCode::TargetRequired);
        assert_eq!(from_null.code, ApiErrorCode::TargetRequired);
        // 两种形态的错误信息必须可区分，否则无法定位是省略还是显式 null。
        assert_ne!(from_absent.message, from_null.message);
    }

    #[test]
    fn two_different_databases_are_a_target_conflict() {
        // CM-07 步骤：传两个不同 database。
        let shape = NamespaceShape::database_and_schema();
        let input: NamespaceInput = serde_json::from_str(
            r#"{"database":"dz_ns_a","schema":"public","catalog":"dz_ns_b"}"#,
        )
        .expect("parse");
        let err = input.resolve(&shape).expect_err("两个不同 database 必须冲突");
        assert_eq!(err.code, ApiErrorCode::TargetConflict);
    }

    #[test]
    fn empty_required_layer_is_rejected() {
        // CM-01 断言：空值 / 缺字段请求返回参数错误。
        let shape = NamespaceShape::database_and_schema();
        let input: NamespaceInput =
            serde_json::from_str(r#"{"database":"","schema":"public"}"#).expect("parse");
        let err = input.resolve(&shape).expect_err("空字符串层必须被拒");
        assert_eq!(err.code, ApiErrorCode::InvalidArgument);
    }

    #[test]
    fn pool_key_changes_with_policy_isolation_and_config_revision() {
        let base = PoolKeyInputs {
            connection_id: ConnectionId::new("conn-fixture-p"),
            config_revision: ConfigRevision::new(7),
            driver_id: "postgres".into(),
            namespace: NamespaceTarget {
                database: "dz_ns_a".into(),
                catalog: String::new(),
                schema: "public".into(),
                path: String::new(),
            },
            execution_identity_key: "exec-identity-shared".into(),
            policy_isolation_key: "policy-u1".into(),
        };
        let revision_bumped = PoolKeyInputs { config_revision: ConfigRevision::new(8), ..base.clone() };
        let other_policy = PoolKeyInputs { policy_isolation_key: "policy-u2".into(), ..base.clone() };
        let other_identity = PoolKeyInputs {
            execution_identity_key: "exec-identity-user-alpha-2".into(),
            ..base.clone()
        };

        let key = PoolKeyFingerprint::derive(&base);
        assert_ne!(key, PoolKeyFingerprint::derive(&revision_bumped), "配置变更必须换 key");
        assert_ne!(key, PoolKeyFingerprint::derive(&other_policy), "policyIsolationKey 必须换 key");
        assert_ne!(
            key,
            PoolKeyFingerprint::derive(&other_identity),
            "执行身份必须换 key"
        );
        assert_eq!(key, PoolKeyFingerprint::derive(&base), "同输入必须完全确定");
    }

    #[test]
    fn owner_hash_changes_when_any_owner_field_changes() {
        // §8.2：换 owner 必增 runtimeEpoch —— 先决条件是 ownerHash 真的变了。
        let a = OwnerRef::Editor {
            organization_id: OrganizationId::new("org-alpha"),
            principal_id: PrincipalId::new("user-alpha-1"),
            connection_id: ConnectionId::new("conn-fixture-p"),
            client_instance_id: ClientInstanceId::new("cli-1"),
            editor_session_id: EditorSessionId::new("ed-1"),
        };
        let b = OwnerRef::Editor {
            organization_id: OrganizationId::new("org-alpha"),
            principal_id: PrincipalId::new("user-alpha-2"),
            connection_id: ConnectionId::new("conn-fixture-p"),
            client_instance_id: ClientInstanceId::new("cli-1"),
            editor_session_id: EditorSessionId::new("ed-1"),
        };
        assert_ne!(a.hash(), b.hash(), "换 principal 必须换 ownerHash");
        assert_eq!(a.hash(), a.clone().hash(), "同 owner 必须稳定");
    }

    #[test]
    fn connection_id_and_db_session_id_cannot_be_interchanged() {
        // 编译期保证：不同 newtype 之间没有 From。
        let conn = ConnectionId::new("conn-fixture-p");
        let session = DbSessionId::new("dbs_w1_0001");
        assert_ne!(conn.as_str(), session.as_str());
    }
}
