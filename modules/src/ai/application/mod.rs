//! AI 应用层：提示词解析与响应文本处理（D6 item 4 切片三）。
//!
//! Provider-neutral 规则与编排的落点：宿主注入数据目录与端口，模块给出
//! 同一份解析/判定答案。会话编排（tool_loop/cancel/ownership）与命令族
//! （single_turn/filter 等）随后续切片归位。

pub mod prompt_resolver;
pub mod response;

pub use prompt_resolver::*;
pub use response::*;
