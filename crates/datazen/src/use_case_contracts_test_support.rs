//! WR-C3「同用例双传输」测试基建（web-ready-plan §5 WR-C3 行）。
//!
//! # 验收口径
//!
//! 「同一用例（组装 request → 调 application → 断言）在 IPC 与 HTTP 两绑定下
//! 跑」；新用例测试**两腿同绿**；**形态差异只允许出现在装配层断言**。
//! 判据（commands-layer-review §1）：把宿主从 Tauri 换成 axum，测试还能原样
//! 过 → 它测的是形态中立的业务。因此每个规格在本 crate 里先跑一遍**无宿主
//! 基线**（直调 application 门面，`cfg(test)` 内联），再由两条宿主腿复用：
//!
//! - **HTTP 腿**：`crates/datazen-server/tests/use_case_contracts.rs`——测试内自建最小 axum
//!   Router（端点形态 = WR-C2 冻结契约），handler 调同一用例；
//! - **IPC 腿**：桌面 `commands/named_views_use_case_contract_tests.rs`——
//!   真实 `#[tauri::command]` 入口经 mock_app 托管的 `AppState` 驱动。
//!
//! # 规格形态（基建）
//!
//! 一个用例规格 = **setup（夹具仓储/假体）→ request 组装 → 调 application →
//! 断言**，参数化于「传输装配」（binding harness，各域的
//! `*ContractBinding` trait）。规格本体完全不感知 IPC/HTTP 词汇：
//!
//! - 入参与出参一律 application DTO（如 `NamedView`）；
//! - 失败观测是 [`ContractFailure`]——application 契约码 + application 文本，
//!   由各腿装配层从自己的传输形态（IPC 信封字符串 / HTTP JSON 体）抽回；
//! - 断言只落在 application 层结果词汇上，两腿必须逐字一致。
//!
//! 夹具（仓储假体、记录构造器）随规格住在 `test-harness` 门控下，两种宿主
//! 形态用**同一份 setup 词汇**（沿 `ai::providers::testing` 的 MockAiProvider
//! 先例；存储实现仍由各腿自选——桌面用真实 `DesktopRepository`，服务端用
//! 共享的内存假体）。
//!
//! # 本刀选型：named_views
//!
//! 1. 用例本体与服务门面已在 modules（`NamedViewService`），端口（仓储 +
//!    id minter）可注入，无 runtime/网络依赖；
//! 2. HTTP 路由形态已由 WR-C2 冻结（web-http-api §6.9），HTTP 腿不必
//!    发明端点；桌面侧三条命令是零事件、零窗口参数的最薄壳；
//! 3. 错误词汇同时覆盖两族——带契约码的规则拒绝（`grid.view.*`）与无码的
//!    存储失败——跨传输断言两族行为（含 D1：宿主消息不携带用户输入）。
//!
//! # 装配差异清单（唯一允许的差异位置）
//!
//! | 维度 | IPC 腿 | HTTP 腿 |
//! |---|---|---|
//! | 宿主装配 | `tauri::test::mock_app()` 托管 `AppState`，命令直调 | 测试内自建最小 Router（GET/PUT/DELETE `/api/v1/named-views`） |
//! | 错误 wire 形态 | 纯字符串信封，契约码在消息**首位前缀**（C1） | JSON 对象 `{code, message}`，状态码 409/400/500 |
//! | setup 落库 | 真实 `DesktopRepository::set_named_views` | 共享 `MemoryViewStore` |
//! | id 铸造 | 宿主 ULID（`HostViewIdMinter`） | 规格 `ContractIdMinter` |
//!
//! 规格对以上差异**零感知**；两腿各自的装配层断言（pin）钉住差异本身。
//!
//! # 后续域接入模板（每域随路由点亮补规格）
//!
//! 1. `use_case_contracts_test_support/` 建域子模块：binding trait（`seed` + 该域用例
//!    入口；application 词汇入参出参，失败出 [`ContractFailure`]）；
//! 2. 夹具：仓储假体 + 记录构造器（与域既有测试假体同源，能共享就
//!    re-export，不复制第二份）；
//! 3. scenarios：正常路径 + ≥1 错误路径，断言全部落 application 层词汇；
//! 4. 先跑本 crate 内联基线（`cfg(test)`，无宿主）——基线绿是判据本体；
//! 5. server 腿：`crates/datazen-server/tests/use_case_contracts.rs` 加**同名** scenario
//!    测试 + 装配 pin（状态码 / wire 体形状），路由沿 WR-C2 契约表；
//! 6. desktop 腿：`commands/` 域内加**同名** scenario 测试 + 装配 pin
//!    （信封形状），沿 `*_command_test_host` 模式走真实命令入口；
//! 7. 主线集成：server 腿需要 `--features datazen/<域>,datazen/test-harness`
//!    的 CLI feature 形式进 CI（不改 manifest 的前提下）。

/// 双传输下对一次用例失败的**形态中立**观测。
///
/// 两腿装配层各自把传输形态抽回本词汇（IPC：消息首位前缀即契约码；HTTP：
/// wire 体 `code` 字段），规格据此断言。`code` 是 application 契约码
/// （如 `grid.view.nameExists`）；无契约码的失败（如存储故障）为 `None`，
/// 但 `message` 仍须携带 application 文本——「无码」本身也是两腿一致的
/// 契约事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractFailure {
    /// application 契约码；无码失败为 `None`。
    pub code: Option<String>,
    /// application 错误文本（宿主消息：只说宿主知道的，不引用用户输入）。
    pub message: String,
}

impl ContractFailure {
    /// 从 application 错误的契约码与文本构造（无宿主基线腿用）。
    pub fn from_application(code: Option<String>, message: String) -> Self {
        Self { code, message }
    }
}

/// 规格 scenario 的统一结果词汇：成功给 application DTO，失败给
/// [`ContractFailure`]。
pub type ContractResult<T> = Result<T, ContractFailure>;

/// named_views 域的双传输规格（第一刀；选型理由见模块头）。
#[cfg(feature = "named_views")]
pub mod named_views;

/// settings 域的双传输规格（第二刀）：`GET/PUT /api/v1/settings`（§6.1）
/// 与桌面 `get_settings`/`save_settings` 走同一用例；本域无契约码，「无码」
/// 即两腿一致的契约事实（选型与错误词汇见模块内文档）。
#[cfg(feature = "settings")]
pub mod settings;

/// connections 域的双传输规格（第三刀，WR-C3 余域）：§6.3 的
/// `GET/POST /api/v1/connections`、`PATCH /api/v1/connections/{id}`（一条
/// 路由双服务）与桌面 kernel-wire 命令（`create_connection` /
/// `update_connection` / `disable_connection` / `list_connections`）走同一
/// Profile 端口；CAS 冲突带契约码 `configRevisionMismatch`（选型与错误
/// 词汇见模块内文档）。消费 connection + execution 两个 feature
/// （execution 蕴含 connection，CI 单 feature 形式即
/// `datazen/execution,datazen/test-harness`）。
#[cfg(all(feature = "connection", feature = "execution"))]
pub mod connections;

/// sessions 域的双传输规格（第三刀，WR-C3 余域）：§6.4 的会话生命周期
/// 端点（open/get/context/close；attach/detach 路由同形挂载、场景随后续
/// 登记）与桌面 kernel-wire 命令（`open_session` / `get_session` /
/// `set_session_context` / `close_session`）走同一
/// `RuntimeConnectionUseCases` 用例面；本域错误是 application 自己的
/// `ApiError`，两腿逐字一致（见模块内文档）。
#[cfg(feature = "execution")]
pub mod sessions;
