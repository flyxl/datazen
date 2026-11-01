//! `ServerConfig`：环境变量加载 + fail-closed 校验（team-server §2.3 的 C 步）。
//!
//! D1 骨架只认两个键；OIDC / 管理库 / UI origin / TLS 等部署面随对应切片
//! （WR-D2 管理库、WR-D3 认证与同源 HTTPS）进入同一结构体，**不另开配置
//! 来源**。校验失败即拒绝启动，错误消息不回显环境值原文（只回显键名与
//! 期望形状），避免把部署秘密打进日志。
//!
//! WR-D3 第一刀新增三个键：`server_secret`（CSRF 与 policyIsolationKey 的
//! HMAC 密钥，§5.1/§6.2）与两个会话 TTL 覆盖项（§4.3 的「首版建议值，
//! 来自部署配置」：绝对 24h / 空闲 8h）。开发默认密钥**仅限本地验证**，
//! 用默认值时打 warn 日志——生产部署必须显式注入（接线索留给集成步，
//! `bootstrap::serve` 流程语义本刀不动）。

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

    /// CSRF 与 `policyIsolationKey` 的 HMAC 密钥（§5.1/§6.2；不落库、
    /// 不进日志与响应）。显式注入时要求 ≥ 32 字符；未配置时用开发默认
    /// 值并告警（仅限本地验证——生产必须显式注入）。
    pub server_secret: ServerSecret,

    /// 登录会话空闲寿命（秒；§4.3 默认 8 小时，部署可调）。
    pub session_idle_seconds: u64,

    /// 登录会话绝对寿命（秒；§4.3 默认 24 小时，部署可调）。
    pub session_absolute_seconds: u64,
}

/// 环境变量名。`DATAZEN_` 前缀与仓库既有约定对齐。
pub const ENV_LISTEN: &str = "DATAZEN_SERVER_LISTEN";
pub const ENV_LOG: &str = "DATAZEN_SERVER_LOG";
pub const ENV_DB: &str = "DATAZEN_SERVER_DB";
pub const ENV_SERVER_SECRET: &str = "DATAZEN_SERVER_SECRET";
pub const ENV_SESSION_IDLE_SECS: &str = "DATAZEN_SERVER_SESSION_IDLE_SECS";
pub const ENV_SESSION_ABSOLUTE_SECS: &str = "DATAZEN_SERVER_SESSION_ABSOLUTE_SECS";
pub const DEFAULT_DB_FILE: &str = "datazen-admin.sqlite";

/// 开发默认密钥（仅限本地验证；§4.3/§6.2 的生产密钥必须显式注入）。
pub const DEV_DEFAULT_SERVER_SECRET: &str = "dev-only-datazen-server-secret-change-me";

/// §4.3 的首版建议值（来自部署配置，非协议常量）。
pub const DEFAULT_SESSION_IDLE_SECS: u64 = 8 * 3600;
pub const DEFAULT_SESSION_ABSOLUTE_SECS: u64 = 24 * 3600;

impl ServerConfig {
    /// 从环境变量加载并校验。任一键非法即 [`ServerError::ConfigInvalid`]。
    pub fn from_env() -> Result<Self, ServerError> {
        let listen_addr = read_addr(ENV_LISTEN)?;
        let log_filter = read_filter(ENV_LOG)?;
        let db_path = read_db(ENV_DB)?;
        let server_secret = parse_secret(std::env::var(ENV_SERVER_SECRET).ok())?;
        let session_idle_seconds = parse_seconds(
            ENV_SESSION_IDLE_SECS,
            std::env::var(ENV_SESSION_IDLE_SECS).ok(),
            DEFAULT_SESSION_IDLE_SECS,
        )?;
        let session_absolute_seconds = parse_seconds(
            ENV_SESSION_ABSOLUTE_SECS,
            std::env::var(ENV_SESSION_ABSOLUTE_SECS).ok(),
            DEFAULT_SESSION_ABSOLUTE_SECS,
        )?;
        if server_secret.is_dev_default() {
            tracing::warn!(
                "{} is not set; using the development default server secret — \
                 production deployments must inject a real secret",
                ENV_SERVER_SECRET
            );
        }
        // ck_login_window 的构造期前提：空闲窗不得越过绝对窗（否则每张
        // 会话行都会在 INSERT 时撞 CHECK）。
        if session_idle_seconds > session_absolute_seconds {
            return Err(ServerError::ConfigInvalid(format!(
                "{ENV_SESSION_IDLE_SECS} must not exceed {ENV_SESSION_ABSOLUTE_SECS} \
                 (ck_login_window)"
            )));
        }
        Ok(Self {
            listen_addr,
            log_filter,
            db_path,
            server_secret,
            session_idle_seconds,
            session_absolute_seconds,
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

/// `server_secret` 的解析与校验：显式注入要求 ≥ 32 字符；未配置回落
/// 开发默认值（用默认值时 `from_env` 会告警）。错误消息不回显值原文。
fn parse_secret(raw: Option<String>) -> Result<ServerSecret, ServerError> {
    match raw {
        Some(value) if !value.trim().is_empty() => {
            if value.chars().count() < 32 {
                return Err(ServerError::ConfigInvalid(format!(
                    "{ENV_SERVER_SECRET} must be at least 32 characters"
                )));
            }
            Ok(ServerSecret::injected(value))
        }
        Some(_) => Err(ServerError::ConfigInvalid(format!(
            "{ENV_SERVER_SECRET} must not be blank"
        ))),
        None => Ok(ServerSecret::dev_default()),
    }
}

/// TTL 覆盖项的解析：正整数、≥ 60 秒（低于这个量级的会话寿命只会制造
/// 故障单，不构成安全增益）。
fn parse_seconds(key: &str, raw: Option<String>, default: u64) -> Result<u64, ServerError> {
    match raw {
        None => Ok(default),
        Some(value) => {
            let parsed: u64 = value.trim().parse().map_err(|_| {
                ServerError::ConfigInvalid(format!("{key} must be a positive integer of seconds"))
            })?;
            if parsed < 60 {
                return Err(ServerError::ConfigInvalid(format!(
                    "{key} must be at least 60 seconds"
                )));
            }
            Ok(parsed)
        }
    }
}

/// 密钥值 + 来源标记（是否开发默认值——告警判据，不进日志的只有值本身）。
#[derive(Clone, PartialEq, Eq)]
pub struct ServerSecret(String);

impl ServerSecret {
    fn injected(value: String) -> Self {
        Self(value)
    }

    fn dev_default() -> Self {
        Self(DEV_DEFAULT_SERVER_SECRET.to_string())
    }

    /// 是否仍在使用开发默认密钥（部署告警的判据）。
    pub fn is_dev_default(&self) -> bool {
        self.0 == DEV_DEFAULT_SERVER_SECRET
    }

    /// HMAC 密钥字节（CSRF / policyIsolationKey 消费；不实现 Display/Debug
    /// 序列化——密钥值不进日志）。
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl std::fmt::Debug for ServerSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ServerSecret(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_parsing_rejects_short_and_blank_values_without_echoing_them() {
        let error = parse_secret(Some("short".into())).expect_err("short secret");
        assert!(error.to_string().contains("at least 32 characters"));
        assert!(!error.to_string().contains("short"), "错误消息不回显值原文");
        let error = parse_secret(Some("   ".into())).expect_err("blank secret");
        assert!(error.to_string().contains("must not be blank"));
    }

    #[test]
    fn secret_parsing_falls_back_to_the_dev_default_and_marks_it() {
        let secret = parse_secret(None).expect("default");
        assert!(secret.is_dev_default());
        let secret = parse_secret(Some("x".repeat(40))).expect("injected");
        assert!(!secret.is_dev_default());
    }

    #[test]
    fn ttl_parsing_enforces_the_sixty_second_floor_and_the_default() {
        assert_eq!(
            parse_seconds("K", None, DEFAULT_SESSION_IDLE_SECS).expect("default"),
            DEFAULT_SESSION_IDLE_SECS
        );
        assert_eq!(
            parse_seconds("K", Some("120".into()), 0).expect("override"),
            120
        );
        let error = parse_seconds("K", Some("30".into()), 0).expect_err("too small");
        assert!(error.to_string().contains("at least 60 seconds"));
        let error = parse_seconds("K", Some("soon".into()), 0).expect_err("not a number");
        assert!(error.to_string().contains("positive integer"));
    }

    #[test]
    fn the_dev_secret_is_never_rendered_by_debug() {
        let rendered = format!("{:?}", ServerSecret::dev_default());
        assert_eq!(rendered, "ServerSecret(<redacted>)");
    }
}
