//!
//! `connection-management.md:897-901` 判据：*「或在 body 伪造 organizationId/
//! principalId」⇒ 拒绝 owner；请求身份字段无法覆盖 RequestContext；不创建会话*。
//!
//! 判据要求的是**编译期 / 反序列化期的负例**，不是一句注释。本模块给的是三层：
//!
//! 1. `compile_fail` doctest —— 伪造 `owner` / `organization_id` 的**结构体字面量**
//!    过不了 E0560；伪造主体去构造 `RequestPrincipal` 过不了 E0616。
//! 2. 每条 `compile_fail` 旁边配一条**能编译**的对照，防的是「负例变空断言」：
//!    若哪天 `ExecutionRequest` 真加上了 `owner` 字段，只有 `compile_fail` 会安静地
//!    从「按预期失败」变成「因别的原因失败」，而对照组仍会编译——两者不再成对，
//!    门禁就失去意义。
//! 3. `ExecutionRequest` 连 `Deserialize` 都没实现 ⇒ body 反序列化在**类型层**就是
//!    一条不存在的路径，`serde_json::from_str::<ExecutionRequest>` 报 E0277。
//!
//! 这组断言里**没有一条**回显伪造值：断言文本只说「编译失败」，不回显谁的身份。
//!
//! 每条负例都**写死了错误码**（`compile_fail,E0560` 而不是裸 `compile_fail`）。裸
//! `compile_fail` 只要求「编不过」，把上面某个字面量写错一个逗号也能蒙混过关。
//!
//! 但**别指望它兜底**：本仓库工具链（rustdoc 1.90.0）实测**不校验**错误码后缀——把
//! `compile_fail,E0560` 改成 `compile_fail,E0308` 后门禁照样全绿。这不是小事：本轨正是在
//! 探测阶段才发现标着 `E0616` 的负例实际吐的是 **E0308**（字段当时用了 `String`，类型不
//! 匹配先炸），也就是说「通过了」的负例根本没在测它自己那道门。真正吃劲的是**成对的可
//! 编译对照**：把字段改成 `pub`、或把类型改回 `String`，负例立刻转红。

/// 负例：把 `owner` 塞进 `ExecutionRequest` 的结构体字面量 → E0560。
///
/// ```compile_fail,E0560
/// use datazen_runtime::connection::{CommandCall, Counter, DbSessionId};
/// use datazen_runtime::gateway::{ExecutionRequest, ExecutionSource};
///
/// let request = ExecutionRequest {
///     handle: datazen_runtime::connection::SessionHandle {
///         db_session_id: DbSessionId::new("dbse_a"),
///         runtime_epoch: Counter(0),
///     },
///     expected_context_revision: Counter(0),
///     call: CommandCall { command: "select 1".to_owned(), input: Default::default() },
///     idempotency_key: "k".to_owned(),
///     source: ExecutionSource::new(
///         datazen_runtime::gateway::SourceKind::Editor,
///         "w",
///         None,
///         None,
///     ),
///     resource_binding_id: None,
///     // 前端伪造的 owner：结构体字面量里没有这个字段，编译期就被挡掉。
///     owner: "forged",
/// };
/// ```
///
/// 对照：把上面**除 `owner` 外**的原样写出来，必须能编译。
/// 若对照组也编不过，说明负例是在因无关原因失败（例如 `DbSessionId::new`
/// 签名变了），这条负例就是空断言。
///
/// ```
/// use datazen_runtime::connection::{CommandCall, Counter, DbSessionId};
/// use datazen_runtime::gateway::{ExecutionRequest, ExecutionSource};
///
/// let request = ExecutionRequest {
///     handle: datazen_runtime::connection::SessionHandle {
///         db_session_id: DbSessionId::new("dbse_a"),
///         runtime_epoch: Counter(0),
///     },
///     expected_context_revision: Counter(0),
///     call: CommandCall { command: "select 1".to_owned(), input: Default::default() },
///     idempotency_key: "k".to_owned(),
///     source: ExecutionSource::new(
///         datazen_runtime::gateway::SourceKind::Editor,
///         "w",
///         None,
///         None,
///     ),
///     resource_binding_id: None,
/// };
/// let _ = request;
/// ```
///
/// 负例：伪造 `organization_id` → E0560。
///
/// ```compile_fail,E0560
/// use datazen_runtime::connection::{CommandCall, Counter, DbSessionId};
/// use datazen_runtime::gateway::{ExecutionRequest, ExecutionSource};
///
/// let request = ExecutionRequest {
///     handle: datazen_runtime::connection::SessionHandle {
///         db_session_id: DbSessionId::new("dbse_a"),
///         runtime_epoch: Counter(0),
///     },
///     expected_context_revision: Counter(0),
///     call: CommandCall { command: "select 1".to_owned(), input: Default::default() },
///     idempotency_key: "k".to_owned(),
///     source: ExecutionSource::new(
///         datazen_runtime::gateway::SourceKind::Editor,
///         "w",
///         None,
///         None,
///     ),
///     resource_binding_id: None,
///     organization_id: "forged",
/// };
/// ```
///
/// 负例：把 body 里的主体读出来喂给 `RequestPrincipal` → E0616。
///
/// 这条负例踩过两个坑，都记在这里以免再犯：
///
/// 1. `Body` 若和读取方**写在同一个模块**里，Rust 的隐私是模块级而不是函数级，
///    私有字段照样读得到，负例会安静地变成「编译通过」。所以 `Body` 必须待在
///    另一个模块 `payload` 里，读取方在 crate 根。
/// 2. `Body` 的字段若用 `String`，`RequestPrincipal::new` 收的是 newtype，
///    E0308 会先炸出来把 E0616 盖掉——负例「通过了」，但过的不是它自己那道门。
///    所以字段类型必须与形参**逐一对上**，让字段私有成为唯一可能的错误。
///
/// ```compile_fail,E0616
/// use datazen_platform_api::id::{DbSessionId, OrganizationId, PrincipalId};
/// use datazen_runtime::gateway::RequestPrincipal;
///
/// mod payload {
///     use datazen_platform_api::id::{DbSessionId, OrganizationId, PrincipalId};
///
///     /// body 反序列化出来的形状：三种**不同的** newtype。
///     pub struct Body {
///         principal_id: PrincipalId,
///         organization_id: OrganizationId,
///         org_session_id: DbSessionId,
///     }
///
///     impl Body {
///         pub fn forged() -> Self {
///             Body {
///                 principal_id: PrincipalId::new("p"),
///                 organization_id: OrganizationId::new("o"),
///                 org_session_id: DbSessionId::new("s"),
///             }
///         }
///     }
/// }
///
/// fn main() {
///     let body = payload::Body::forged();
///     // 类型全对，唯一挡路的是「字段不是 pub」。
///     let _ctx = RequestPrincipal::new(
///         body.principal_id,
///         body.organization_id,
///         body.org_session_id,
///     );
/// }
/// ```
///
/// 对照（必须能编译）：**一模一样的双模块结构**，只把私有字段换成只读访问器。
/// 若对照组编不过，上面那条 `compile_fail` 就是因无关原因失败。
///
/// ```
/// use datazen_platform_api::id::{DbSessionId, OrganizationId, PrincipalId};
/// use datazen_runtime::gateway::RequestPrincipal;
///
/// mod payload {
///     use datazen_platform_api::id::{DbSessionId, OrganizationId, PrincipalId};
///
///     pub struct Body {
///         principal_id: PrincipalId,
///         organization_id: OrganizationId,
///         org_session_id: DbSessionId,
///     }
///
///     impl Body {
///         pub fn forged() -> Self {
///             Body {
///                 principal_id: PrincipalId::new("p"),
///                 organization_id: OrganizationId::new("o"),
///                 org_session_id: DbSessionId::new("s"),
///             }
///         }
///         pub fn principal_id(&self) -> PrincipalId { self.principal_id.clone() }
///         pub fn organization_id(&self) -> OrganizationId { self.organization_id.clone() }
///         pub fn org_session_id(&self) -> DbSessionId { self.org_session_id.clone() }
///     }
/// }
///
/// fn main() {
///     let body = payload::Body::forged();
///     let ctx = RequestPrincipal::new(
///         body.principal_id(),
///         body.organization_id(),
///         body.org_session_id(),
///     );
///     // 只有经由访问器才能取回身份，取不回也改不了。
///     let _ = ctx.principal_id();
/// }
/// ```
///
/// 负例：`ExecutionRequest` 上没有 `Deserialize` ⇒ body 反序列化不是一条路 → E0277。
///
/// ```compile_fail,E0277
/// use datazen_runtime::connection::{CommandCall, Counter};
/// use datazen_runtime::gateway::{ExecutionRequest, ExecutionSource};
///
/// let handle = datazen_runtime::connection::SessionHandle {
///     db_session_id: datazen_runtime::connection::DbSessionId::new("dbse_a"),
///     runtime_epoch: Counter(0),
/// };
/// let request = ExecutionRequest::new(
///     handle,
///     Counter(0),
///     CommandCall { command: "select 1".to_owned(), input: Default::default() },
///     "k",
///     ExecutionSource::new(
///         datazen_runtime::gateway::SourceKind::Editor,
///         "w",
///         None,
///         None,
///     ),
/// );
/// // 身份只能从「已经在服务端建立的会话」上读，不能从 body 上反序列化。
/// let from_body: ExecutionRequest = serde_json::from_str(r#"{"owner":"forged"}"#).unwrap();
/// let _ = from_body;
/// ```
///
/// 对照（必须能编译）：同一个 `serde_json::from_str` 打在**实现了 `Deserialize`**
/// 的类型上，证明上一条是因 `ExecutionRequest` 缺 trait 而失败，不是因 `serde_json`
/// 不在作用域、调用形态写错之类无关原因失败。
///
/// ```
/// # use datazen_platform_api::id::DbSessionId;
/// let from_body: DbSessionId = serde_json::from_str(r#""dbse_a""#).expect("可反序列化的对照类型");
/// let _ = from_body;
/// ```

/// 本模块里的负例条数（`compile_fail`）。
///
/// 刻意写成可读常量：改动上面的负例时，这个数字必须一起改，否则「我以为钉住了」
/// 就退化成了注释。
#[allow(dead_code)]
const NEGATIVE_CASES: usize = 4;
