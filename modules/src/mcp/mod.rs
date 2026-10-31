//! MCP：权限模式、连接白名单与表描述的 Provider-neutral 业务规则
//! （D6 item 4 切片一，2026-10-26；M1 前置刀 2026-10-31 调整分层）。
//!
//! 词源：`datazen::mcp` 三文件逐字迁移。分层：
//! * `domain`——连接白名单（allowlist：空表即全拒绝的安全默认）；权限
//!   模式词汇已归位 `execution::domain::permission`（它是 workflow / ai /
//!   table_browsing / MCP 共同消费的执行分级词汇，留在 mcp 会形成 feature
//!   环），此处保持旧路径再导出垫片；
//! * `application`——表描述格式化（describe：MCP `describe_table` 工具
//!   输出的人类可读文案）、无状态工具用例（tools，M1）与有状态会话映射
//!   语义（session，M2：registry/TTL/归属校验/清理登记；定时 worker 与
//!   传输驱动留宿主）；command_policy 垫片（已归位 execution）。
//!
//! 边界（07-stages D6 工作项 4）：业务分发（server/handler/tools/resources）
//! 与认证/stdio/HTTP 登录/事件传输留宿主（src-tauri::mcp）；模块只持有
//! 规则与词汇。SQL 分级复用 `execution::domain::sql_guard` 的机制内核，
//! 不另立一份判据。

pub mod application;
pub mod domain;

// 旧扁平路径镜像（modules 垫片与直接消费者兼容）。
pub use application::describe;
pub use application::session;
pub use application::tools;
pub use domain::allowlist;
pub use domain::permission;
