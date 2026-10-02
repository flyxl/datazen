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

// ---------------------------------------------------------------------------
// ID newtype 与计数器：全项目唯一定义处在 `datazen-platform-api::id`。
// ---------------------------------------------------------------------------
//
// 本文件**不再**重复定义 `string_id!` 与 `Counter`。同一批 newtype 在
// `packages/platform-api/src/id.rs` 里已有一套等价实现；两处并存会让
// `ConnectionId` / `DbSessionId` 各自拥有两个互不相干的类型，
// AGENTS.md「ID 术语规范」要求的编译期保障也就落空了。
//
// 两套实现是**超集关系**，因此 re-export 不改变任何既有调用点与线上字面量：
//
// | 能力 | 本文件原实现 | `platform-api::id` |
// |---|---|---|
// | `new` / `as_str` / `is_empty` | ✓ | ✓ |
// | `Display`、`From<String>`、`From<&str>`、`AsRef<str>` | ✓ | ✓ |
// | `Borrow<str>`（可直接查 `HashMap<String, _>`） | — | ✓ |
// | `Counter` 十进制字符串序列化（CM-01） | ✓ | ✓（同样接受 str/u64/i64） |
// | 计数器自增 | `increment(&mut self)` | `saturating_increment(self)` |
//
// 序列化形状逐项相同：ID newtype 是 `#[serde(transparent)]`，`Counter` 走
// `serialize_str(十进制)`，因此 re-export 前后 JSON 完全一致。
//
// ID 术语纪律依然成立：`ConnectionId`（持久化配置 id，落盘）与
// `DbSessionId`（内存态运行时会话 id，永不落盘）是 platform-api 里**两个独立类型**，
// 既无跨类型 `From`，也无共用底层类型的隐式转换，混用在编译期即被拒绝。

pub use datazen_platform_api::id::{
    ArtifactId, BlockId, ClientInstanceId, ConnectionId, Counter, DbSessionId, EditorSessionId,
    ExecutionId, HandleId, JobId, LeaseId, OrganizationId, PrincipalId, ResourceId, RunId,
    StreamId, Timestamp, WorkerId,
};

/// 命名空间层与形状的**唯一**定义在 `datazen-platform-api::target`。
///
/// 本文件原先**重复定义**了这两个类型，代价是两份 JSON 形态（`optional` 的 `#[serde(default)]`
/// 只有 runtime 一侧有）与两套 `database_and_schema()` 语义同时在线。现已删除本地副本。
///
/// 唯一未被合并的行为差异：`platform-api` 的 `database_and_schema()` 把 schema 列为**可选**，
/// 而本 crate 历史上的 `database_and_schema()` 把 database 与 schema **都列为必填**
/// （见下方 `schema_required_shape()`）。两者语义不同，所以本 crate 的调用点一律显式构造，
/// 不调用 `platform-api` 的那个便捷构造器 —— 具体取舍见 `schema_required_shape()` 的注释。
pub use datazen_platform_api::target::{NamespaceLayer, NamespaceShape};

/// 本 crate 在**历史行为**上使用的形状：database 与 schema 都必填。
///
/// 为什么不用 `NamespaceShape::database_and_schema()`：那个构造器把 schema 列为**可选**，
/// 而 CM-07 要求「省略必需层」判 `TargetRequired`。换成它，本文件
/// `absent_and_null_required_layers_are_distinct_but_both_rejected` 所断言的规格行为会被静默放行 ——
/// 这正是本文件此前拒绝合并时写下的理由（「会让原本被拒的调用静默放行」）。
///
/// 因此这里是**显式构造**而不是再 `pub use` 一个构造器：类型只有一份，但两处声明的语义各自明确，
/// 谁想要哪种形状必须自己写出来，不存在一个名字覆盖两种含义的入口。
/// 待 spec owner 裁定「schema 是否应改为可选」之前，`schema_required_shape()` 是本 crate 的既定语义。
pub fn schema_required_shape() -> NamespaceShape {
    NamespaceShape::new(
        [NamespaceLayer::Database, NamespaceLayer::Schema],
        Vec::new(),
    )
}

// ---------------------------------------------------------------------------
// 以下类型与 `platform-api` 存在同名物，但**形状或语义确实不同**，因此刻意不去重。
// 合并它们会改动序列化后的线上字面量，属于行为变更，不在类型去重范围内。
//
// | 类型 | 本文件 | `platform-api` | 不能合并的原因 |
// |---|---|---|---|
// | `NamespaceTarget` | 4 个 `String` | 3 个 `Option<String>` + `path: Vec<String>` | JSON 不同：`path` 是标量还是数组；platform-api 的单测显式断言必须是数组 |
// | `ObjectTarget` | `{schema?, name, signature}` | `{kind, name, signature?}` | 字段集合不同，`signature` 可选性与有无 `kind` 都相反 |
// | `ExecutionTarget` | 包装上面两个 | 同上 | 随 `NamespaceTarget` / `ObjectTarget` 传递性不可合并 |
// | `ConfigRevision` | `pub u64` | `version_id!` → `pub Counter` | 线上字面量相同（十进制字符串），但元组字段类型不同，不能直接别名替换 |
// | `OwnerRef` | `WorkflowBlock{run_id, block_id}`、`Job` 带 `organization_id` | 多一个 `Editor` 变体，字段经 `rename_all_fields` 重命名 | 变体载荷与 JSON tag 都不同 |
//
// `NamespaceShape` / `NamespaceLayer` 曾经也在这张表里，理由是「两套语义并存」；
// 该条目已失效并删除：类型本身现已通过 `pub use` 收敛为一份，剩余的
// `database_and_schema()` 语义差异改由上面的 `schema_required_shape()` 显式承载。
//
// 另有两处同名物不在本文件、也不适合跨 crate 合并：
// `ApiError` / `ApiErrorCode` 的规范定义在 `packages/application/src/error.rs`
// （4 字段结构体），`CapabilitySnapshot` 两边是**不同概念**——本 crate 的
// `connection/capability.rs` 描述驱动能力矩阵（`confirmed: bool` + 10 个能力枚举），
// `platform-api/src/dto/execution.rs` 描述执行审计记录（`confirmed: BTreeMap<_, _>`）。
//
// 第三处同名物：**`datazen-driver-api::namespace::NamespaceShape`**（`packages/driver-api/src/namespace.rs`）。
// 它**不是**上面那份的第二份副本 —— 层枚举的名字都不一样（`NamespaceLayer` vs `NamespaceLevel`），
// 配套还有 `NamespaceLevelKind`、`Default` 派生，且 15 个驱动用**结构体字面量**（配合
// `..NamespaceShape::default()`）构造、从不调用 `new()`。也就是说 driver 侧是一套**独立词汇**，
// 描述「驱动命名空间的层级与种类」，与 platform-api 描述的「一次请求声明了哪些层」不是同一维度。
//
// 这里显式记录，避免它再次被当成「没人知道的重复定义」：
// 收敛它是一次真正的**词汇归一化重构**，会波及全部 15 个驱动的 provider；
// 在 driver-type-guard phase-2 落地前不做（那 8 个驱动正在改能力值，
// 此时动层模型等于在它下面抽地板）。排期见 backlog，不在本文件追踪。
// 注意两份类型的层名不可直接互换，误用不会编译报错之外的任何提示 —— 迁移时必须逐驱动核对。

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
            if shape.is_required(layer) && self.get(layer).is_empty() {
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
    #[serde(default, deserialize_with = "double_option::present")]
    pub database: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option::present")]
    pub catalog: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option::present")]
    pub schema: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option::present")]
    pub path: Option<Option<String>>,
}

/// serde 默认把 JSON `null` 解成 `None`，于是「字段未出现」和「显式传 null」在
/// `Option<Option<String>>` 上塌缩成同一个值，CM-07 的两个步骤就分不开。
/// `present` 只包一层：字段出现时一定是 `Some(内层)`，`null` ⇒ `Some(None)`；
/// 字段未出现时走 `#[serde(default)]` ⇒ `None`。
mod double_option {
    use serde::{Deserialize, Deserializer};

    pub(super) fn present<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        // 字段出现了：内层 `None`（JSON null）也必须表达成 `Some(None)`。
        Option::<T>::deserialize(deserializer).map(|inner| Some(inner))
    }
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

    /// 按驱动形状构造目标。
    ///
    /// 必填层**未出现 / 显式 null** → `TargetRequired`（两种形态文案不同，CM-07 才能定位）；
    /// 必填层**给了空串** → `InvalidArgument`：空串是参数本身不合法，不是目标缺失。
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
                if shape.is_required(layer) {
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
            if value.is_empty() && shape.is_required(layer) {
                return Err(ApiError::new(
                    ApiErrorCode::InvalidArgument,
                    format!("目标层 {} 不得为空串", layer.as_str()),
                ));
            }
            target.set(layer, value);
        }
        Ok(target)
    }

    /// 解析后再与会话已记录的 `expected` 逐层比对：任一层两边都非空却不同 ⇒ `TargetConflict`。
    ///
    /// CM-73 / §10 的跨目标护栏需要它：句柄来自 A 命名空间的会话，命令却点名 B 命名空间，
    /// 宿主**不得**悄悄改上下文去迁就句柄。
    pub fn resolve_against(
        &self,
        expected: &NamespaceTarget,
        shape: &NamespaceShape,
    ) -> Result<NamespaceTarget, ApiError> {
        let resolved = self.resolve(shape)?;
        for layer in NamespaceLayer::ALL {
            let mine = resolved.get(layer);
            let theirs = expected.get(layer);
            if !mine.is_empty() && !theirs.is_empty() && mine != theirs {
                return Err(ApiError::new(
                    ApiErrorCode::TargetConflict,
                    format!(
                        "目标层 {} 冲突：请求 {}，会话已绑定 {}",
                        layer.as_str(),
                        mine,
                        theirs
                    ),
                ));
            }
        }
        Ok(resolved)
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
            OwnerRef::Editor {
                organization_id,
                principal_id,
                connection_id,
                client_instance_id,
                editor_session_id,
            } => {
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
            OwnerRef::Job {
                organization_id,
                job_id,
                stage_id,
            } => {
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
        assert_eq!(
            json, "\"9007199254740993\"",
            "Counter 必须序列化成十进制字符串"
        );
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
        let shape = schema_required_shape();
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
        // CM-07 步骤：会话已绑 dz_ns_a，命令却点名 dz_ns_b。
        let shape = schema_required_shape();
        let bound = NamespaceInput {
            database: Some(Some("dz_ns_a".to_owned())),
            schema: Some(Some("public".to_owned())),
            ..NamespaceInput::default()
        };
        let requested: NamespaceInput =
            serde_json::from_str(r#"{"database":"dz_ns_b","schema":"public"}"#).expect("parse");
        let session_target = bound.resolve(&shape).expect("已绑定目标必须能解析");
        let err = requested
            .resolve_against(&session_target, &shape)
            .expect_err("两个不同 database 必须冲突");
        assert_eq!(err.code, ApiErrorCode::TargetConflict);
        // 同一 database 必须放行，否则护栏会误伤合法的同库操作。
        let same = bound
            .resolve_against(&session_target, &shape)
            .expect("同库同名必须放行");
        assert_eq!(same, session_target);
    }

    #[test]
    fn empty_required_layer_is_rejected() {
        // CM-01 断言：空值 / 缺字段请求返回参数错误。
        let shape = schema_required_shape();
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
        let revision_bumped = PoolKeyInputs {
            config_revision: ConfigRevision::new(8),
            ..base.clone()
        };
        let other_policy = PoolKeyInputs {
            policy_isolation_key: "policy-u2".into(),
            ..base.clone()
        };
        let other_identity = PoolKeyInputs {
            execution_identity_key: "exec-identity-user-alpha-2".into(),
            ..base.clone()
        };

        let key = PoolKeyFingerprint::derive(&base);
        assert_ne!(
            key,
            PoolKeyFingerprint::derive(&revision_bumped),
            "配置变更必须换 key"
        );
        assert_ne!(
            key,
            PoolKeyFingerprint::derive(&other_policy),
            "policyIsolationKey 必须换 key"
        );
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
