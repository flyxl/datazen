//! Dashboard 域的 server 服务层（W8 刀3d-2）。
//!
//! 桌面对照物：`datazen-desktop/src/dashboard/`（create / execute / runs /
//! repository / export）与 `datazen-desktop/src/monitor/`（engine / grants /
//! scheduled）。规则单家在共享层 `datazen::dashboard` 与
//! `datazen::monitor`（执行编排、告警评估、运行转换、授权单词汇、目标
//! 收集、只读 SQL 校验、刷新调度引擎）；本包只提供宿主绑定：
//!
//! - [`execution`]——`WidgetExecutionHost` 的 server 实现（交互执行 +
//!   运行落账）与授权单作用域的 `ResourceExecutionHost` 包装（调度执行
//!   的准入闸门）；
//! - [`creation`]——`WidgetCreationHost` 的 server 实现（组件创建三用例
//!   的宿主半边）；
//! - [`grants`]——monitor 授权/撤销/读取的服务端流程（桌面
//!   `monitor::grants` 的组织键投影）；
//! - [`scheduler`]——共享刷新监控引擎的 server 接线（「双端调度」的
//!   dashboard 半边：arm-not-fire、授权闸门失败关闭、graceful drain）。
//!
//! **形态差异登记**（模块头纪律）：
//! - 桌面 `scheduled.rs` 的 job-occurrence 机器（幂等槽位/checkpoint/
//!   恢复核验）不在本刀的 server 面——server 调度派发与刀3c 的 workflow
//!   调度器同形（直接执行 + 运行落账），重启后同槽位重跑的幂等 receipts
//!   随 job 面立项（登记缺口，不假装在場）；
//! - 通知投递（webhook/email）server v1 不做——`deliver_run` 是结构化
//!   审计日志；事件面（SSE 桥）随 WR-G 切片；
//! - 导入/导出走桌面文件对话框（浏览器侧用产物上传/下载），不进 HTTP。

pub mod creation;
pub mod execution;
pub mod grants;
pub mod scheduler;
