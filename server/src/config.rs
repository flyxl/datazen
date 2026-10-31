//! `ServerConfig`：环境变量加载 + fail-closed 校验（team-server §2.3 的 C 步）。
//!
//! D1 骨架只认两个键；OIDC / 管理库 / UI origin / TLS 等部署面随对应切片
//! （WR-D2 管理库、WR-D3 认证与同源 HTTPS）进入同一结构体，**不另开配置
//! 来源**。校验失败即拒绝启动，错误消息不回显环境值原文（只回显键名与
//! 期望形状），避免把部署秘密打进日志。

use std::net::SocketAddr;
use std::path::PathBuf;

use crate::error::ServerError;

/// `serve` 模式的进程配置。
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// 监听地址。D1 默认 `127.0.0.1:8080`：生产监听是 `:8443` + TLS
    /// （team-server §2.3 的 N 步），随 WR-D3 的同源 HTTPS 一起切换；
    /// 骨架期只允许显式指定地址，不允许伪装成生产形态。
    pub listen_addr: SocketAddr,

    /// `tracing` filter 表达式（`EnvFilter` 语法），默认 `info`。
    pub log_filter: String,

    /// 管理库文件路径（WR-D2）。默认工作目录下的 `datazen-admin.sqlite`；
    /// 部署目录约定随 WR-G3 的运维切片固化，不在骨架期伪装。
    pub db_path: PathBuf,
}

/// 环境变量名。`DATAZEN_` 前缀与仓库既有约定对齐。
pub const ENV_LISTEN: &str = "DATAZEN_SERVER_LISTEN";
pub const ENV_LOG: &str = "DATAZEN_SERVER_LOG";
pub const ENV_DB: &str = "DATAZEN_SERVER_DB";
pub const DEFAULT_DB_FILE: &str = "datazen-admin.sqlite";

impl ServerConfig {
    /// 从环境变量加载并校验。任一键非法即 [`ServerError::ConfigInvalid`]。
    pub fn from_env() -> Result<Self, ServerError> {
        let listen_addr = read_addr(ENV_LISTEN)?;
        let log_filter = read_filter(ENV_LOG)?;
        let db_path = read_db(ENV_DB)?;
        Ok(Self {
            listen_addr,
            log_filter,
            db_path,
        })
    }
}

fn read_db(key: &str) -> Result<PathBuf, ServerError> {
    match std::env::var(key) {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                Err(ServerError::ConfigInvalid(format!(
                    "{key} must not be blank"
                )))
            } else {
                Ok(PathBuf::from(trimmed))
            }
        }
        Err(std::env::VarError::NotPresent) => Ok(PathBuf::from(DEFAULT_DB_FILE)),
        Err(std::env::VarError::NotUnicode(_)) => Err(ServerError::ConfigInvalid(format!(
            "{key} must be valid UTF-8"
        ))),
    }
}

fn read_addr(key: &str) -> Result<SocketAddr, ServerError> {
    match std::env::var(key) {
        Ok(raw) => raw
            .parse::<SocketAddr>()
            .map_err(|_| ServerError::ConfigInvalid(format!("{key} must be host:port (e.g. 127.0.0.1:8080)"))),
        Err(std::env::VarError::NotPresent) => Ok(SocketAddr::from(([127, 0, 0, 1], 8080))),
        Err(std::env::VarError::NotUnicode(_)) => Err(ServerError::ConfigInvalid(format!(
            "{key} must be valid UTF-8"
        ))),
    }
}

fn read_filter(key: &str) -> Result<String, ServerError> {
    match std::env::var(key) {
        Ok(raw) => {
            if raw.trim().is_empty() {
                Err(ServerError::ConfigInvalid(format!("{key} must not be blank")))
            } else {
                Ok(raw)
            }
        }
        Err(std::env::VarError::NotPresent) => Ok("info".to_string()),
        Err(std::env::VarError::NotUnicode(_)) => Err(ServerError::ConfigInvalid(format!(
            "{key} must be valid UTF-8"
        ))),
    }
}
