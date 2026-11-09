//! Shared workspace-app URI grammar, access decisions and package asset routing.
use crate::wapps::{is_valid_wapp_id, manager::WappManager};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteStatus {
    NotFound,
    Forbidden,
}
impl RouteStatus {
    pub const NOT_FOUND: Self = Self::NotFound;
    pub const FORBIDDEN: Self = Self::Forbidden;
}
/// Event emitted when a `open` deep link resolves to a contributed page.
/// Payload: `{ wappId, pageId, params }`.
pub const WAPPS_OPEN_PAGE_EVENT: &str = "wapps:open-page";

/// Reserved first path segment marking a deep-link command (not an asset).
pub const OPEN_COMMAND: &str = "open";

/// WR-F4 web 形态的资产路径前缀（origin 相对形）：每条 web 资产 URL 都是
/// `{origin}/wapps/{wapp id}/{package-relative path}`。与前端
/// `wappAssetOrigin.ts:18` 的 `WAPP_ASSET_PATH_PREFIX` 逐字节相同，由
/// [`parse_wapp_origin_path`] 作为唯一消费方（server 路由登记形状见
/// w8-knife4 设计档 §3.1）。
const WAPP_ASSET_PATH_PREFIX: &str = "/wapps/";

// Explicit scheme sources alongside `'self'`: WebKit (macOS) serves
// custom-scheme documents from an opaque origin and does not match `'self'`
// against `datazen://` subresources, which blocked the page's own scripts and
// images (BUG-F9-04) — mirrors VSCode webviews enumerating the resource scheme
// explicitly in CSP instead of trusting `'self'`. `'self'` is kept because
// Windows/WebView2 maps the scheme to `http(s)://datazen.<host>/…`, where only
// `'self'` matches. `connect-src 'none'` keeps plugins offline; `data:` stays
// image-only for inline SVG data URLs.
pub const ASSET_CSP: &str = "default-src 'self' datazen:; script-src 'self' datazen:; \
style-src 'self' datazen: 'unsafe-inline'; img-src 'self' datazen: data:; \
font-src 'self' datazen:; connect-src 'none'";

/// WR-F4 web 形态资产面的单源 CSP（wapp 资产 origin 静态路由逐应答注入，
/// W8 刀4）。与桌面 [`ASSET_CSP`] 同指令集、每条源列表去掉全部
/// ` datazen:` 源：web 形态资产 URL 是真实 https origin，`'self'` 有明确
/// 匹配物，无需点名自定义 scheme（桌面点名 `datazen:` 的 WebKit
/// opaque-origin 根因见上方 [`ASSET_CSP`] 注释，BUG-F9-04）。与前端
/// `packages/frontend/src/lib/wappAssetOrigin.ts:30-32` 的 `WAPP_ASSET_CSP`
/// **逐字节互钉**（跨语言漂移由两侧单测同时钉死）；机械推导关系
/// （`ASSET_CSP` 全文移除 ` datazen:`）见设计档 §5.2
/// （`docs/development/w8-knife4-wapp-static-design.md`）与本文件单测
/// `origin_asset_csp_is_asset_csp_minus_every_datazen_source`。
pub const ORIGIN_ASSET_CSP: &str = "default-src 'self'; script-src 'self'; \
style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; \
connect-src 'none'";

/// Decoded query parameters keyed by parameter name.
pub type QueryMap = BTreeMap<String, String>;

/// Result of routing a parsed request against the wapp registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatazenOutcome {
    /// Serve package file bytes with a Content-Type from the safe table.
    Asset {
        content_type: &'static str,
        bytes: Vec<u8>,
    },
    /// Emit [`WAPPS_OPEN_PAGE_EVENT`] and answer 200 with an empty body.
    OpenPage {
        wapp_id: String,
        page_id: String,
        /// Remaining query entries (`page` excluded) forwarded verbatim.
        params: Map<String, Value>,
    },
}

// ---------------------------------------------------------------------------
// URI parsing (pure)
// ---------------------------------------------------------------------------

/// Parse a raw `datazen://...` / `http(s)://datazen./...` request target into
/// `(wapp id, relative path-or-command, decoded query map)`.
///
/// Percent-escapes are decoded *before* any component validation so encoded
/// traversal sequences cannot slip past later checks.
pub fn parse_datazen_uri(uri: &str) -> Result<(String, String, QueryMap), String> {
    let without_scheme = strip_scheme(uri).ok_or_else(|| format!("unsupported scheme: {uri}"))?;
    split_host_path_query(without_scheme)
}

/// Parse `/wapps/<id>/<path>?<query>` (WR-F4 origin-relative asset form).
/// 与 `parse_datazen_uri` 共用 host/path/query 切分与解码内核；
/// 前缀必须逐字是 `/wapps/`，其余 Err（错误文案与前端
/// `parseWappAssetTarget` 同词，便于跨仓 grep 对账）。
///
/// 与 scheme 形的语义差异：解码后的路径安全（空路径 / 隐藏分量 / 穿越 /
/// 反斜杠 / NUL）在 parse 期即拒——对齐前端 `parseWappAssetTarget` 把
/// `isSafeWappAssetPath` 折进 parse 的拒绝语义（wappAssetOrigin.ts:104-128），
/// scheme 形则把空路径与不安全路径留给 route 层答 404。只收 origin 相对形：
/// 绝对 URL 形是前端 SPA 的输入面，服务端 request-target 天然是路径形
/// （w8-knife4 设计档 §3.2 记录的不对称）。
pub fn parse_wapp_origin_path(target: &str) -> Result<(String, String, QueryMap), String> {
    let rest = target
        .strip_prefix(WAPP_ASSET_PATH_PREFIX)
        .ok_or_else(|| format!("unsupported wapp asset path: {target}"))?;
    let (host, path, query) = split_host_path_query(rest)?;
    // Parse-time path safety (frontend parity): the single kernel table
    // (`safe_relative_path`) decides — no second grammar. The message keeps
    // the frontend wording so cross-repo grepping lines up.
    safe_relative_path(&path).map_err(|_| format!("unsafe path component in `{path}`"))?;
    Ok((host, path, query))
}

/// Shared parsing core of both entry forms: given `<host>/<path>?<query>`
/// with the scheme or `/wapps/` prefix already stripped, split off the query
/// *before* any decoding, split host from path, validate the host as a wapp
/// id, then lenient-decode the path and parse the query. Path safety is the
/// caller's stage (scheme form defers it to routing, origin form gates in
/// [`parse_wapp_origin_path`]).
fn split_host_path_query(rest: &str) -> Result<(String, String, QueryMap), String> {
    // Split off the query string before any decoding.
    let (host_and_path, raw_query) = match rest.split_once('?') {
        Some((hp, q)) => (hp, Some(q)),
        None => (rest, None),
    };

    let (host, raw_path) = match host_and_path.split_once('/') {
        Some((h, p)) => (h, p),
        None => (host_and_path, ""),
    };

    // Host must be a valid `<publisher>.<name>` wapp id; this rejects hosts
    // without the publisher dot, uppercase ids and overlong segments.
    if !is_valid_wapp_id(host) {
        return Err(format!("invalid wapp host: `{host}`"));
    }

    let path = percent_decode(raw_path);
    let query = parse_query(raw_query);

    Ok((host.to_string(), path, query))
}

/// Strip one of the accepted scheme prefixes, returning `<host>/<path>?<query>`.
fn strip_scheme(uri: &str) -> Option<&str> {
    let lower = uri.to_ascii_lowercase();
    if lower.starts_with("datazen://") {
        return uri.get("datazen://".len()..);
    }
    // Windows WebView2 maps custom schemes to http(s) with a dotted host:
    // `http://datazen./acme.bill-audit/index.html`. The `/` separator is
    // mandatory (BUG-F2-01): spec §2.4 only defines
    // `http://datazen./<host>/<path>`, so `datazen.` followed directly by the
    // wapp id is not an alias and must fail as an unsupported scheme.
    for prefix in ["https://datazen.", "http://datazen."] {
        if lower.starts_with(prefix) {
            return uri.get(prefix.len()..)?.strip_prefix('/');
        }
    }
    None
}

/// Parse a raw query string into a decoded map. Duplicate keys: last wins,
/// missing values decode to the empty string.
fn parse_query(raw: Option<&str>) -> QueryMap {
    let mut map = QueryMap::new();
    let Some(raw) = raw else {
        return map;
    };
    for pair in raw.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        map.insert(decode_query_component(key), decode_query_component(value));
    }
    map
}

/// Decode one query component: `+` means space (form encoding), then
/// percent-escapes are resolved.
fn decode_query_component(input: &str) -> String {
    percent_decode(&input.replace('+', " "))
}

/// Lenient percent-decoding: invalid escapes are kept literally, non-UTF-8
/// sequences are replaced lossily. Paths/queries that survive validation only
/// ever contain benign characters anyway.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(hi * 16 + lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Path safety + MIME (pure)
// ---------------------------------------------------------------------------

/// Validate a *decoded* relative path from a datazen URI: every `/`-separated
/// component must be non-empty, must not start with `.` (rejects `.storage.json`,
/// `.enabled` and any hidden component, including `.` / `..`), and the whole
/// string must not contain backslashes or NUL bytes.
pub fn safe_relative_path(path: &str) -> Result<PathBuf, String> {
    if path.is_empty() {
        return Err("empty path".into());
    }
    if path.contains('\\') {
        return Err(format!("backslash not allowed: {path}"));
    }
    if path.contains('\0') {
        return Err(format!("NUL byte not allowed: {path}"));
    }
    let mut rel = PathBuf::new();
    for component in path.split('/') {
        if component.is_empty() || component.starts_with('.') {
            return Err(format!("unsafe path component in `{path}`"));
        }
        rel.push(component);
    }
    Ok(rel)
}

/// Safe Content-Type table (§2.4): extensions outside the package whitelist
/// yield `None`, which callers must turn into 404 — never guess a type.
pub fn content_type_for(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "html" => Some("text/html"),
        "js" | "mjs" => Some("text/javascript"),
        "css" => Some("text/css"),
        "json" => Some("application/json"),
        "svg" => Some("image/svg+xml"),
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        "woff2" => Some("font/woff2"),
        "woff" => Some("font/woff"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Routing (pure against the registry; testable without a Tauri app)
// ---------------------------------------------------------------------------

/// Route `(wapp id, path, query)` through the registry: existence → enabled
/// → command/asset dispatch. Errors are bare status codes (404 / 403) so the
/// HTTP layer can respond without leaking details.
pub fn route_datazen_request(
    manager: &WappManager,
    wapp_id: &str,
    path: &str,
    query: &QueryMap,
) -> Result<DatazenOutcome, RouteStatus> {
    let Some(loaded) = manager.get(wapp_id) else {
        return Err(RouteStatus::NOT_FOUND);
    };
    if !loaded.enabled {
        return Err(RouteStatus::FORBIDDEN);
    }

    let mut segments = path.split('/');
    let first = segments.next().unwrap_or("");
    if first == OPEN_COMMAND {
        if segments.next().is_some() {
            return Err(RouteStatus::NOT_FOUND);
        }
        let page_id = query.get("page").ok_or(RouteStatus::NOT_FOUND)?;
        loaded
            .manifest
            .contributes
            .pages
            .iter()
            .find(|page| page.id == *page_id)
            .ok_or(RouteStatus::NOT_FOUND)?;

        let params: Map<String, Value> = query
            .iter()
            .filter(|(key, _)| key.as_str() != "page")
            .map(|(key, value)| (key.clone(), Value::String(value.clone())))
            .collect();

        return Ok(DatazenOutcome::OpenPage {
            wapp_id: wapp_id.to_string(),
            page_id: page_id.clone(),
            params,
        });
    }

    // Asset form: validate, whitelist the extension, then read the file.
    let rel = safe_relative_path(path).map_err(|_| RouteStatus::NOT_FOUND)?;
    let ext = rel
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default();
    let content_type = content_type_for(ext).ok_or(RouteStatus::NOT_FOUND)?;

    let wapp_dir = manager.wapp_dir(wapp_id);
    let file_path = wapp_dir.join(&rel);
    if !file_path.is_file() {
        return Err(RouteStatus::NOT_FOUND);
    }
    ensure_within_wapp_dir(&wapp_dir, &file_path)?;

    let bytes = fs::read(&file_path).map_err(|_| RouteStatus::NOT_FOUND)?;
    Ok(DatazenOutcome::Asset {
        content_type,
        bytes,
    })
}

/// Symlink-swap defense: resolve both directories and require containment.
fn ensure_within_wapp_dir(wapp_dir: &Path, file_path: &Path) -> Result<(), RouteStatus> {
    let canonical_dir = fs::canonicalize(wapp_dir).map_err(|_| RouteStatus::NOT_FOUND)?;
    let canonical_file = fs::canonicalize(file_path).map_err(|_| RouteStatus::NOT_FOUND)?;
    if canonical_file.starts_with(&canonical_dir) {
        Ok(())
    } else {
        Err(RouteStatus::NOT_FOUND)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- origin 形解析：`parse_datazen_uri` 既有用例的孪生（设计档 §9.1）------

    #[test]
    fn origin_form_parses_simple_asset() {
        let (id, path, query) =
            parse_wapp_origin_path("/wapps/acme.bill-audit/index.html").unwrap();
        assert_eq!(id, "acme.bill-audit");
        assert_eq!(path, "index.html");
        assert!(query.is_empty());
    }

    #[test]
    fn origin_form_decodes_percent_escapes_and_query_params() {
        let (_, _, query) = parse_wapp_origin_path(
            "/wapps/acme.bill-audit/open?page=quota-check&uid=123&q=a%20b&flag&plus=z+y",
        )
        .unwrap();
        assert_eq!(query.get("page").map(String::as_str), Some("quota-check"));
        assert_eq!(query.get("uid").map(String::as_str), Some("123"));
        assert_eq!(query.get("q").map(String::as_str), Some("a b"));
        assert_eq!(query.get("flag").map(String::as_str), Some(""));
        assert_eq!(query.get("plus").map(String::as_str), Some("z y"));

        let (_, path, _) =
            parse_wapp_origin_path("/wapps/acme.bill-audit/assets%2Ficon.svg").unwrap();
        assert_eq!(path, "assets/icon.svg");
    }

    #[test]
    fn origin_form_parses_open_deep_link() {
        // 冻结 TS 用例（wappAssetOrigin.test.ts:68-71）的内核孪生：open 形
        // 解析回 (id, "open", {page, …})，首段命令交 route 层裁决。
        let (id, path, query) =
            parse_wapp_origin_path("/wapps/acme.bill-audit/open?page=quota-check&uid=123").unwrap();
        assert_eq!(id, "acme.bill-audit");
        assert_eq!(path, "open");
        assert_eq!(query.get("page").map(String::as_str), Some("quota-check"));
        assert_eq!(query.get("uid").map(String::as_str), Some("123"));

        let (_, path, query) =
            parse_wapp_origin_path("/wapps/acme.bill-audit/open?page=home").unwrap();
        assert_eq!(path, "open");
        assert_eq!(query.get("page").map(String::as_str), Some("home"));
    }

    #[test]
    fn origin_form_lenient_decode_serves_encoded_slash() {
        // 宽松解码钉（冻结 TS 用例 wappAssetOrigin.test.ts:74-76 的内核
        // 半边）：`assets%2Ficon.svg` 解码为 `assets/icon.svg` 后照常通过。
        let (id, path, query) =
            parse_wapp_origin_path("/wapps/acme.bill-audit/assets%2Ficon.svg?q=a%20b").unwrap();
        assert_eq!(id, "acme.bill-audit");
        assert_eq!(path, "assets/icon.svg");
        assert_eq!(query.get("q").map(String::as_str), Some("a b"));
    }

    #[test]
    fn origin_form_matches_scheme_form_on_the_shared_core() {
        // 两入口共用同一解码内核：同一 host/path/query 在两形下同解。
        let scheme = parse_datazen_uri("datazen://acme.bill-audit/assets/icon.svg?x=1").unwrap();
        assert_eq!(
            parse_wapp_origin_path("/wapps/acme.bill-audit/assets/icon.svg?x=1").unwrap(),
            scheme
        );
    }

    // -- 拒绝组：逐字抄冻结清单（wappAssetOrigin.test.ts:85-106）-------------

    #[test]
    fn origin_form_rejects_unknown_prefix() {
        let target = "/plugins/acme.bill-audit/index.html";
        let err = parse_wapp_origin_path(target).unwrap_err();
        assert!(
            err.starts_with("unsupported wapp asset path"),
            "`{target}` should be rejected, got: {err}"
        );
    }

    #[test]
    fn origin_form_rejects_invalid_hosts() {
        // host 不是合法 wapp id：缺 publisher 点号 / 大写 / 空段（冻结四例）。
        for host in ["bill-audit", "Acme.bill-audit", "acme.", ".bill"] {
            let err = parse_wapp_origin_path(&format!("/wapps/{host}/index.html")).unwrap_err();
            assert!(
                err.starts_with("invalid wapp host"),
                "`{host}` should be rejected, got: {err}"
            );
        }
    }

    #[test]
    fn origin_form_rejects_empty_path() {
        // 与 scheme 形的语义差异：`parse_datazen_uri` 空路径 parse Ok、由
        // route 层答 404（desktop `rejects_missing_path_but_keeps_host_check_in_route`）；
        // origin 形按冻结清单在 parse 期即拒。
        let err = parse_wapp_origin_path("/wapps/acme.bill-audit").unwrap_err();
        assert!(
            err.starts_with("unsafe path component"),
            "empty path should be rejected, got: {err}"
        );
    }

    #[test]
    fn origin_form_rejects_decoded_unsafe_paths() {
        // 解码后穿越 / 隐藏分量 / 反斜杠 / NUL——与 `safe_relative_path`
        // 同表（冻结清单六条，wappAssetOrigin.test.ts:97-106）。
        for evil in [
            "/wapps/acme.bill-audit/%2e%2e/%2e%2e/settings.json",
            "/wapps/acme.bill-audit/.storage.json",
            "/wapps/acme.bill-audit/.enabled",
            "/wapps/acme.bill-audit/dir/.secret.png",
            "/wapps/acme.bill-audit/a%5Cb.html",
            "/wapps/acme.bill-audit/%00.html",
        ] {
            let err = parse_wapp_origin_path(evil).unwrap_err();
            assert!(
                err.starts_with("unsafe path component"),
                "`{evil}` should be rejected, got: {err}"
            );
        }
    }

    #[test]
    fn origin_form_rejects_malformed_prefix_shapes() {
        // 设计档 §3.1/§3.3 钉死的路由形状：`/wapps`（前缀不完整）与
        // `/wapps/`（空 host）在 parse 期即拒，server 半边据此答 404、
        // 绝不落入 SPA fallback。
        assert!(parse_wapp_origin_path("/wapps").is_err());
        assert!(parse_wapp_origin_path("/wapps/").is_err());
    }

    #[test]
    fn origin_form_rejects_absolute_url_and_scheme_forms() {
        // 服务端只收 origin 相对形：绝对 URL 形是前端 SPA 的输入面
        // （wappAssetOrigin.ts:109-111），scheme 形归 `parse_datazen_uri`。
        for target in [
            "https://app.example.com/wapps/acme.one/data.json",
            "datazen://acme.bill-audit/index.html",
        ] {
            assert!(
                parse_wapp_origin_path(target).is_err(),
                "`{target}` should be rejected as a non-origin form"
            );
        }
    }

    // -- CSP 推导（设计档 §5.2 / §9.1）---------------------------------------

    #[test]
    fn origin_asset_csp_is_asset_csp_minus_every_datazen_source() {
        // 机械推导：与桌面 `ASSET_CSP` 同指令集，每条源列表去掉 ` datazen:`。
        assert_eq!(ORIGIN_ASSET_CSP, ASSET_CSP.replace(" datazen:", ""));
    }

    #[test]
    fn origin_asset_csp_is_byte_identical_to_frontend_frozen_literal() {
        // 前端冻结字面量原文（wappAssetOrigin.ts:30-32 的 `WAPP_ASSET_CSP`
        // 两段拼接）逐字节互钉——跨语言漂移在此立即可见。
        let frontend_frozen = concat!(
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; ",
            "img-src 'self' data:; font-src 'self'; connect-src 'none'"
        );
        assert_eq!(ORIGIN_ASSET_CSP, frontend_frozen);
    }
}
