# W8 刀4：Wapp 资产 origin 静态路由实施级设计

> **设计日期**：2026-11-09。**基线**：main 工作树（本文所有断言按当前文件行号逐处核实；设计任务不跑 git，不记 hash）。**输入**：WR-F4 冻结的 web 资产约定（`packages/web/src/platform/web/__tests__/wappAssetOrigin.test.ts` + `packages/frontend/src/lib/wappAssetOrigin.ts`）、wapp 内核架构（`docs/architecture/backend/wapps.md`）、HTTP 契约面（`docs/architecture/platform/http-api-contract.md`）。
> **范围**：web 形态下 wapp 沙箱 iframe 资产服务的 **server 半边**——把 `{origin}/wapps/{wapp id}/{package-relative path}?{query}` 这条冻结 URL 约定变成 datazen-server 上真正伺服文件的 HTTP 静态面。桌面 `datazen://` 协议桥（`crates/datazen-desktop/src/wapps/protocol.rs`）零改动。
> **性质**：纯设计文档，零产品代码。本文每个断言指回真实 `file:line`。
> **验证基线**：`cargo test -p datazen --lib`（内核 origin 形入口）+ `cargo test -p datazen-server`（新静态面套件）；web 侧既有 `wappAssetOrigin.test.ts` 保持原样双绿。

---

## 0. 排刀位置

W8 队列的收尾刀。前置事实链：

| 件 | 状态 | 位置 |
| --- | --- | --- |
| 资产 URL 约定（web 形） | **已冻结（WR-F4）** | `packages/frontend/src/lib/wappAssetOrigin.ts:18`（`WAPP_ASSET_PATH_PREFIX = '/wapps/'`）、`:108-128`（`parseWappAssetTarget`） |
| web 装配绑定 | 已落地 | `packages/web/src/platform/web/assembly.ts:183-187`（首帧前 `configureWappAssetResolver(WEB_WAPP_ASSET_RESOLVER)`） |
| 单源 CSP 字面量 | 已冻结 | `wappAssetOrigin.ts:30-32`（`WAPP_ASSET_CSP`） |
| 内核路径安全 / MIME / 路由 | 已下沉共享层 | `crates/datazen/src/wapps/protocol.rs:170-286` |
| 内核存储根参数化 | 已就位 | `crates/datazen/src/wapps/mod.rs:8-11`（所有入口收 `wapps_dir` 入参） |
| 桌面协议宿主（参照物） | 已在跑 | `crates/datazen-desktop/src/wapps/protocol.rs:55-128` |
| **server 半边** | **缺** | datazen-server 无任何 `/wapps` 路由；`packages/web/src/platform/web/webUnsupportedMethods.ts:148-220` 仍把全部 wapp 命令列为 web 不支持 |

本刀补最后一行。

---

## 1. 现状盘点（代码事实）

### 1.1 冻结的 web 侧约定（WR-F4，逐字在案）

- **URL 形**：`{origin}/wapps/{wapp id}/{package-relative path}?{query}`。入口带 `?v=<manifest version>` 缓存破坏符，贡献图标不带（`wappAssetOrigin.test.ts:30-46`）。
- **文法与拒绝语义与桌面 `parse_datazen_uri` 同构**（`crates/datazen/src/wapps/protocol.rs:64-88`）：
  - wapp id 文法 `^[a-z0-9][a-z0-9-]{0,30}\.[a-z][a-z0-9-]{1,31}$`（内核 `manifest.rs:23-27` 的 `WAPP_ID_RE`；TS 镜像 `wappAssetOrigin.ts:38`）。缺 publisher 点号 / 大写 / 空段一律拒绝（`wappAssetOrigin.test.ts:89-93`）。
  - **percent-decode 宽松语义**：非法转义保留原文、非 UTF-8 有损替换，**校验发生在解码之后**——`assets%2Ficon.svg` 解码为 `assets/icon.svg` 后照常服务（`wappAssetOrigin.test.ts:74-76`，内核 `protocol.rs:135-151` 同款）。
  - 解码后路径安全（隐藏分量 / 穿越 / 反斜杠 / NUL）与桌面 `safe_relative_path` 同表（`protocol.rs:170-188`；冻结用例清单 `wappAssetOrigin.test.ts:96-106`）。
  - **解析失败即 404，绝不静默**（`wappAssetOrigin.ts:104-106` 的模块注释）。
- **CSP 单源化**：`WAPP_ASSET_CSP = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'none'"`（`wappAssetOrigin.ts:30-32`）——与桌面 `ASSET_CSP`（`protocol.rs:31-33`）**同指令集、只删 `datazen:` 源**（`wappAssetOrigin.test.ts:115-131` 逐指令断言）。

### 1.2 内核能力面（server 可直接复用的单家实现）

| 能力 | 内核入口 | 备注 |
| --- | --- | --- |
| URI 解析（scheme 形） | `parse_datazen_uri(uri) -> (id, path, QueryMap)`（`protocol.rs:64-88`） | 只收 `datazen://` 与 Windows 映射形（`:91-107`）；**没有 origin 路径形入口** |
| 路径安全 | `safe_relative_path(&str) -> Result<PathBuf>`（`:170-188`） | 空路径 / 反斜杠 / NUL / 空分量 / `.` 开头分量全拒 |
| MIME 白名单 | `content_type_for(ext) -> Option<&'static str>`（`:192-205`） | 未知扩展 → `None` → 调用方必须 404，**绝不猜类型** |
| 路由判定 | `route_datazen_request(&WappManager, id, path, query) -> Result<DatazenOutcome, RouteStatus>`（`:214-275`） | 存在 → **启用闸（disabled ⇒ `Forbidden`）** → open 深链 / 资产分派；错误是裸状态码（404/403），HTTP 层不泄露细节 |
| 符号链接防换 | `ensure_within_wapp_dir`（`:278-286`） | canonicalize 双侧包含性核对 |
| 桌面 CSP | `pub const ASSET_CSP`（`:31-33`） | 双源（`'self' datazen:`） |
| 注册表 | `WappManager`（`manager.rs:18-193`） | 构造收 `wapps_dir`（`:24-29`）；`load_from_disk` 扫描并跳过非法包（`:51-89`）；启用态 = 包目录内 `.enabled` 标记文件（`:7`，`:74-78`） |

### 1.3 桌面协议宿主——HTTP 对偶的逐字段参照物

`crates/datazen-desktop/src/wapps/protocol.rs`：

- 响应头**每一条都注入** CSP + `X-Content-Type-Options: nosniff` + `Cache-Control: no-cache`，含 403/404 错误应答；错误应答空体（模块头 `:16-17`，`datazen_response` `:112-128`，钉子测试 `:502-535`）。
- 禁用 wapp ⇒ 403，未知 id / 非法扩展 / 隐藏文件 ⇒ 404（`:407-469`）。
- `open` 深链 ⇒ 发宿主事件 + 200 空体（`:76-79`，`emit_open_page` `:92-109`）。
- 桌面装配：`crates/datazen-desktop/src/bootstrap/app_state.rs:163-170`——`{appData}/wapps` 作 `wapps_dir`，`WappManager::new(...)` + `load_from_disk()`，非法包 warn 跳过不阻断启动。

### 1.4 server 侧现状

- 路由装配：`build_router_with_auth`（`crates/datazen-server/src/http/router.rs:131-247`）内 merge 域路由片段（`:211-217` 的 `domain_routers` 循环）与 `/mcp`（`:206-210`）；层链 L0/L1 全路径包裹，L2/L3/L4/L5/L6 用 `under_api`（`/api/` 前缀，`middleware/mod.rs:66-68`）自跳过——**模块头 `:62-65` 明文「未来的静态资源面只到 L0」**。
- 静态面先例：WR-E5 把 web 构建产物挂 `ServeDir` fallback（`bootstrap.rs:987-1007`），根 `DATAZEN_SERVER_WEB_ROOT`，未设 = 不挂（`config.rs:88-90,278-283`）。
- 非 `/api/` 服务面先例：`/mcp`（`mcp/http.rs:348-354`）挂在认证链内、自做会话校验（`SessionAuthenticator` 端口 `:44-46`；服务端 `CookieSessionAuthenticator` 用 `LoginSessions::peek`，`bootstrap.rs:1037-1068`）。
- 错误兜底：`api_or_not_found`（`router.rs:266-282`）——非 `/api/` 未知路径 404；**但 web_root 设置后该 fallback 被 `ServeDir` 替换**（`bootstrap.rs:1001-1004`），未匹配路由的 `/wapps/*` 形状会落进 SPA shell（见 §3.3 的钉法）。
- L5 限流对非 `/api/` 路径跳过（`middleware/ratelimit.rs:261-266`）；IP 键依赖 `ConnectInfo`，装配点**当前没接** `into_make_service_with_connect_info`（`bootstrap.rs:1018` 直接 `axum::serve(listener, app)`；`ratelimit.rs:8-9` 自己注明「装配点经 into_make_service_with_connect_info 提供；不可用时归并桶」）。

---

## 2. 范围裁决：本刀只做「读面静态伺服」

**裁决：刀4 = 静态资产面（server crate 单仓改动 + 两个纯函数级内核增项 + 一个 env 键）。清单读（`list_wapps` / `get_wapp_manifest`）与 install/remove/enable 管理面全部不随刀。**

依据（按分量排序）：

1. **契约增长规则把「服务端先行、方法后补」封死**。`http-api-contract.md` §2（`:22`）：「先在 backend-client 落域客户端方法，再在此表登记镜像端点。反向（先开端点后补方法）只允许出现在已立项的服务自举路由上（§6.5 的 uploads 是唯一在账者）」，§9.2（`:515`）的「多余端点为零」验收同口径。`list_wapps` / `get_wapp_manifest` 是域面 JSON 读（前端 `commands/wapps.ts:17-19` 经 invoke 消费），上 HTTP 就必须有域客户端方法背书——要么连 `@datazen/backend-client` + web 分发 RouteSpec + 前端命令切换 + `webUnsupportedMethods` 收缩一起做（跨三包的 web 半边刀），要么不做。**没有「server 先开两条裸端点」的合法中间态。**
2. **W8 的既有纪律就是两半分刀**。契约 §11 变更记录里 v1.4→v1.9 每个域都是「服务端半边本刀、web 半边随下刀收口」（queryAssets v1.4→v1.5.1、schemaDiff v1.5→v1.5.1、workflow v1.6、dashboard v1.7、dataSync v1.8/v1.9 尾注）。本刀保持同构：server 静态面先行，`wapps` 域客户端（list/manifest 两方法）+ 名单收缩作紧随收口刀。
3. **静态面不是域面**。资产 URL 由浏览器原生资源加载（iframe `src`、`<img>`/`<script>`）直取，不经过 `MethodMap` / `WebClient` 传输——前端解析器直接拼路径（`wappAssetOrigin.ts:164-171`）。它天然无方法背书要求，挂载口径类比 `/mcp`（WR-F1 登记为「§6.6 之外的服务面」，契约 §11 v1.2）而非 §6 域表。**刀4 因此不触碰契约的域面账目。**
4. **install/remove/`set_wapp_enabled`/storage/bridge 命令明确不随**：
   - `inspect_wapp_package_with_dialog` / `install_wapp` 是原生文件对话框 + pickToken 语义（`commands/wapps.ts:32-43`）——契约 §8「平台服务永不上 HTTP」的直接先例（`http-api-contract.md:507`）。服务端安装面的正确形状是受控上传（§6.5 在账端点 `POST /api/v1/artifacts/uploads` 的 envelope 天然适配 ZIP 分块上传），涉及认证、配额、病毒式路径扫描与审计面，**另立项**。
   - `remove` / `set_wapp_enabled` 是管理写面：没有安装面就没有管理面的服务端语义（包从哪来）。`wapp_storage_*` / `execute_wapp_driver_command` / `wapp_session` 是桥接面，web 形态归 wapp 桥接刀（postMessage 桥本身形态中立，`wapps.md:66-78`）。
   - v1 服务端 wapps 目录的填充方式 = **运维侧落盘**（部署卷 + 重启扫描生效），与桌面 `{appData}/wapps` 同构（`app_state.rs:163-170`）。热加载/重载端点进开放问题（§10 O6）。

> **对父 agent 倾向的偏离说明**：任务简报倾向「静态伺服 + 必要的资产清单读」。本设计基于上述契约硬约束（依据 1）把清单读移出本刀——它不是「捎带两条读端点」的事，而是完整的域客户端半边。若父 agent 坚持随刀，范围立即扩为三包改动（见 §10 O1 的两案对比）。

---

## 3. 路由形状与挂载

### 3.1 路由文法（与 web resolver 测试逐字对齐）

```
GET /wapps/{wapp id}/{package-relative path}?{query}
```

axum 0.8（`Cargo.toml:64`）路由登记（两条，钉死整个前缀）：

```rust
Router::new()
    .route("/wapps", get(handler_wapp_asset))        // 空形：parse 拒绝 → 404
    .route("/wapps/{*rest}", get(handler_wapp_asset)) // 其余一切 /wapps/… 形状
    .with_state(state)
```

- **不使用 axum `Path` 提取器**——它自带 percent-decode 语义，与内核宽松解码（非法转义保留、`+` 在路径里不是空格）不保证一致，且 `{*rest}` 通配下 `%2F` 的处理跨版本有歧义。handler 直收 `uri: Uri`，取 `uri.path()`（http crate 原样未解码）+ `uri.query()`，**原文交内核解析**。冻结用例 `assets%2Ficon.svg` 必须照常服务（`wappAssetOrigin.test.ts:74-76`），这是唯一能保证它的形状。
- 只登记 `get`；POST/PUT 等落 MethodRouter 默认 405。本面无 HEAD 消费方，HEAD 行为随 axum 默认，不作验收项。

### 3.2 内核增项 1：origin 形解析入口（单一文法所有者）

`crates/datazen/src/wapps/protocol.rs` 新增：

```rust
/// Parse `/wapps/<id>/<path>?<query>` (WR-F4 origin-relative asset form).
/// 与 `parse_datazen_uri` 共用 host/path/query 切分与解码内核；
/// 前缀必须逐字是 `/wapps/`，其余 Err（错误文案与前端
/// `parseWappAssetTarget` 同词，便于跨仓 grep 对账）。
pub fn parse_wapp_origin_path(target: &str)
    -> Result<(String, String, QueryMap), String>
```

实现 = 前缀剥离（`/wapps/` 逐字，`None ⇒ "unsupported wapp asset path: …"`）+ 复用既有 `(host, raw_path, raw_query)` 切分、`is_valid_wapp_id`、`percent_decode`、`parse_query`（`protocol.rs:68-151` 的私有件在本文件内共享，不复制）。**不新增任何依赖**——纯字符串解析，F-14 边界检查（`scripts/check-platform-crate-boundaries.mjs:231`）不受影响。（编号漂移备忘：`wapps.md:37` 记 F-14、内核 `mod.rs:18` 记 F-12，两处引用同一脚本同一断言，顺带修文档不属本刀。）

前端 `parseWappAssetTarget` 额外收绝对 URL 形（`wappAssetOrigin.ts:109-111`）——那是 SPA 深链/链接解析的需要；**服务端永远只收 origin 相对形**（HTTP 请求行的 request-target 就是路径形），两侧输入面的这一不对称是形态事实，记录即可。

### 3.3 handler 流程与 404 钉法

```
uri → parse_wapp_origin_path
  Err  → 404（空体；含未知前缀/非法 id 文法/空路径/解码后不安全路径——全部同答 404，
          对齐「解析失败即 404，绝不静默」与桌面 parse-Err 分支 desktop/protocol.rs:65-69）
  Ok(id, path, query) → route_datazen_request(&manager, …)
      Ok(Asset { content_type, bytes }) → 200 + §5 的头集
      Ok(OpenPage { .. })              → 204（空体；见下）
      Err(NotFound)                    → 404（空体）
      Err(Forbidden)                   → 403（空体；禁用 wapp，桌面 :407-425 同语义）
```

- **`/wapps` 与 `/wapps/{*rest}` 两条路由吃掉整个前缀**，任何畸形形状（`/wapps`、`/wapps/`、`/wapps/acme.bill-audit` 空路径）都由本 handler 答 404——**绝不落入 fallback**。这是对 WR-E5 的防御：web_root 设置后 fallback 是 `ServeDir` + SPA `index.html` 兜底（`bootstrap.rs:1001-1004`），不钉住的话畸形 `/wapps/*` 会把整个 SPA 壳 200 进沙箱 iframe。
- **open 深链 ⇒ 204**。桌面的 200 空体 + 宿主事件（`desktop/protocol.rs:76-79`）在 web 形态没有事件通道（宿主是浏览器里的 SPA，不是协议处理器）；本刀不发明 SPA 回跳路由。web 真正的深链消费方是前端 `parseWappAssetTarget`（`wappAssetOrigin.test.ts:68-71` 把 open 形解析回 `(id, "open", {page, …})`），路由归属 wapp web 半边刀（§10 O6/O7 备注）。204 表示「受理、无表示」；备选 404（不受理）在内核 `route_datazen_request` 单家形状下成本相同，裁决取 204 是为了保留「open 是合法首段」的语义可见性。
- `?v=` 缓存破坏符：查询串进 `QueryMap` 后路由不消费（`protocol.rs:214-275` 全程不看 `v`）——携带与否同答。缓存策略见 §5.4。

### 3.4 挂载点（mcp 先例）

- `AuthDeps`（`router.rs:73-98`）新增可选槽：`pub wapp_assets: Option<crate::http::wapps_static::WappStaticState>`；`build_router_with_auth` 内 `if let Some(state) = deps.wapp_assets { router = router.merge(wapps_static_router(state)); }`——与 `mcp` 槽（`:206-210`）同款：`None = 不挂`，部署/测试的显式最小形态。
- 新文件 `crates/datazen-server/src/http/wapps_static.rs`（与 `sse.rs` 同层——非域面服务面直挂 `http/`，不进 `domains/`；`http/mod.rs` 加一行 `pub mod wapps_static;`）。
- 层链效果：L0 requestId / L1a/L1b 体闸 / catch-panic 全路径生效（它们不看路径）；L2/L3/L4/L6 经 `under_api` 自跳过（`middleware/mod.rs:66-68`——「静态资源面只到 L0」是该层既定语义）；L5 见 §6.3。
- `build_router()`（D1 骨架，`tests/skeleton.rs` 钉住不得改动）**不挂本面**——骨架构建下 `/wapps/*` 走 `api_or_not_found` 404，与「未装配 = 显式形态」一致。

---

## 4. 存储根与装配

### 4.1 env 键（对照既有键风格）

| 项 | 裁决 | 依据 |
| --- | --- | --- |
| 键名 | **`DATAZEN_SERVER_WAPPS_DIR`** | 仓内全族是 `DATAZEN_SERVER_*`（`config.rs:99-126` 逐键；`env.example` 头部「与 config.rs 的键逐一对齐」纪律）。任务简报示例 `DZ_WAPPS_DIR` 不合族，不采用 |
| 类型 | `PathBuf` + 默认值，**不是 `Option`** | 对照 `artifact_root`（`config.rs:77-78,148-149`，默认 `"data/artifacts"` 相对 cwd）。`web_root` 用 `Option` 是因为有「独立 origin/CDN 承载」的替代部署形；WR-F4 冻结了同源路径约定，wapps 面没有替代形，空目录 = 全 404 即天然的「无 wapps」形态，不需要第二种配置语义 |
| 默认值 | `DEFAULT_WAPPS_DIR: &str = "data/wapps"` | 与 `data/artifacts` 同族同缀；生产由 compose 注入卷内绝对路径（`DATAZEN_SERVER_DB` / `ARTIFACT_ROOT` 同款纪律，`env.example:14-16,49-51`） |
| env.example | 新增分节注释 + 键（`deploy/config/env.example` 尾部），头部键计数随 +1 对齐 | 该文件是「WR-G3 全键对齐」的锚 |

### 4.2 装配（composition root）

`bootstrap.rs`（`AuthDeps` 构造点 `:974-985` 附近）：

```rust
// W8 刀4：wapp 资产静态面。目录缺失则建（空面全 404）；非法包 warn
// 跳过不阻断启动（桌面 app_state.rs:163-170 同语义）。
std::fs::create_dir_all(&config.wapps_dir)?;
let wapp_manager = std::sync::Arc::new(
    datazen::workspace_apps::manager::WappManager::new(config.wapps_dir.clone()));
let wapp_count = wapp_manager.load_from_disk();
tracing::info!(wapps = wapp_count, "wapp asset face armed");
```

`WappStaticState`（`http/wapps_static.rs`）：

```rust
pub struct WappStaticState {
    pub manager: std::sync::Arc<WappManager>,  // 共享层注册表（启用闸/路由单家）
}
```

**与内核「存储根参数化」的对接**（`wapps.md:36`、内核 `mod.rs:8-11` 的既定设计）：server 适配器传 `config.wapps_dir`。本刀只消费 `WappManager`（读面：`load_from_disk` + `route_datazen_request` 内的 `wapp_dir`/`fs::read`）；`install_from_zip(zip, wapps_dir)` / `install_from_dir(src, wapps_dir)` / `storage_*(wapps_dir, id, key)`（`install.rs:30,46`、`wapps.md:36`）在安装面立项时由上传管线调用同一根——签名零改动，这正是内核下沉时预留的接缝。同步 IO 说明：`route_datazen_request` 内是 `fs::read`（`protocol.rs:270`），与仓内既有的 handler 内同步 SQLite（`login_sessions.rs:326-358` 的 `lock_for_adapter`）同级，不另起 `spawn_blocking`（≤50MB/包、单文件典型 KB 级，`manifest.rs:17`）；列为非目标。

---

## 5. 安全面

### 5.1 路径安全：**复用内核，server 零重写**

裁决依据：`route_datazen_request` 已闭环全部读面判定——存在性（`:220-222`）、启用闸（`:223-225`）、`safe_relative_path`（`:256`）、扩展白名单（`:257-261`，与打包期 `allowed_wapp_file_ext`（`manifest.rs:203-208`）同表）、`is_file` 目录拒绝（`:265-267`，天然无目录列表）、`ensure_within_wapp_dir` 符号链接防换（`:268,278-286`）。隐藏分量（`.storage.json` / `.enabled` / `dir/.secret.png`）由「分量不得 `.` 开头」拒绝（`:182-184`）。**server 侧任何第二份路径规则都是规则漂移面**——web 前端已经存在一份 TS 镜像（`wappAssetOrigin.ts:49-75`，跨语言镜像 + 测试互钉是 WR-F4 的既定纪律）；同仓再出第三份 Rust 镜像不可接受。唯一的新代码是 §3.2 的前缀剥离（复用文件内私有件）。

### 5.2 CSP 响应头：内核增项 2 + 逐字段对齐

`crates/datazen/src/wapps/protocol.rs` 新增常量（与 `ASSET_CSP` 相邻，同一文件内可 diff 出「只删 `datazen:` 源」）：

```rust
pub const ORIGIN_ASSET_CSP: &str = "default-src 'self'; script-src 'self'; \
style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; \
connect-src 'none'";
```

- **逐字段对齐**：与前端冻结字面量 `WAPP_ASSET_CSP`（`wappAssetOrigin.ts:30-32`）逐字节相同；与桌面 `ASSET_CSP`（`protocol.rs:31-33`）同指令集、每条源列表去掉 `datazen:`。内核单测断言「`ORIGIN_ASSET_CSP` == `ASSET_CSP` 全文移除 ` datazen:`」的机械推导关系；server 集成测试把前端字面量原文抄进断言（跨语言互钉，与 `wappAssetOrigin.test.ts:115-131` 对桌面字面量的做法对称）。
- **macOS WebKit 双源教训的裁剪说明**：桌面必须点名 `datazen:` 的根因是**自定义 scheme 的 URL origin 本身是 opaque origin**（URL 规范：非 special scheme ⇒ opaque；BUG-F9-04，`protocol.rs:23-30` 注释与 `wapps.md:63`）——`'self'` 无可匹配物，与 iframe 沙箱无关。web 形态资产 URL 是真实 https origin（tuple origin），`'self'` 有明确匹配物；`WappPageShell` 的 `sandbox="allow-scripts"`（`WappPageShell.tsx:220`）让**文档** origin 变 opaque，但 CSP source-expression 的 `'self'` 按受保护资源 URL 的 origin 求值——WR-F4 的冻结裁决（`wappAssetOrigin.ts:20-29` 注释）已采纳这一读法。因此单源成立，`datazen:` 源在 web 形态既无必要也无物可点。残余风险进 §10 O2。
- **不增发任何额外指令**（不加 `frame-ancestors`、不加 `X-Frame-Options`）：与冻结字面量的「指令集合不变」纪律一致（`wappAssetOrigin.ts:27-28`）；第三方面 iframe 嵌套的遏制靠 §6 的会话/边界裁决 + iframe 自身 sandbox。`frame-ancestors` 硬化需要连前端冻结字面量一起改，列 §10 O3 备选。

### 5.3 nosniff 与 MIME

- 每条应答（含 403/404）注入 `X-Content-Type-Options: nosniff`（桌面 `datazen_response` `:119-120` 的对偶）。
- `Content-Type` 只来自内核白名单表（`content_type_for`，`:192-205`）：html/js/mjs/css/json/svg/png/webp/woff2/woff；未知扩展 404，**绝不猜类型**（`:190-191` 的表注释是显式契约）。200 才带 `Content-Type`。

### 5.4 缓存策略

**每条应答 `Cache-Control: no-cache`（桌面逐字对齐，`desktop/protocol.rs:121`）。** 理由：

1. wapp 包在服务端是**可变内容**：同 id 重装走 `{id}.old.bak` 备份 + 原地替换（`install.rs:4-6` 流程注释），`?v=` 携带的是 manifest 版本号不是内容哈希——**同版本重装会换字节**，任何 `immutable`/长 `max-age` 假设都不成立。版本化长缓存（`max-age=31536000, immutable`）需要「重装必 bump 版本」的打包纪律背书，本刀不发明它（§10 O7）。
2. `no-cache` ≠ 不缓存：语义是「每次用前必须再验证」。桌面已付这个成本，静态微应用（≤50MB/≤2000 文件，`manifest.rs:17-19`）的量级可承受。
3. `?v=` 的作用因此收敛为：跨版本升级时 URL 天然分片（历史项/中间缓存不串版本），与桌面形保持 URL 逐字节同构（`wappAssetOrigin.test.ts:30-32`）。ETag/304 再验证优化（文件 len+mtime 弱校验）列为可选增强，不进最小刀。

---

## 6. 认证与授权

### 6.1 裁决：**v1 静态面公开（无会话门）**

这是本刀最重要的裁决，与任务简报的预期方向（「同源 cookie 自动携带」）相反，依据如下：

1. **同源 cookie 在本场景只对入口文档成立，对子资源大概率不成立**。`WappPageShell` 的 iframe 是 `sandbox="allow-scripts"`（**无** `allow-same-origin`，`WappPageShell.tsx:220`）——wapp 文档运行在 opaque origin。入口文档请求由宿主 SPA 发起（same-site ⇒ `SameSite=Lax` cookie 在场）；但**wapp 文档自己发起的子资源请求**（`<script src="main.js">`、`<img>`、`<link>`，全部是包内相对路径）的 initiator 是 opaque-origin 文档——按 cookie 规范的 site-for-cookies 求值，opaque initiator 不与任何 site 同站，`SameSite=Lax` 的 `__Host-dz_session`（`middleware/auth.rs:34-37`）会被扣住。会话门的结果是：入口 200、全部子资源 401，**每个 wapp 页面必坏**。
2. **桌面先例本就是零认证**：`handle_datazen_request`（`desktop/protocol.rs:55-88`）没有任何身份概念——OS 用户边界即边界，启用闸（403）是唯一的访问控制。wapp 字节在产品语义里是「静态 UI 资产」，不是机密（机密不进包是打包纪律；桥接面的机密在宿主侧，`system-overview.md:327`「Wapp 不能从 iframe 自行取得用户 cookie 或调用任意服务端 URL」——opaque sandbox 保证了这一点与 URL 同源性无关）。
3. **旧一代服务端设计同样把 wapp 面放在登录面外**：`team-server-and-auth.md:755`「Web Wapp 与主应用不同源且登录 cookie 为主应用 host-only」——独立 origin 形下 wapp 资产根本收不到登录 cookie。WR-F4 把 URL 形改为同源路径，但「wapp 资产不经登录 cookie」这一事实在两代设计里一致（见 §10 O4 的文档冲突项）。

**推论**：未登录访问 `/wapps/` 的应答与登录态完全相同（200/403/404），**本面不存在 401 分支**；不需要 `LoginSessions`、不需要 `CookieSessionAuthenticator`、不需要 org 闸——v1 单组织（`config.rs:45-48`）下面即宿主组织的 wapps，跨组织隔离是「多组织立项时必须先补认证」的记账项（§10 O8）。

**加固路线（不随刀，按优先级）**：
- **HTML 入口会话门**（推荐的第一步后续）：只对 `text/html` 应答核对会话 cookie——入口请求由父页面发起，cookie 可靠在场；跨站第三方 `<iframe src=/wapps/…>` 嵌套（cross-site ⇒ Lax 扣留 ⇒ 无 cookie）会被 401 挡住，防的是 clickjacking/白嫖嵌入，子资源保持公开。成本一个 header 检查。
- **签名 URL / 独立 wapp origin**：若产品裁决 wapp 字节必须 org-internal，需改冻结 URL 约定（`?dzat=` 票据无法向子资源传播——相对 URL 不继承查询串，此路不通须记死）或复活独立 origin 形（`team-server-and-auth.md:742` 旧案）。均另立项。

**明确否决**：给 iframe 加 `allow-same-origin` 来救会话门。同源形下它等于把宿主 SPA 的 DOM、`document.cookie`（含 CSRF cookie）暴露给第三方 wapp 代码，sandbox 拆除。`team-server-and-auth.md:742` 处方里的 `allow-same-origin` 只在独立 origin 形下无害——同源形禁用（此差异须写进 wapps.md 的 web 段，见 §8 文档清单）。

### 6.2 可见性：启用闸照查

禁用 wapp ⇒ 403（内核 `route_datazen_request` `:223-225`，桌面 `:407-425` 同语义）。启用态存储 = wapps 目录内的 `.enabled` 标记文件（`manager.rs:7,74-78`）——本刀无写面，状态由落盘事实决定，重启扫描生效。403（而非 404）保留了与桌面逐字一致的禁用可见性：wapp id 不是机密（publisher.name 约定文法），org 内成员本就能在管理 UI 看到禁用项。

### 6.3 限流：**L5 扩展到 `/wapps/` 前缀（推荐随刀）**

公开面 = 未认证高频读面，不限流即 DoS 敞口（`/healthz` 可以豁免是因为它不读盘）。改法是 `ratelimit.rs:264` 的门从 `!under_api(path)` 扩为 `!under_api(path) && !path.starts_with("/wapps/")`——**只动 L5 自己的判定，不动共享谓词 `under_api`**（动后者会把本面拉进 L2/L4/L6，违反 §3.4 的层语义）。配套必须接线 `into_make_service_with_connect_info::<SocketAddr>()`（`bootstrap.rs:1018` 的 `axum::serve` 处）：否则无 `AuthenticatedSession` 扩展的请求全部落 `"unattributed"` 归并桶（`ratelimit.rs:283-296`）——整个 `/wapps/` 面共享一个桶，等于全局互斥。该接线顺带修复既有缺口：OIDC 豁免面（`/api/v1/auth/*`）今天的未认证请求就在共用归并桶（`ratelimit.rs:8-9` 自己注明装配点应提供 ConnectInfo）。分类：默认档（`RateLimiter::classify` 现表不动；静态面不是令牌签发类严格档语义）。

---

## 7. 契约落点

### 7.1 §4 总账与 §5 归置

**裁决：进 §5（非域面服务路由扩展），不立 §6.17。**

- §5 的自我定义就是「server 自身的机器面，不属于任何域客户端」（`http-api-contract.md:67`）——wapp 资产面由浏览器原生资源加载直取、无方法背书（§2 依据 3），与 `/mcp`（§11 v1.2 登记「§6.6 之外的服务面」）同类，但比 `/mcp` 更贴 §5 的字面（`/mcp` 至少有 JSON-RPC 方法面，本面纯资源）。落 §5 使「多余端点为零」验收（§9.2）零改动地继续成立。
- §4 总账补一行注记：「另有服务自举路由 5 条（§5）」→「6 条」，**方法数 112/108 不动**——本面不产生域方法。
- 表行草案：

| 路由 | 用途 | 权威 |
| --- | --- | --- |
| `GET /wapps/{wapp id}/{package-relative path}` | wapp 资产 origin 静态面（WR-F4 web 形态的 server 半边；浏览器原生 iframe/img/script 直取，不经 MethodMap——无方法背书要求，`/mcp` 同款非域面先例） | [w8-knife4 设计档](../../development/w8-knife4-wapp-static-design.md) + [wapps.md](../architecture/backend/wapps.md) |

### 7.2 changelog 条目草案（契约 §11）

> - 2026-11-XX：v1.10（W8 刀4，wapp 资产 origin 静态面）。**§5 非域面加一行** `GET /wapps/{wapp id}/{package-relative path}`——WR-F4 冻结约定（`{origin}/wapps/{id}/{path}?{query}`）的服务端半边。路径文法 / 拒绝语义 / 宽松 percent-decode / MIME 白名单 / 启用闸全部复用内核 `workspace_apps::protocol`（新增 origin 形入口 `parse_wapp_origin_path` + 单源 `ORIGIN_ASSET_CSP`；`route_datazen_request` 单家不变）。每条应答注入 `WAPP_ASSET_CSP`（与桌面 `ASSET_CSP` 同指令集去 `datazen:` 源）+ `nosniff` + `Cache-Control: no-cache`，错误应答空体。会话裁决：**面公开**（桌面 `datazen://` 零认证同语义；`sandbox="allow-scripts"` 的 opaque-origin 子资源请求不携带 SameSite=Lax cookie，会话门会拦断子资源——依据与加固路线见设计档 §6）。L5 限流扩至 `/wapps/` 前缀（IP 桶）+ 装配点补 `into_make_service_with_connect_info`。存储根 `DATAZEN_SERVER_WAPPS_DIR`（默认 `data/wapps`；本刀无 HTTP 安装面，包由运维落盘、重启扫描生效）。§4 注记：自举路由 5→6 条，方法账不动。wapps 域客户端面（list/manifest 读 + web 半边 unsupportedMethods 收缩）与安装/管理写面（§6.5 uploads envelope 关系）另立项。

---

## 8. 实施清单（文件级，预估 S/M）

| # | 文件 | 改动 |
| --- | --- | --- |
| 1 | `crates/datazen/src/wapps/protocol.rs` | +`ORIGIN_ASSET_CSP` 常量；+`parse_wapp_origin_path`（前缀剥离 + 复用私有切分件）；+内核单测（origin 形孪生用例 + CSP 推导断言） |
| 2 | `crates/datazen-server/src/http/wapps_static.rs` | **新件**：`WappStaticState` / `wapps_static_router` / `handler_wapp_asset` / `asset_response`（桌面 `datazen_response` 的 axum 对偶：CSP + nosniff + no-cache 逐应答注入） |
| 3 | `crates/datazen-server/src/http/mod.rs` | +`pub mod wapps_static;` |
| 4 | `crates/datazen-server/src/http/router.rs` | `AuthDeps` +`wapp_assets: Option<WappStaticState>` 槽 + merge 块（mcp 槽同款） |
| 5 | `crates/datazen-server/src/http/middleware/ratelimit.rs` | 门扩展 `!path.starts_with("/wapps/")` + 单测（含 ConnectInfo 在场/缺席两态） |
| 6 | `crates/datazen-server/src/config.rs` | +`wapps_dir: PathBuf` + `ENV_WAPPS_DIR` + `DEFAULT_WAPPS_DIR` + from_env 解析（artifact_root 同款）+ 配置测试 |
| 7 | `crates/datazen-server/src/bootstrap.rs` | WappManager 装配（§4.2）+ `AuthDeps` 接线 + `into_make_service_with_connect_info`（§6.3） |
| 8 | `deploy/config/env.example` | 新键 + 分节注释 + 头部计数对齐 |
| 9 | `crates/datazen-server/tests/wapps_static_routes.rs` | **新件**：§9 套件 |
| 10 | 文档 | 契约 §5 行 + §4 注记 + §11 changelog（§7 草案）；`docs/architecture/backend/wapps.md` 增「web 形态」小节（同源 `/wapps/` 约定、单源 CSP、`sandbox` 禁 `allow-same-origin`、公开面裁决）；`team-server-and-auth.md:742/755` 旧独立 origin 文与 WR-F4 的冲突 reconciliation（§10 O4） |

不动的件：`tests/skeleton.rs` 钉住的 `build_router()`；桌面全部；前端/ web 包全部（`wappAssetOrigin.test.ts` 原样双绿）。

---

## 9. 测试计划

### 9.1 内核单测（`crates/datazen/src/wapps/protocol.rs` tests 模块）

- `parse_wapp_origin_path` 孪生组：把 `parse_datazen_uri` 既有用例逐条翻成 origin 形（happy / 查询解码 `+`→空格 / `%2F` 路径 / open 形）。
- 拒绝组**逐字抄 `wappAssetOrigin.test.ts:85-106` 的冻结清单**：未知前缀（`/plugins/…`）、非法 host 四例（`bill-audit` / `Acme.bill-audit` / `acme.` / `.bill`）、空路径（`/wapps/acme.bill-audit`）、六条邪恶路径（`%2e%2e/%2e%2e/settings.json`、`.storage.json`、`.enabled`、`dir/.secret.png`、`a%5Cb.html`、`%00.html`）——三语言（TS / 内核 / server 集成）共享同一份用例清单是 WR-F4「两形态同绿」纪律的延伸。
- CSP：`ORIGIN_ASSET_CSP` == `ASSET_CSP` 移除全部 ` datazen:`；且与前端冻结字面量逐字节相等（字面量原文进断言）。

### 9.2 server 集成测试（`tests/wapps_static_routes.rs`，fixture 参照 `workflow_routes.rs:26-110` 的装配面 + 桌面 `desktop/protocol.rs:137-165` 的包夹具）

夹具：tempdir wapps 根 + `acme.bill-audit` 包（manifest/index.html/assets/icon.svg/data.json/`.storage.json`/`.enabled`）经 `WappManager` 装载；router = `build_router_with_auth`（`wapp_assets: Some`，其余域不挂）。公开面无需登录夹具。

| # | 用例 | 断言 |
| --- | --- | --- |
| t1 | happy：`/wapps/acme.bill-audit/index.html?v=1.0.0` | 200；`Content-Type: text/html`；体逐字节；**CSP 头逐指令等于冻结字面量且不含 `datazen:`**；`nosniff`；`Cache-Control: no-cache` |
| t2 | MIME 表：icon.svg / data.json / 嵌套 assets/icon.svg | `image/svg+xml` / `application/json` / 同 |
| t3 | 宽松解码：`/wapps/acme.bill-audit/assets%2Ficon.svg` | 200（冻结 TS 用例 `:74-76` 的 server 半边） |
| t4 | 穿越/反斜杠/NUL 六条 + `..` 族 | 404 空体 |
| t5 | 隐藏分量：`.storage.json` / `.enabled` / `dir/.secret.png` | 404 |
| t6 | 未知 id；非法 host 文法四例 | 404（**不是 400**——parse 失败统一 404） |
| t7 | 禁用 wapp → 403 空体；重启用 → 200 | 内核启用闸 |
| t8 | 畸形形状：`/wapps`、`/wapps/`、`/wapps/acme.bill-audit` | 404；**t8b**：router 再 `.fallback_service(ServeDir…)`（WR-E5 形状）后仍 404——不落 SPA shell |
| t9 | open 深链：合法 page → 204 空体；未知 page / `open/sub` → 404 | 内核 open 分支 |
| t10 | 扩展白名单：装载后落盘的 `blob.txt`（绕过安装校验模拟） | 404 |
| t11 | 符号链接逃逸：装载后植入 symlink | 404（`ensure_within_wapp_dir`） |
| t12 | `?v=` 有无 | 同答 200 |
| t13 | POST `/wapps/…` | 405 |
| t14 | 403/404 应答也带 CSP + nosniff + no-cache | 桌面 `security_headers_are_always_injected`（`:502-535`）的对偶 |
| t15 | L5（ratelimit.rs 单测补）：`/wapps/` 消耗默认桶；`/healthz` 不消耗；无 ConnectInfo → 归并桶（显式可预期） | §6.3 |

### 9.3 与 web 侧既有测试的关系

`wappAssetOrigin.test.ts`（URL 构造 + 前端解析半边）与 `wappFreezeAcceptance.test.ts`（冻结验收）不动、保持双绿；server 套件是同一文法的 HTTP 半边；内核单测是两形态共同的文法所有者。三层用例清单互抄（§9.1），漂移在 CI 三处同时可见。**端到端浏览器验证（CSP `'self'` 在 `sandbox="allow-scripts"` 下的实际放行、子资源 cookie 行为复核）不属于本刀的 Rust/TS 测试面**，挂到 wapp web 半边刀的 E2E（§10 O2/O3）。

---

## 10. 风险与开放问题（需拍板清单）

| # | 问题 | 影响与建议 |
| --- | --- | --- |
| **O1** | **范围**：清单读（list/manifest）不随本刀（偏离父 agent 倾向）。备选 = 随刀则必须连 `@datazen/backend-client` wapps 域客户端 + web RouteSpec + 前端命令切换 + unsupportedMethods 收缩一起上（契约 §2 增长规则），刀面三包 | 建议维持本设计（server 单仓）；收口刀形状已备好：两方法（`listWapps` → `GET /api/v1/wapps`、`getWappManifest` → `GET /api/v1/wapps/{id}`，无令牌读面） |
| **O2** | **CSP `'self'` 求值语义**：`sandbox="allow-scripts"`（opaque 文档 origin）下 `'self'` 是否匹配 URL origin。设计采纳 WR-F4 冻结读法（URL origin 求值）；若实际浏览器按文档 origin 求值，wapp 页自加载被拦且**冻结指令集内无补救源**（同源即 URL origin，无物可点名）——WR-F4 需重开 | 上线前用真实包在 Chromium/WebKit/Firefox 各跑一次 iframe 冒烟；这是本设计唯一的浏览器行为押注，且该押注是 WR-F4 已冻结的、非本刀新增 |
| **O3** | **SameSite 与子资源 cookie 复核 + 加固路线**：公开面的依据是「Lax cookie 不达 opaque-origin 子资源」（§6.1 依据 1，规范行为需浏览器实证）。若要 wapp 字节 org-internal：首选 HTML 入口会话门（跨站嵌入 401、子资源公开）；签名 URL 不可行（相对 URL 不继承查询串，已记死）；独立 origin 形（`team-server-and-auth.md:742` 旧案）成本最高 | 产品/安全拍板 wapp 字节的机密级别；建议 v1.1 先做 HTML 入口门 |
| **O4** | **文档冲突**：`team-server-and-auth.md:742,755` 的「独立 wapp origin、不得与主应用同源、`allow-same-origin`」旧文 vs WR-F4 同源冻结。旧文的 `allow-same-origin` 处方若被误用于同源形是**漏洞级错误** | 建议随本刀顺带改这两行（指向 wapps.md 新 web 段）；或开独立文档 reconciliation |
| **O5** | **L5 扩展 + ConnectInfo 接线是否随刀**：不随则公开面无限流（healthz 同现状），且 OIDC 豁免面的归并桶缺口继续潜伏 | 建议随刀（两处小改，见 §6.3） |
| **O6** | **安装/管理面立项形状**：服务端 wapps 目录目前 = 运维落盘 + 重启生效。上传安装走 §6.5 uploads envelope 复用还是独立 admin 面；热加载（fs watcher / 重载端点）要不要 | 另立项；在此之前文档明示「重启扫描」语义 |
| **O7** | **缓存升级**：ETag/304 或版本化 immutable（需「重装必 bump 版本」的打包纪律背书，同版本重装现状会破坏 immutable） | 可选增强，非本刀 |
| **O8** | **多组织**：单 wapps 根是 v1 单组织事实；多组织需要 per-org 存储根 + 认证补齐（公开面在多组织下不可接受），顺序上认证先行 | 记账，随多组织立项 |
| O9 | **深链（open）web 语义**：204 是占位；真正的 SPA 深链路由（windowRouter 消费 `parseWappAssetTarget`）与 SDK 侧导航方式归 wapp web 半边刀 | 消费方明确后可改 404/重定向，成本一个分支 |

---

## 11. 验收口径

1. `cargo test -p datazen --lib`：内核 origin 形解析 + CSP 推导新增用例全绿，既有用例零改动。
2. `cargo test -p datazen-server`：`wapps_static_routes.rs` t1–t15 全绿；`skeleton.rs` / `auth_chain.rs` 等既有套件零回退（`build_router` 未动；`AuthDeps` 新槽默认 `None` 不改任何既有装配）。
3. web 侧 `wappAssetOrigin.test.ts` / `wappFreezeAcceptance.test.ts` 原样双绿（前端半边不受 server 实现影响）。
4. 契约面：§5 表可点到本路由；axum router 与契约逐条对齐的 CI 断言（§9 WR-C3 机器核对）对新行同样成立——本面在 `/api/` 之外，域面扫描器天然不命中，需在扫描器的非域面白名单（`/healthz` / `/readyz` / `/mcp` 同列）加 `/wapps`。
