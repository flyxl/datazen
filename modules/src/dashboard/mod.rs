//! Dashboard：仪表盘/组件/运行历史的领域词汇、仓储端口与用例规则
//! （D6 item 2，2026-10-26）。
//!
//! 词源：`datazen::dashboard` 全家族逐字迁移。分层：
//! * `domain`——行词汇（records）、仪表盘/组件/图表/告警词汇（types）、
//!   仓储端口（repository，trait 归 domain、宿主实现）与 Monitor 授权
//!   元数据（schedule_grant）；
//! * `application`——组件创建（create）、运行历史规则（runs）、告警评估
//!   （alert）、导入导出与脱敏（archive）、执行编排（execute）、刷新
//!   监控引擎（monitor）、通知规则（notifications）。
//!
//! 边界（07-stages D6 工作项 2）：保存/组件创建/运行历史/导入导出规则进
//! 模块；管理库（AppDb SQLite）实现留宿主（src-tauri::dashboard）。
//! 文件对话框留宿主交互层——`archive` 只消费字节与产出字节，GUI 对话框
//! 不是模块依赖。

pub mod application;
pub mod domain;

// 旧扁平路径镜像（modules 垫片与直接消费者兼容）。
pub use application::alert;
pub use application::archive;
pub use application::create;
pub use application::execute;
pub use application::monitor;
pub use application::notifications;
pub use application::runs;
pub use domain::records;
pub use domain::repository;
pub use domain::schedule_grant;
pub use domain::types;
