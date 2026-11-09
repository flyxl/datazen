//! W8 刀4：wapp 资产 origin 静态面集成钉子（设计档 §9.2 的 t1–t14；t15 的
//! L5 门在 `ratelimit.rs` 单测）。
//!
//! 夹具沿 `workflow_routes.rs` 的全栈装配面（`build_router_with_auth` +
//! `AuthDeps` 全字面量构造），只点亮 `wapp_assets` 槽、其余域不挂。
//! `/wapps/` 是**公开面**（设计档 §6.1）：L2/L3/L4/L6 经 `under_api`
//! 自跳过（非 `/api/` 前缀），因此无需登录夹具——未登录请求与登录态
//! 同答，本面不存在 401 分支。
//!
//! 包夹具沿桌面 `desktop/wapps/protocol.rs` 的 `manager_with_page_wapp`
//! 形状（manifest/index.html/assets/隐藏分量/`.enabled`），manifest 按
//! `manifest.rs` 的真实 schema 写（serde camelCase + deny_unknown_fields、
//! apiVersion==2、id 文法、entry/pages 声明、包级白名单扫描），必须能过
//! `WappManager::load_from_disk` 的真实校验。
//!
//! 三层用例互抄纪律（WR-F4「两形态同绿」的延伸）：TS 冻结用例
//! （`wappAssetOrigin.test.ts`）/ 内核单测（`datazen/src/wapps/protocol.rs`）
//! 与本套件共享同一份文法清单——t4/t5 的六条邪恶路径与 t6 的非法 host
//! 四例逐字取自冻结用例；t1 的 CSP 断言把 `wappAssetOrigin.ts:30-32` 的
//! `WAPP_ASSET_CSP` 冻结字面量原文抄进来（跨语言互钉，与
//! `wappAssetOrigin.test.ts:115-131` 对桌面字面量的做法对称）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::Router;
use tower::ServiceExt;

use datazen::workspace_apps::manager::{WappManager, ENABLED_MARKER_FILE};
use datazen::workspace_apps::protocol::ORIGIN_ASSET_CSP;
use datazen_server::http::router::{build_router_with_auth, AuthDeps};
use datazen_server::http::wapps_static::WappStaticState;
use datazen_server::policy::PolicyService;
use datazen_server::storage::AdminStore;

const SERVER_SECRET: &[u8] = b"w8-wapps-static-routes-secret";
const WAPP_ID: &str = "acme.bill-audit";

/// 前端冻结字面量 `WAPP_ASSET_CSP`（`wappAssetOrigin.ts:30-32`）的逐字节
/// 抄录——server 每条应答的 CSP 头必须与它相等（跨语言互钉）。Rust 的
/// `\` 续行剥掉下一行前导空白，拼接结果与 TS 两段字符串字面量相加
/// 逐字节一致（桌面 `ASSET_CSP` 常量同一写法）。
const WAPP_ASSET_CSP_FROZEN: &str = "default-src 'self'; script-src 'self'; \
     style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; \
     connect-src 'none'";

/// manifest 按 `manifest.rs` 真实 schema：apiVersion==2（握手）、
/// `<publisher>.<name>` id 文法、semver 版本、pages 贡献 + entry + 页级
/// icon（规则 4/5：entry 与 page icon 必须是盘上存在的白名单文件）。
const PAGE_MANIFEST: &str = r#"{
  "id": "acme.bill-audit",
  "name": "Bill Audit",
  "version": "1.0.0",
  "apiVersion": 2,
  "entry": "index.html",
  "contributes": {
    "pages": [{ "id": "quota-check", "title": "Quota Check", "icon": "assets/icon.svg" }]
  }
}"#;

/// 入口文档：体逐字节断言的目标（含换行与 doctype，防止「碰巧同长度」）。
const INDEX_HTML: &str = "<!doctype html>\n<html><body>bill-audit</body></html>\n";
/// SVG 夹具必须过 `validate_svg_content`（禁 `<script` / `javascript:` /
/// 事件属性）——manifest 页级 icon 的规则 5 扫描会读它。
const ICON_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\"/>\n";
const DATA_JSON: &str = "{\"k\":1}\n";
/// t8b 的 SPA shell（WR-E5 形状 fallback 的产物；与 INDEX_HTML 内容刻意
/// 不同，断言「不落 shell」时不会假绿）。
const SPA_SHELL: &str =
    "<!doctype html>\n<html><head><title>spa shell</title></head><body><div id=\"root\"></div></body></html>\n";

// ── tempdir 守卫与写盘助手（桌面 protocol.rs 测试同款形状） ────────────────

/// 极简 tempdir 守卫：datazen-server 测试面没有 `tempfile` dev-dep
/// （`workflow_routes.rs` 等既有套件均不落盘），用 `std::env::temp_dir` +
/// uuid 唯一目录 + Drop 清理达到 `tempfile::TempDir` 同语义。
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("dz-wapps-static-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("temp dir");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write_file(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().expect("parent dir")).expect("create dirs");
    std::fs::write(path, content).expect("write file");
}

/// `acme.bill-audit` 包（桌面 `manager_with_page_wapp` 同形状）：manifest +
/// index.html + 根级 icon.svg + 嵌套 assets/icon.svg + data.json + 隐藏分量
/// `.storage.json`（供 t5 证明「在盘上仍被拒」）+ `.enabled` 标记文件。
/// dot 前缀条目被包扫描跳过（宿主自管状态），其余全部过白名单扫描。
fn write_page_wapp(root: &Path) {
    let pkg = root.join(WAPP_ID);
    write_file(&pkg, "manifest.json", PAGE_MANIFEST);
    write_file(&pkg, "index.html", INDEX_HTML);
    write_file(&pkg, "icon.svg", ICON_SVG);
    write_file(&pkg, "assets/icon.svg", ICON_SVG);
    write_file(&pkg, "data.json", DATA_JSON);
    write_file(&pkg, ".storage.json", "{}");
    write_file(&pkg, ENABLED_MARKER_FILE, "1\n");
}

// ── 夹具（公开面：无登录、无域路由，只点亮 wapp_assets 槽） ────────────────

struct WappsFixture {
    app: Router,
    root: TempDir,
    manager: Arc<WappManager>,
}

async fn wapps_static_fixture() -> WappsFixture {
    let store = Arc::new(AdminStore::open_in_memory().expect("memory store"));
    store.migrate().expect("migrations");

    let root = TempDir::new("root");
    write_page_wapp(root.path());

    // 真实装载链：manifest 校验失败会静默跳过（load_from_disk 的 warn 路径），
    // 所以这里断言计数——夹具 manifest 若不合规，测试在这里红而不是假 404。
    let manager = Arc::new(WappManager::new(root.path().to_path_buf()));
    let loaded = manager.load_from_disk();
    assert!(
        loaded >= 1,
        "fixture wapp must load from disk (got {loaded})"
    );

    let policy =
        Arc::new(PolicyService::new(store.clone(), SERVER_SECRET.to_vec()).expect("policy"));
    let app = build_router_with_auth(AuthDeps {
        store,
        policy,
        server_secret: SERVER_SECRET.to_vec(),
        idle_ttl_seconds: 3600,
        rate_limit: None,
        sse: None,
        oidc: None,
        domains: None,
        mcp: None,
        domain_routers: None,
        wapp_assets: Some(WappStaticState {
            manager: manager.clone(),
        }),
    })
    .expect("router");

    WappsFixture { app, root, manager }
}

// ── 请求与断言助手（skeleton.rs / workflow_routes.rs 的三元组形状） ─────────

/// 派发一条空体请求，返回（状态码，响应头，原始体字节）。
async fn call(app: &Router, method: Method, uri: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .expect("request builds");
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("oneshot dispatch");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    (status, headers, bytes.to_vec())
}

async fn get(app: &Router, uri: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
    call(app, Method::GET, uri).await
}

/// 取头值（缺头返回空串——配合 assert_eq 直接红出「头缺失」）。
/// 参数取 `&HeaderName`：对 const 形与 static 形的标准头常量都成立。
fn header_value<'a>(headers: &'a HeaderMap, name: &header::HeaderName) -> &'a str {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
}

// ── t1：happy path——冻结 CSP 字面量逐指令对齐 ──────────────────────────────

#[tokio::test]
async fn t1_happy_entry_serves_bytes_with_frozen_csp() {
    let fx = wapps_static_fixture().await;

    let (status, headers, body) = get(&fx.app, "/wapps/acme.bill-audit/index.html?v=1.0.0").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(header_value(&headers, &header::CONTENT_TYPE), "text/html");
    assert_eq!(body, INDEX_HTML.as_bytes(), "体逐字节等于写入内容");

    // CSP 头逐指令等于前端冻结字面量（wappAssetOrigin.ts:30-32 原文抄进
    // 断言），且不含 `datazen:` 源（web 形态单源化——与桌面 ASSET_CSP 的
    // 唯一差异）；同时与内核常量 ORIGIN_ASSET_CSP 互钉（server 消费内核
    // 单家值，第三份字面量即规则漂移）。
    let csp = header_value(&headers, &header::CONTENT_SECURITY_POLICY);
    assert_eq!(csp, WAPP_ASSET_CSP_FROZEN, "CSP 必须逐指令等于冻结字面量");
    assert!(!csp.contains("datazen:"), "web 形态 CSP 不得带 datazen: 源");
    assert_eq!(
        csp, ORIGIN_ASSET_CSP,
        "server 头必须来自内核 ORIGIN_ASSET_CSP"
    );

    assert_eq!(
        header_value(&headers, &header::X_CONTENT_TYPE_OPTIONS),
        "nosniff"
    );
    assert_eq!(header_value(&headers, &header::CACHE_CONTROL), "no-cache");
}

// ── t2：MIME 表（内核 content_type_for 的 HTTP 半边） ───────────────────────

#[tokio::test]
async fn t2_mime_table_covers_svg_json_and_nested_assets() {
    let fx = wapps_static_fixture().await;

    for (uri, expected_type, expected_body) in [
        ("/wapps/acme.bill-audit/icon.svg", "image/svg+xml", ICON_SVG),
        (
            "/wapps/acme.bill-audit/data.json",
            "application/json",
            DATA_JSON,
        ),
        (
            "/wapps/acme.bill-audit/assets/icon.svg",
            "image/svg+xml",
            ICON_SVG,
        ),
    ] {
        let (status, headers, body) = get(&fx.app, uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_eq!(
            header_value(&headers, &header::CONTENT_TYPE),
            expected_type,
            "{uri}"
        );
        assert_eq!(body, expected_body.as_bytes(), "{uri}");
    }
}

// ── t3：宽松 percent-decode（冻结 TS 用例 :74-76 的 server 半边） ───────────

#[tokio::test]
async fn t3_lenient_percent_decode_serves_encoded_separator() {
    let fx = wapps_static_fixture().await;

    // `assets%2Ficon.svg` 解码为 `assets/icon.svg` 后照常服务——校验发生在
    // 解码**之后**，合法转义不被拒绝；这正是 handler 直收 `uri.path()`
    // 原文（不用 axum Path 提取器）的形状依据（设计档 §3.1）。
    let (status, headers, body) = get(&fx.app, "/wapps/acme.bill-audit/assets%2Ficon.svg").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        header_value(&headers, &header::CONTENT_TYPE),
        "image/svg+xml"
    );
    assert_eq!(body, ICON_SVG.as_bytes());
}

// ── t4：穿越 / 反斜杠 / NUL（冻结六条）+ `..` 族 ───────────────────────────

#[tokio::test]
async fn t4_traversal_backslash_nul_and_dot_family_are_404_empty() {
    let fx = wapps_static_fixture().await;

    let cases = [
        // 冻结六条（wappAssetOrigin.test.ts:97-106 逐字）。
        "/wapps/acme.bill-audit/%2e%2e/%2e%2e/settings.json",
        "/wapps/acme.bill-audit/.storage.json",
        "/wapps/acme.bill-audit/.enabled",
        "/wapps/acme.bill-audit/dir/.secret.png",
        "/wapps/acme.bill-audit/a%5Cb.html",
        "/wapps/acme.bill-audit/%00.html",
        // `..` 族（桌面 unsafe_paths_are_rejected 的 HTTP 半边）：
        // 解码后穿越 / 隐藏分量 / 空分量全部同答 404。
        "/wapps/acme.bill-audit/..",
        "/wapps/acme.bill-audit/../secret.txt",
        "/wapps/acme.bill-audit/assets/../..",
        "/wapps/acme.bill-audit/a/../../b.png",
        "/wapps/acme.bill-audit/.hidden/file.html",
        "/wapps/acme.bill-audit//etc/passwd",
    ];
    for uri in cases {
        let (status, _, body) = get(&fx.app, uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        assert!(
            body.is_empty(),
            "{uri}: 错误应答必须空体（got {} bytes）",
            body.len()
        );
    }
}

// ── t5：隐藏分量拒绝（在盘上仍拒绝——不是文件缺失的假 404） ─────────────────

#[tokio::test]
async fn t5_hidden_components_are_never_served() {
    let fx = wapps_static_fixture().await;

    let pkg = fx.root.path().join(WAPP_ID);
    // `dir/.secret.png` 装载后落盘：dot 前缀分量被包扫描跳过，模拟绕过
    // 安装期校验的存在物（内核拒绝发生在请求时的分量文法，与装载无关）。
    write_file(&pkg, "dir/.secret.png", "png");
    assert!(
        pkg.join(".storage.json").is_file(),
        ".storage.json 必须在盘上"
    );
    assert!(
        pkg.join(ENABLED_MARKER_FILE).is_file(),
        ".enabled 必须在盘上"
    );
    assert!(
        pkg.join("dir/.secret.png").is_file(),
        "夹具必须落盘隐藏文件"
    );

    for uri in [
        "/wapps/acme.bill-audit/.storage.json",
        "/wapps/acme.bill-audit/.enabled",
        "/wapps/acme.bill-audit/dir/.secret.png",
    ] {
        let (status, _, body) = get(&fx.app, uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        assert!(body.is_empty(), "{uri}");
    }
}

// ── t6：未知 id 与非法 host 文法（parse 失败统一 404，不是 400） ────────────

#[tokio::test]
async fn t6_unknown_id_and_invalid_host_grammar_are_404_not_400() {
    let fx = wapps_static_fixture().await;

    let cases = [
        // 未知 id：文法合法但未注册 → route 层 404。
        "/wapps/acme.ghost/index.html",
        // 非法 host 文法四例（冻结用例 :89-93 逐字）：缺 publisher 点号 /
        // 大写 / 空段——parse 失败统一 404（「解析失败即 404，绝不静默」），
        // 本面没有 400 分支。
        "/wapps/bill-audit/index.html",
        "/wapps/Acme.bill-audit/index.html",
        "/wapps/acme./index.html",
        "/wapps/.bill/index.html",
    ];
    for uri in cases {
        let (status, _, body) = get(&fx.app, uri).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "{uri}（parse 失败必须是 404 而非 400）"
        );
        assert!(body.is_empty(), "{uri}");
    }
}

// ── t7：启用闸（禁用 → 403；重启用 → 200） ─────────────────────────────────

#[tokio::test]
async fn t7_disabled_wapp_is_403_then_reenabled_is_200() {
    let fx = wapps_static_fixture().await;
    let marker = fx.root.path().join(WAPP_ID).join(ENABLED_MARKER_FILE);

    // 禁用 = 移除 `.enabled` 标记并重载注册表（v1 无管理写面，状态由落盘
    // 事实决定、重启扫描生效——设计档 §6.2）。
    std::fs::remove_file(&marker).expect("remove enabled marker");
    fx.manager.load_from_disk();

    let (status, _, body) = get(&fx.app, "/wapps/acme.bill-audit/index.html").await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "禁用 wapp 必须是 403（桌面同语义）"
    );
    assert!(body.is_empty(), "403 应答必须空体");

    // 重启用 = 重新落 `.enabled` 并重载注册表。
    std::fs::write(&marker, b"1\n").expect("re-write enabled marker");
    fx.manager.load_from_disk();

    let (status, _, body) = get(&fx.app, "/wapps/acme.bill-audit/index.html").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, INDEX_HTML.as_bytes());
}

// ── t8 / t8b：畸形形状吃死整个前缀，SPA fallback 武装后仍不落 shell ────────

#[tokio::test]
async fn t8_malformed_shapes_stay_404_even_with_spa_fallback_armed() {
    let malformed = ["/wapps", "/wapps/", "/wapps/acme.bill-audit"];

    // t8：基础形态（fallback 仍是 api_or_not_found）。
    let fx = wapps_static_fixture().await;
    for uri in malformed {
        let (status, _, body) = get(&fx.app, uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        assert!(body.is_empty(), "{uri}");
    }

    // t8b：SPA fallback 武装后，畸形 `/wapps/*` 仍由本面答 404——绝不把
    // 整个 SPA 壳 200 进沙箱 iframe。
    //
    // 形状说明（集成期发现，tower-http 0.6.11 `serve_dir/mod.rs:223-241`
    // 文档）：`not_found_service` 会把 fallback 应答**强制包成 404**（
    // `SetStatus` 语义）——生产 bootstrap.rs 的 WR-E5 装配用的正是它，
    // 即生产 SPA shell 一直以 404 状态码在服务（浏览器照渲染 body，故
    // 无人发现）。本测试按**正确的 SPA 形状**武装（`ServeDir::fallback(
    // ServeFile)` 保留 200），正向对照断言 200+shell；生产侧
    // `.not_found_service` 的存量缺陷（连同 `/api/` 未知路径经
    // fallback_service 替换后不再落 `api_or_not_found` 的 JSON 契约面）
    // 在台账登记为后续 mini-slice，不随本刀扩刀。
    let web_root = TempDir::new("web-root");
    write_file(web_root.path(), "index.html", SPA_SHELL);
    let app = fx.app.clone().fallback_service(
        tower_http::services::ServeDir::new(web_root.path()).fallback(
            tower_http::services::ServeFile::new(web_root.path().join("index.html")),
        ),
    );

    // 正向对照：未认领路径确实落 SPA shell（200）——证明 fallback 真的
    // 挂上了，下面的 404 不是「fallback 没武装」的假绿。
    let (status, _, body) = get(&app, "/some/unclaimed/route").await;
    assert_eq!(status, StatusCode::OK, "对照：未认领路径应落 SPA shell");
    assert_eq!(
        body,
        SPA_SHELL.as_bytes(),
        "对照：应答体是 shell 的 index.html"
    );

    for uri in malformed {
        let (status, _, body) = get(&app, uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        assert!(
            body.is_empty() && body != SPA_SHELL.as_bytes(),
            "{uri}: 不得落 SPA shell"
        );
    }
}

// ── t9：open 深链（合法 page → 204；未知 page / open/sub → 404） ───────────

#[tokio::test]
async fn t9_open_deep_link_is_204_and_unknown_pages_404() {
    let fx = wapps_static_fixture().await;

    // 合法 page → 204 空体（受理、无表示：web 形态无宿主事件通道，设计档
    // §3.3），安全头照注入（「每条应答」契约含 204）。
    let (status, headers, body) = get(
        &fx.app,
        "/wapps/acme.bill-audit/open?page=quota-check&uid=123",
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(body.is_empty(), "204 应答必须空体");
    assert_eq!(
        header_value(&headers, &header::CONTENT_SECURITY_POLICY),
        WAPP_ASSET_CSP_FROZEN
    );
    assert_eq!(
        header_value(&headers, &header::X_CONTENT_TYPE_OPTIONS),
        "nosniff"
    );
    assert_eq!(header_value(&headers, &header::CACHE_CONTROL), "no-cache");

    // 未知 page / 缺 page 参数 / open 后带子段 → 404（内核 open 分支：
    // segments.next().is_some() 或 page 未贡献均 NotFound）。
    for uri in [
        "/wapps/acme.bill-audit/open?page=nope",
        "/wapps/acme.bill-audit/open",
        "/wapps/acme.bill-audit/open?other=1",
        "/wapps/acme.bill-audit/open/sub",
    ] {
        let (status, _, body) = get(&fx.app, uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        assert!(body.is_empty(), "{uri}");
    }
}

// ── t10：扩展白名单（装载后落盘的 blob.txt → 404） ─────────────────────────

#[tokio::test]
async fn t10_unknown_extension_is_404_even_when_file_exists() {
    let fx = wapps_static_fixture().await;

    // 装载后向包目录落盘 `blob.txt`：绕过安装期白名单校验的模拟（桌面
    // unknown_extension_yields_not_found_even_when_file_exists 同款）。
    // 路由期 content_type_for("txt") → None → 404，绝不猜类型。
    write_file(&fx.root.path().join(WAPP_ID), "blob.txt", "nope");

    let (status, _, body) = get(&fx.app, "/wapps/acme.bill-audit/blob.txt").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.is_empty());
}

// ── t11：符号链接逃逸（ensure_within_wapp_dir 的请求时拦截） ────────────────

#[cfg(unix)]
#[tokio::test]
async fn t11_symlink_escape_out_of_wapps_root_is_404() {
    let fx = wapps_static_fixture().await;

    // 装载后植入指向 wapps 根之外的 symlink（目标是一个真实文件、扩展在
    // 白名单内）——canonicalize 双侧包含性核对在请求时拦截（Unix 语义，
    // 内核 manifest_tests 的 symlink 用例同款平台门）。
    let outside = TempDir::new("outside");
    let target = outside.path().join("escape-target.svg");
    std::fs::write(&target, ICON_SVG).expect("outside target file");
    let link = fx.root.path().join(WAPP_ID).join("escape.svg");
    std::os::unix::fs::symlink(&target, &link).expect("plant symlink");

    let (status, _, body) = get(&fx.app, "/wapps/acme.bill-audit/escape.svg").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "符号链接逃逸必须 404");
    assert!(body.is_empty());
}

// ── t12：`?v=` 缓存破坏符携带与否同答 ──────────────────────────────────────

#[tokio::test]
async fn t12_cache_buster_query_param_does_not_change_the_answer() {
    let fx = wapps_static_fixture().await;

    let with_v = get(&fx.app, "/wapps/acme.bill-audit/index.html?v=1.0.0").await;
    let without = get(&fx.app, "/wapps/acme.bill-audit/index.html").await;

    assert_eq!(with_v.0, StatusCode::OK);
    assert_eq!(without.0, StatusCode::OK);
    assert_eq!(with_v.2, without.2, "体逐字节一致");
    for name in [
        &header::CONTENT_TYPE,
        &header::CONTENT_SECURITY_POLICY,
        &header::X_CONTENT_TYPE_OPTIONS,
        &header::CACHE_CONTROL,
    ] {
        assert_eq!(
            with_v.1.get(name),
            without.1.get(name),
            "带不带 ?v= 的 {name:?} 头必须一致"
        );
    }
}

// ── t13：POST 落 MethodRouter 默认 405 ──────────────────────────────────────

#[tokio::test]
async fn t13_post_to_wapps_face_is_405() {
    let fx = wapps_static_fixture().await;

    // 面只登记 get；写方法落 405（设计档 §3.1）。公开面无 L4 CSRF 语义
    // （非 /api/ 前缀），未认证 POST 也必须到达 MethodRouter 而非 401。
    let (status, _, _) = call(&fx.app, Method::POST, "/wapps/acme.bill-audit/index.html").await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}

// ── t14：403/404 应答也带 CSP + nosniff + no-cache（桌面对偶） ──────────────

#[tokio::test]
async fn t14_error_responses_carry_csp_nosniff_and_no_cache() {
    // 桌面 security_headers_are_always_injected（desktop/protocol.rs:502-535）
    // 的 HTTP 对偶：错误应答（空体）同样注入全套安全头。
    let fx = wapps_static_fixture().await;

    // 造一个 403 面：禁用 wapp（标记文件 + 重载注册表）。
    let marker = fx.root.path().join(WAPP_ID).join(ENABLED_MARKER_FILE);
    std::fs::remove_file(&marker).expect("disable marker");
    fx.manager.load_from_disk();

    for (uri, expected) in [
        ("/wapps/acme.ghost/index.html", StatusCode::NOT_FOUND), // 未知 id
        ("/wapps/acme.bill-audit/index.html", StatusCode::FORBIDDEN), // 禁用闸
        (
            "/wapps/acme.bill-audit/.storage.json",
            StatusCode::NOT_FOUND,
        ), // 隐藏分量
        ("/wapps/acme.bill-audit", StatusCode::NOT_FOUND),       // 空路径（parse 拒绝）
    ] {
        let (status, headers, body) = get(&fx.app, uri).await;
        assert_eq!(status, expected, "{uri}");
        assert!(body.is_empty(), "{uri}: 错误应答必须空体");
        let csp = header_value(&headers, &header::CONTENT_SECURITY_POLICY);
        assert_eq!(csp, WAPP_ASSET_CSP_FROZEN, "{uri}");
        assert!(!csp.contains("datazen:"), "{uri}");
        assert_eq!(
            header_value(&headers, &header::X_CONTENT_TYPE_OPTIONS),
            "nosniff",
            "{uri}"
        );
        assert_eq!(
            header_value(&headers, &header::CACHE_CONTROL),
            "no-cache",
            "{uri}"
        );
    }
}
