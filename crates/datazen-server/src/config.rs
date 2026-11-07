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

    /// 本服务的组织 id（单组织 v1；connection_profile / 登录会话 /
    /// OIDC membership 映射都落在这个组织）。默认 `datazen-local-org`
    /// （中性形态词表——共享内核禁形态专名）。
    pub organization_id: String,

    /// 对外可见 origin（§4.1 的 redirect_uri 构造 + §8.1 的 SSE Origin
    /// 白名单来源）。未配置 = 不挂 OIDC 登录与 SSE 事件流。
    pub public_origin: Option<String>,

    /// OIDC IdP issuer（§4.1；发现文档的 issuer 必须逐字等值）。
    pub oidc_issuer: Option<String>,
    /// OIDC client id（本服务在 IdP 的登记名）。
    pub oidc_client_id: Option<String>,
    /// OIDC client secret（机密客户端；None = 纯 PKCE 公开客户端）。
    pub oidc_client_secret: Option<String>,

    /// SSE 心跳间隔（秒；§8.3 的 15s 建议值，部署可调）。
    pub sse_keepalive_secs: u64,
    /// SSE 首波积压上界（连接管理 §7.7 的每订阅 256 条建议值）。
    pub sse_max_backlog_frames: usize,

    /// L5 默认档桶容量（§9.1：令牌签发/开会话/提交执行/取消为严格档，
    /// 其余默认档；全部来自部署配置）。
    pub rate_default_capacity: u64,
    /// L5 默认档补充速率（令牌/秒）。
    pub rate_default_refill_per_second: f64,
    /// L5 严格档桶容量。
    pub rate_strict_capacity: u64,
    /// L5 严格档补充速率（令牌/秒）。
    pub rate_strict_refill_per_second: f64,
    /// L5 键表上界（未认证 IP 键是内存攻击面）。
    pub rate_max_entries: usize,
    /// WR-D7 受控上传的产物落盘根。
    pub artifact_root: PathBuf,
    /// WR-D6 结果 TTL 清扫周期（秒）。
    pub artifact_ttl_sweep_secs: u64,
    /// WR-D4/D6 预算协调器节点额度。
    pub budget_node_limit: u64,
    /// WR-D4/D6 预算协调器组织额度。
    pub budget_organization_limit: u64,
    /// WR-F7 出站白名单（驱动/隧道出口的主机表；逗号分隔；空表 = 全拒，
    /// fail-closed——「未接线 = 不设防」在 server 形态不允许）。
    pub egress_allowed_hosts: Vec<String>,
    /// WR-E5 静态资源根（web 构建产物目录；None = 不挂同源静态面——
    /// 显式部署形态：独立 origin/CDN 承载时使用）。
    pub web_root: Option<PathBuf>,
    /// IdleReclaimer 武装刀：业务空闲期限（秒）；0 = 本部署不做空闲回收
    /// （`IdlePolicy::disabled()`，配置事实而非运行时事实）。
    pub runtime_idle_ttl_secs: u64,
    /// 空闲扫描间隔（秒）；下限 60（防全表扫描反噬，设计档 R4）。
    pub runtime_idle_scan_secs: u64,
}

/// 环境变量名。`DATAZEN_` 前缀与仓库既有约定对齐。
pub const ENV_LISTEN: &str = "DATAZEN_SERVER_LISTEN";
pub const ENV_LOG: &str = "DATAZEN_SERVER_LOG";
pub const ENV_DB: &str = "DATAZEN_SERVER_DB";
pub const ENV_SERVER_SECRET: &str = "DATAZEN_SERVER_SECRET";
pub const ENV_SESSION_IDLE_SECS: &str = "DATAZEN_SERVER_SESSION_IDLE_SECS";
pub const ENV_SESSION_ABSOLUTE_SECS: &str = "DATAZEN_SERVER_SESSION_ABSOLUTE_SECS";
pub const ENV_ORGANIZATION_ID: &str = "DATAZEN_SERVER_ORGANIZATION_ID";
pub const ENV_PUBLIC_ORIGIN: &str = "DATAZEN_SERVER_PUBLIC_ORIGIN";
pub const ENV_OIDC_ISSUER: &str = "DATAZEN_SERVER_OIDC_ISSUER";
pub const ENV_OIDC_CLIENT_ID: &str = "DATAZEN_SERVER_OIDC_CLIENT_ID";
pub const ENV_OIDC_CLIENT_SECRET: &str = "DATAZEN_SERVER_OIDC_CLIENT_SECRET";
pub const ENV_SSE_KEEPALIVE_SECS: &str = "DATAZEN_SERVER_SSE_KEEPALIVE_SECS";
pub const ENV_SSE_MAX_BACKLOG: &str = "DATAZEN_SERVER_SSE_MAX_BACKLOG";
pub const ENV_RATE_DEFAULT_CAPACITY: &str = "DATAZEN_SERVER_RATE_DEFAULT_CAPACITY";
pub const ENV_RATE_DEFAULT_REFILL: &str = "DATAZEN_SERVER_RATE_DEFAULT_REFILL";
pub const ENV_RATE_STRICT_CAPACITY: &str = "DATAZEN_SERVER_RATE_STRICT_CAPACITY";
pub const ENV_RATE_STRICT_REFILL: &str = "DATAZEN_SERVER_RATE_STRICT_REFILL";
pub const ENV_RATE_MAX_ENTRIES: &str = "DATAZEN_SERVER_RATE_MAX_ENTRIES";
pub const ENV_ARTIFACT_ROOT: &str = "DATAZEN_SERVER_ARTIFACT_ROOT";
pub const ENV_ARTIFACT_TTL_SWEEP_SECS: &str = "DATAZEN_SERVER_ARTIFACT_TTL_SWEEP_SECS";
pub const ENV_BUDGET_NODE_LIMIT: &str = "DATAZEN_SERVER_BUDGET_NODE_LIMIT";
pub const ENV_BUDGET_ORGANIZATION_LIMIT: &str = "DATAZEN_SERVER_BUDGET_ORGANIZATION_LIMIT";
pub const ENV_RUNTIME_IDLE_TTL_SECS: &str = "DATAZEN_SERVER_RUNTIME_IDLE_TTL_SECS";
pub const ENV_RUNTIME_IDLE_SCAN_SECS: &str = "DATAZEN_SERVER_RUNTIME_IDLE_SCAN_SECS";
/// WR-F7 出站白名单（逗号分隔 host；空/未设 = 全拒）。
pub const ENV_EGRESS_ALLOWED_HOSTS: &str = "DATAZEN_SERVER_EGRESS_ALLOWED_HOSTS";
/// WR-E5 静态资源根（web 构建产物；未设 = 不挂同源静态面）。
pub const ENV_WEB_ROOT: &str = "DATAZEN_SERVER_WEB_ROOT";
pub const DEFAULT_DB_FILE: &str = "datazen-admin.sqlite";

/// 开发默认密钥（仅限本地验证；§4.3/§6.2 的生产密钥必须显式注入）。
pub const DEV_DEFAULT_SERVER_SECRET: &str = "dev-only-datazen-server-secret-change-me";

/// §4.3 的首版建议值（来自部署配置，非协议常量）。
pub const DEFAULT_SESSION_IDLE_SECS: u64 = 8 * 3600;
pub const DEFAULT_SESSION_ABSOLUTE_SECS: u64 = 24 * 3600;

/// 单组织 v1 的默认组织 id（中性形态词表，见字段文档）。
pub const DEFAULT_ORGANIZATION_ID: &str = "datazen-local-org";
/// §8.3 的心跳建议值。
pub const DEFAULT_SSE_KEEPALIVE_SECS: u64 = 15;
/// 连接管理 §7.7 的每订阅投递上限建议值。
pub const DEFAULT_SSE_MAX_BACKLOG: usize = 256;
/// §9.1 的 L5 首版建议值（宽松默认档 + 紧严格档；全部部署可调）。
pub const DEFAULT_RATE_DEFAULT_CAPACITY: u64 = 120;
pub const DEFAULT_RATE_DEFAULT_REFILL: f64 = 20.0;
pub const DEFAULT_RATE_STRICT_CAPACITY: u64 = 10;
pub const DEFAULT_RATE_STRICT_REFILL: f64 = 1.0;
pub const DEFAULT_RATE_MAX_ENTRIES: usize = 65_536;
/// WR-D7 受控上传的产物落盘根（相对 cwd；部署显式注入绝对路径）。
pub const DEFAULT_ARTIFACT_ROOT: &str = "data/artifacts";
/// WR-D6 结果 TTL 清扫周期（与 `operations::ttl` 登记值一致）。
pub const DEFAULT_ARTIFACT_TTL_SWEEP_SECS: u64 = 300;
/// WR-D4/D6 预算协调器的首版额度起点（部署可调；非协议常量）。
pub const DEFAULT_BUDGET_NODE_LIMIT: u64 = 64;
pub const DEFAULT_BUDGET_ORGANIZATION_LIMIT: u64 = 32;
/// IdleReclaimer 武装刀：业务空闲期限默认 30 分钟（§9.2 同族，内核
/// `IdlePolicy::default()` 本源）；扫描间隔默认 60 秒（最坏 overshoot
/// 1 分钟；下限 60s，防全表扫描反噬）。
pub const DEFAULT_RUNTIME_IDLE_TTL_SECS: u64 = 1800;
pub const DEFAULT_RUNTIME_IDLE_SCAN_SECS: u64 = 60;

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
        let organization_id = match std::env::var(ENV_ORGANIZATION_ID).ok() {
            Some(raw) if !raw.trim().is_empty() => raw.trim().to_string(),
            _ => DEFAULT_ORGANIZATION_ID.to_string(),
        };
        let public_origin = read_optional_origin(ENV_PUBLIC_ORIGIN)?;
        let oidc_issuer = read_optional_text(ENV_OIDC_ISSUER)?;
        let oidc_client_id = read_optional_text(ENV_OIDC_CLIENT_ID)?;
        let oidc_client_secret = read_optional_text(ENV_OIDC_CLIENT_SECRET)?;
        // OIDC 三件套必须成套出现（issuer + client_id 是最小面；secret 可缺）。
        if oidc_issuer.is_some() != oidc_client_id.is_some() {
            return Err(ServerError::ConfigInvalid(format!(
                "{ENV_OIDC_ISSUER} and {ENV_OIDC_CLIENT_ID} must be configured together"
            )));
        }
        if oidc_issuer.is_some() && public_origin.is_none() {
            return Err(ServerError::ConfigInvalid(format!(
                "{ENV_PUBLIC_ORIGIN} is required when OIDC login is configured \
                 (redirect_uri construction)"
            )));
        }
        let sse_keepalive_secs = parse_seconds(
            ENV_SSE_KEEPALIVE_SECS,
            std::env::var(ENV_SSE_KEEPALIVE_SECS).ok(),
            DEFAULT_SSE_KEEPALIVE_SECS,
        )?;
        let sse_max_backlog_frames = parse_usize(
            ENV_SSE_MAX_BACKLOG,
            std::env::var(ENV_SSE_MAX_BACKLOG).ok(),
            DEFAULT_SSE_MAX_BACKLOG,
        )?;
        let rate_default_capacity = parse_u64(
            ENV_RATE_DEFAULT_CAPACITY,
            std::env::var(ENV_RATE_DEFAULT_CAPACITY).ok(),
            DEFAULT_RATE_DEFAULT_CAPACITY,
        )?;
        let rate_default_refill_per_second = parse_f64(
            ENV_RATE_DEFAULT_REFILL,
            std::env::var(ENV_RATE_DEFAULT_REFILL).ok(),
            DEFAULT_RATE_DEFAULT_REFILL,
        )?;
        let rate_strict_capacity = parse_u64(
            ENV_RATE_STRICT_CAPACITY,
            std::env::var(ENV_RATE_STRICT_CAPACITY).ok(),
            DEFAULT_RATE_STRICT_CAPACITY,
        )?;
        let rate_strict_refill_per_second = parse_f64(
            ENV_RATE_STRICT_REFILL,
            std::env::var(ENV_RATE_STRICT_REFILL).ok(),
            DEFAULT_RATE_STRICT_REFILL,
        )?;
        let rate_max_entries = parse_usize(
            ENV_RATE_MAX_ENTRIES,
            std::env::var(ENV_RATE_MAX_ENTRIES).ok(),
            DEFAULT_RATE_MAX_ENTRIES,
        )?;
        let artifact_root = match std::env::var(ENV_ARTIFACT_ROOT).ok() {
            Some(raw) if !raw.trim().is_empty() => PathBuf::from(raw.trim().to_string()),
            _ => PathBuf::from(DEFAULT_ARTIFACT_ROOT),
        };
        let artifact_ttl_sweep_secs = parse_seconds(
            ENV_ARTIFACT_TTL_SWEEP_SECS,
            std::env::var(ENV_ARTIFACT_TTL_SWEEP_SECS).ok(),
            DEFAULT_ARTIFACT_TTL_SWEEP_SECS,
        )?;
        let budget_node_limit = parse_u64(
            ENV_BUDGET_NODE_LIMIT,
            std::env::var(ENV_BUDGET_NODE_LIMIT).ok(),
            DEFAULT_BUDGET_NODE_LIMIT,
        )?;
        let budget_organization_limit = parse_u64(
            ENV_BUDGET_ORGANIZATION_LIMIT,
            std::env::var(ENV_BUDGET_ORGANIZATION_LIMIT).ok(),
            DEFAULT_BUDGET_ORGANIZATION_LIMIT,
        )?;
        // 出站白名单：逗号分隔，空白/未设 = 空表（ServerNetworkProvider 的
        // fail-closed 语义：空白名单全拒）。
        let egress_allowed_hosts = match std::env::var(ENV_EGRESS_ALLOWED_HOSTS).ok() {
            Some(raw) => raw
                .split(',')
                .map(str::trim)
                .filter(|host| !host.is_empty())
                .map(str::to_owned)
                .collect(),
            None => Vec::new(),
        };
        // WR-E5：web 构建产物根。空/未设 = 不挂同源静态面（独立 origin
        // /CDN 承载的显式部署形态）。
        let web_root = match std::env::var(ENV_WEB_ROOT).ok() {
            Some(raw) if !raw.trim().is_empty() => Some(PathBuf::from(raw.trim().to_string())),
            _ => None,
        };
        // IdleReclaimer 武装刀：两键。TTL 未设 = 1800；显式 0 = disabled（
        // 「不做空闲回收」是配置事实）——用允许 0 的变体解析（parse_seconds
        // 的 60s 下限会拒绝 disabled 语义）。扫描间隔下限 60s（设计档 R4：
        // 全表扫描是 O(会话数) 的 actor 往返，过勤反噬）；未设 = 60。
        let runtime_idle_ttl_secs = parse_seconds_allow_zero(
            ENV_RUNTIME_IDLE_TTL_SECS,
            std::env::var(ENV_RUNTIME_IDLE_TTL_SECS).ok(),
            DEFAULT_RUNTIME_IDLE_TTL_SECS,
        )?;
        let runtime_idle_scan_secs = parse_seconds(
            ENV_RUNTIME_IDLE_SCAN_SECS,
            std::env::var(ENV_RUNTIME_IDLE_SCAN_SECS).ok(),
            DEFAULT_RUNTIME_IDLE_SCAN_SECS,
        )?;
        if runtime_idle_scan_secs < 60 {
            return Err(ServerError::ConfigInvalid(format!(
                "{ENV_RUNTIME_IDLE_SCAN_SECS} must be >= 60 (scan interval lower bound)"
            )));
        }
        Ok(Self {
            listen_addr,
            log_filter,
            db_path,
            server_secret,
            session_idle_seconds,
            session_absolute_seconds,
            organization_id,
            public_origin,
            oidc_issuer,
            oidc_client_id,
            oidc_client_secret,
            sse_keepalive_secs,
            sse_max_backlog_frames,
            rate_default_capacity,
            rate_default_refill_per_second,
            rate_strict_capacity,
            rate_strict_refill_per_second,
            rate_max_entries,
            artifact_root,
            artifact_ttl_sweep_secs,
            budget_node_limit,
            budget_organization_limit,
            egress_allowed_hosts,
            web_root,
            runtime_idle_ttl_secs,
            runtime_idle_scan_secs,
        })
    }
}

fn read_optional_text(key: &str) -> Result<Option<String>, ServerError> {
    match std::env::var(key) {
        Ok(raw) => {
            let trimmed = raw.trim().to_string();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed))
            }
        }
        Err(_) => Ok(None),
    }
}

fn read_optional_origin(key: &str) -> Result<Option<String>, ServerError> {
    let Some(value) = read_optional_text(key)? else {
        return Ok(None);
    };
    // origin 形态校验（§8.1 的等值比较对象）：scheme://authority，无路径。
    if !(value.starts_with("https://") || value.starts_with("http://")) || value.contains('/') {
        // 允许结尾恰好一个 '/'？Origin 头不含路径——这里拒绝任何 '/'。
        return Err(ServerError::ConfigInvalid(format!(
            "{key} must be a bare origin (scheme://host[:port]) without a path"
        )));
    }
    Ok(Some(value))
}

fn parse_usize(key: &str, raw: Option<String>, default: usize) -> Result<usize, ServerError> {
    let Some(raw) = raw else { return Ok(default) };
    raw.trim()
        .parse::<usize>()
        .map_err(|_| ServerError::ConfigInvalid(format!("{key} must be a non-negative integer")))
}

fn parse_u64(key: &str, raw: Option<String>, default: u64) -> Result<u64, ServerError> {
    let Some(raw) = raw else { return Ok(default) };
    raw.trim()
        .parse::<u64>()
        .map_err(|_| ServerError::ConfigInvalid(format!("{key} must be a non-negative integer")))
}

fn parse_f64(key: &str, raw: Option<String>, default: f64) -> Result<f64, ServerError> {
    let Some(raw) = raw else { return Ok(default) };
    let value = raw
        .trim()
        .parse::<f64>()
        .map_err(|_| ServerError::ConfigInvalid(format!("{key} must be a number")))?;
    if !value.is_finite() || value <= 0.0 {
        return Err(ServerError::ConfigInvalid(format!(
            "{key} must be a positive finite number"
        )));
    }
    Ok(value)
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
        Ok(raw) => raw.parse::<SocketAddr>().map_err(|_| {
            ServerError::ConfigInvalid(format!("{key} must be host:port (e.g. 127.0.0.1:8080)"))
        }),
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
                Err(ServerError::ConfigInvalid(format!(
                    "{key} must not be blank"
                )))
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

/// 同 [`parse_seconds`] 但允许 `0`（IdleReclaimer 武装刀的 TTL 键专用：
/// `0` = `IdlePolicy::disabled()`，是配置事实而非非法值）。
fn parse_seconds_allow_zero(
    key: &str,
    raw: Option<String>,
    default: u64,
) -> Result<u64, ServerError> {
    match raw {
        None => Ok(default),
        Some(value) => {
            let parsed: u64 = value.trim().parse().map_err(|_| {
                ServerError::ConfigInvalid(format!(
                    "{key} must be a non-negative integer of seconds"
                ))
            })?;
            Ok(parsed)
        }
    }
}

/// IdleReclaimer 武装刀：config 两键 → 内核 `IdlePolicy` 的唯一映射（
/// 设计档 §6.1）。`(0, _) → disabled`；其余 → `with_deadline(ttl, scan)`。
/// bootstrap 与测试同源调用——映射规则单家，不复制。
pub fn idle_policy_from_config(
    ttl_secs: u64,
    scan_secs: u64,
) -> datazen::execution::application::orchestration::IdlePolicy {
    use datazen::execution::application::orchestration::IdlePolicy;
    if ttl_secs == 0 {
        IdlePolicy::disabled()
    } else {
        IdlePolicy::with_deadline(
            std::time::Duration::from_secs(ttl_secs),
            std::time::Duration::from_secs(scan_secs.max(1)),
        )
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
