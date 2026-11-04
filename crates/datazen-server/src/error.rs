//! 进程级失败类型（team-server §2.1 的模块纪律：`thiserror` 分层）。
//!
//! 它是**进程边界**的错误：配置被拒、端口绑定失败、运行时崩塌——这些
//! 事实只进日志与 stderr，不进 HTTP 响应。跨边界的请求拒绝一律走
//! `datazen::shared::error::ApiError`（wire 词汇，见 `http::response`），
//! 两者不共用类型（§9：`CommandError` 与 `ApiError` 分立的同款裁决）。

/// `serve` 进程的失败。fail-closed：任一变体都终止启动。
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// `ServerConfig` 校验失败。消息脱敏（不含环境值原文）。
    #[error("server configuration rejected: {0}")]
    ConfigInvalid(String),

    /// 日志订阅器初始化失败（非法 filter 表达式等）。
    #[error("logging initialization failed: {0}")]
    LoggingInit(String),

    /// 监听地址绑定失败。
    #[error("listen on {addr} failed: {source}")]
    Listen {
        addr: std::net::SocketAddr,
        source: std::io::Error,
    },

    /// 服务循环失败（连接器层错误）。
    #[error("http serve loop failed: {0}")]
    Serve(String),
}
