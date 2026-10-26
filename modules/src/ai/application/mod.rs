//! AI 应用层：Provider-neutral 业务（D6 item 4，切片三 + 收尾）。
//!
//! 宿主注入数据目录与端口，模块给出同一份解析/判定答案：
//! - 提示词解析与响应文本处理（prompt_resolver / response）；
//! - 会话编排（chat_request / sql_generation / single_turn / filter /
//!   schema_doc / schema_pipeline / context——schema 上下文管道）；
//! - 工具循环（tools / tool_loop / db_tools + DatabaseToolHost 端口）；
//! - 取消注册表（cancel）与会话所有权（ownership，P6 §14.4 授权）；
//! - schema 上下文实现（schema_context + SessionConnectionLookup 端口，
//!   经 workflow 的 ResourceExecutionHost 走共享执行网关）。
//!
//! 认证/stdio/HTTP 登录/事件传输在宿主；授权数据库工具走同一执行入口。

pub mod cancel;
pub mod chat_request;
pub mod context;
pub mod db_tools;
pub mod error;
pub mod filter;
pub mod ownership;
pub mod ports;
pub mod prompt_resolver;
pub mod response;
pub mod schema_context;
pub mod schema_doc;
pub mod schema_pipeline;
pub mod single_turn;
pub mod sql_generation;
pub mod tool_loop;
pub mod tools;

pub use error::AiUseCaseError;
pub use prompt_resolver::*;
pub use response::*;
