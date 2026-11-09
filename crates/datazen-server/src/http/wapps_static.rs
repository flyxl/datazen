//! W8 刀4：wapp 资产 origin 静态面——WR-F4 冻结 URL 约定的 server 半边。
//!
//! URL 形：`{origin}/wapps/{wapp id}/{package-relative path}?{query}`——把
//! `packages/frontend/src/lib/wappAssetOrigin.ts` 冻结的同源资产约定变成
//! datazen-server 上真正伺服文件的 HTTP 静态面（桌面 `datazen://` 协议桥
//! `crates/datazen-desktop/src/wapps/protocol.rs` 的 HTTP 对偶，桌面零改动）。
//! 设计档：`docs/development/w8-knife4-wapp-static-design.md`；契约归置
//! §5「非域面服务路由」——浏览器原生 iframe/img/script 直取、不经
//! MethodMap，无方法背书要求（`/mcp` 同款先例），因此不进 `domains/`
//! 而与 `sse.rs` 同层直挂 `http/`。
//!
//! # 面公开裁决（设计档 §6.1）
//!
//! v1 静态面**公开（无会话门）**：`WappPageShell` 的 iframe 是
//! `sandbox="allow-scripts"`（无 `allow-same-origin`），wapp 文档运行在
//! opaque origin——它自己发起的子资源请求（`<script src>`/`<img>`/`<link>`
//! 全是包内相对路径）不携带 SameSite=Lax cookie，会话门的结果是入口 200、
//! 全部子资源 401，每个 wapp 页面必坏。桌面 `datazen://` 协议宿主同样是
//! 零认证（OS 用户边界即边界，启用闸 403 是唯一访问控制）。**本面不存在
//! 401 分支**；wapp 字节在产品语义里是静态 UI 资产，不是机密。
//!
//! # 层链效果（设计档 §3.4）
//!
//! L0 requestId / L1a/L1b 体闸 / catch-panic 全路径生效（它们不看路径）；
//! L2/L3/L4/L6 经 `under_api`（`/api/` 前缀）自跳过——「静态资源面只到
//! L0」是认证链的既定层语义；L5 由本刀单独扩展到 `/wapps/` 前缀
//! （`middleware::ratelimit`：公开面 = 未认证高频读面，不限流即 DoS 敞口，
//! 设计档 §6.3）。`build_router()`（D1 骨架，`tests/skeleton.rs` 钉住）
//! 不挂本面——骨架构建下 `/wapps/*` 走 fallback 404，与「未装配 = 显式
//! 形态」一致。
//!
//! # 文法与安全单家在内核
//!
//! 路径安全 / 宽松 percent-decode（非法转义保留原文）/ MIME 白名单 /
//! 启用闸 / 符号链接防换全部复用 `datazen::workspace_apps::protocol`
//! （`parse_wapp_origin_path` + `route_datazen_request`）——server 侧零
//! 重写：前端已有一份 TS 镜像（`wappAssetOrigin.ts`，跨语言互钉是 WR-F4
//! 的既定纪律），同仓再出第三份 Rust 路径规则不可接受。

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, StatusCode, Uri};
use axum::response::Response;
use axum::routing::get;
use axum::Router;

use datazen::workspace_apps::manager::WappManager;
use datazen::workspace_apps::protocol::{
    parse_wapp_origin_path, route_datazen_request, DatazenOutcome, RouteStatus, ORIGIN_ASSET_CSP,
};

/// wapp 资产静态面的装配状态（`wapps_static_router` 的注入物；
/// `crate::mcp::http::HttpMcpState` 同款的 composition-root 预装配形态）。
///
/// `AuthDeps::wapp_assets` 为 `None` = 不挂本面——骨架腿（`build_router()`，
/// `tests/skeleton.rs` 钉住不得改动）与最小部署形态的显式语义，未装配时
/// `/wapps/*` 落 fallback 404。装配点（bootstrap）传 `config.wapps_dir`
/// 构造的 [`WappManager`]：空目录 = 全 404，即天然的「无 wapps」形态。
#[derive(Clone)]
pub struct WappStaticState {
    /// 共享层 wapp 注册表（启用闸 / 路由判定的单家实现；bootstrap 经
    /// `load_from_disk` 扫描装载，非法包 warn 跳过不阻断启动——桌面
    /// `app_state.rs` 同语义）。
    pub manager: std::sync::Arc<WappManager>,
}

// ---------------------------------------------------------------------------
// Router 片段与 handler
// ---------------------------------------------------------------------------

/// wapp 资产静态面的 Router 片段：`GET /wapps`、`GET /wapps/` 与
/// `GET /wapps/{*rest}`（`mcp_router` 同款：主线在 `build_router_with_auth`
/// 内 merge）。
///
/// - 只登记 `get`——POST/PUT 等落 MethodRouter 默认 405（设计档 §9.2 t13
///   验收项）；本面无 HEAD 消费方，HEAD 行为随 axum 默认，不作验收项。
/// - 三条路由**钉死整个 `/wapps` 前缀**：任何畸形形状（`/wapps`、`/wapps/`、
///   `/wapps/acme.bill-audit` 空路径）都由 handler 答 404，绝不落入
///   fallback——WR-E5 的 `ServeDir` + SPA `index.html` 兜底不能把整个壳
///   200 进沙箱 iframe（设计档 §3.3 的 404 钉法）。
pub fn wapps_static_router(state: WappStaticState) -> Router {
    Router::new()
        .route("/wapps", get(handler_wapp_asset))
        // `/wapps/` 必须显式登记：axum 0.8（matchit 0.8）的 `{*rest}` 通配
        // 不匹配空余段，尾斜杠形状若不钉住会漏到 fallback（集成期 t8b
        // 实测；设计档 §3.1 双路由草案据此补第三条，钉死前缀的意图不变）。
        .route("/wapps/", get(handler_wapp_asset))
        .route("/wapps/{*rest}", get(handler_wapp_asset))
        .with_state(state)
}

/// 唯一 handler（三条路由共用；文法判定在内核，本层只做原文重组与映射）。
///
/// **不使用 `Path` 提取器**——它自带 percent-decode 语义，与内核宽松解码
/// （非法转义保留原文、`+` 在路径里不是空格）不保证一致，且 `{*rest}`
/// 通配下 `%2F` 的处理跨版本有歧义。直收 [`Uri`]，取 `uri.path()`（http
/// crate 原样未解码）+ `uri.query()`，重组 `path?query` 原文交内核解析
/// ——冻结用例 `assets%2Ficon.svg` 必须照常服务，这是唯一能保证它的形状。
///
/// 响应映射（设计档 §3.3 逐字）：
///
/// | 内核结论 | HTTP 应答 |
/// | --- | --- |
/// | `parse_wapp_origin_path` Err（未知前缀 / 非法 id 文法 / 空路径 / 解码后不安全路径） | 404 空体——解析失败即 404，绝不静默（对齐桌面 parse-Err 分支） |
/// | `Ok(Asset { content_type, bytes })` | 200 + `Content-Type` + 体 |
/// | `Ok(OpenPage { .. })` | 204 空体——web 形态无宿主事件通道；204 保留「open 是合法首段」的语义可见性 |
/// | `Err(NotFound)` | 404 空体 |
/// | `Err(Forbidden)` | 403 空体——禁用 wapp，桌面 :407-425 同语义 |
///
/// `?v=` 缓存破坏符：查询串进 `QueryMap` 后路由不消费——携带与否同答
/// （缓存策略是逐应答 `no-cache`，见 [`asset_response`]）。
async fn handler_wapp_asset(State(state): State<WappStaticState>, uri: Uri) -> Response {
    // 原文重组：path 未解码 + query 原样，解码纪律全部单家在内核。
    let target = match uri.query() {
        Some(query) => format!("{}?{}", uri.path(), query),
        None => uri.path().to_string(),
    };

    let (wapp_id, path, query) = match parse_wapp_origin_path(&target) {
        Ok(parsed) => parsed,
        Err(error) => {
            // 对齐桌面「rejected malformed」风格：错误文案含路径本身（可记），
            // 不含文件系统细节之外的任何信息。
            tracing::warn!(error = %error, "rejected malformed wapp asset path");
            return asset_response(StatusCode::NOT_FOUND, None, Vec::new());
        }
    };

    match route_datazen_request(&state.manager, &wapp_id, &path, &query) {
        Ok(DatazenOutcome::Asset {
            content_type,
            bytes,
        }) => asset_response(StatusCode::OK, Some(content_type), bytes),
        // web 形态的宿主是浏览器里的 SPA，不是协议处理器——没有
        // `wapps:open-page` 事件通道可发；真正的深链消费方是前端
        // `parseWappAssetTarget`（wapp web 半边刀），这里只回「受理、
        // 无表示」。
        Ok(DatazenOutcome::OpenPage { .. }) => {
            asset_response(StatusCode::NO_CONTENT, None, Vec::new())
        }
        Err(status) => {
            // 错误是裸状态码（404/403）：wapp id 不是机密（publisher.name
            // 文法），HTTP 层不泄露更多细节（对齐桌面 Err 分支的日志形状）。
            tracing::warn!(wapp = %wapp_id, status = ?status, "wapp asset request rejected");
            match status {
                RouteStatus::NotFound => asset_response(StatusCode::NOT_FOUND, None, Vec::new()),
                RouteStatus::Forbidden => asset_response(StatusCode::FORBIDDEN, None, Vec::new()),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 响应构造（安全头逐应答注入）
// ---------------------------------------------------------------------------

/// 桌面 `datazen_response`（`datazen-desktop/src/wapps/protocol.rs`）的
/// axum 对偶：**每条应答（含 403/404/204）注入**——
///
/// - `Content-Security-Policy`：内核 [`ORIGIN_ASSET_CSP`]（与桌面
///   `ASSET_CSP` 同指令集、去 `datazen:` 源——web 形态资产 URL 是真实
///   https origin，`'self'` 有明确匹配物，单源成立）；
/// - `X-Content-Type-Options: nosniff`；
/// - `Cache-Control: no-cache`——wapp 包同 id 重装会换字节（`?v=` 是
///   manifest 版本号不是内容哈希），任何 `immutable`/长 `max-age` 假设
///   都不成立（设计档 §5.4；`no-cache` = 每次用前必须再验证）。
///
/// `Content-Type` 仅 200 Asset 携带——只来自内核白名单 `content_type_for`
/// 的单家结论，未知扩展在内核已 404，本层**绝不猜类型**；非 200 一律空体。
fn asset_response(
    status: StatusCode,
    content_type: Option<&'static str>,
    body: Vec<u8>,
) -> Response {
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_SECURITY_POLICY, ORIGIN_ASSET_CSP)
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CACHE_CONTROL, "no-cache");
    if let Some(content_type) = content_type {
        builder = builder.header(header::CONTENT_TYPE, content_type);
    }
    builder
        .body(Body::from(body))
        .expect("static response parts always build")
}
