# DataZen 插件系统技术方案

> 对应 [PRD](./ui-plugins.md) v0.3。命名沿用 `data-dashboard-implementation.md` 惯例。
> 状态：Draft。分支：`feature/ui-plugins`（worktree：`../datazen-ui-plugins`）。

## 1. 总体架构

```text
┌───────────────────────────── Main Window (React) ─────────────────────────────┐
│ aside(+workspace/+plugins) │ WorkspaceNav │ WorkspaceTabBar │ PluginPageShell │
│        pluginStore         │  workspaceTabsStore            │   │postMessage │
└─────────────────────────────┬───────────────────────────────┼────────────────┘
                              │ Tauri IPC                     │ datazen://{pub}.{name}/…
┌─────────────────────────────▼───────────────────────────────▼────────────────┐
│ src-tauri                                                                     │
│  commands/plugins.rs ──► plugins/{mod,manifest,install,storage}.rs            │
│  register_uri_scheme_protocol("datazen") ──► 资产服务 + open 深链命令           │
│  execute_driver_command（复用，不改）                                           │
└───────────────────────────────────────────────────────────────────────────────┘
                                   ▲
                    sandbox iframe（opaque origin，@datazen/ui-plugin-sdk）
```

职责边界：

| 层 | 职责 |
|----|------|
| Rust | 安装/校验/启停持久化、`datazen://` 资产与深链服务、storage KV、命令转发 |
| Host 前端 | 模式导航、Tab 状态、桥接路由与权限判定、token 快照、管理页 UI |
| SDK（插件侧） | 类型化 RPC 客户端、token 应用、基础样式；不含宿主逻辑 |

## 2. Rust 后端设计

> **兼容性决议**：一次性切换，不做旧数据迁移。`{appData}/themes/` 旧主题包不再读取；主题以新插件格式重新安装。
> Settings「主题包」分区改造为 **「外观 → 主题」纯切换器**（仅列已启用插件的 themes 贡献），移除安装/卸载逻辑；治理统一在插件管理页（Q10）。

### 2.1 模块结构

```text
src-tauri/src/plugins/
├── mod.rs        # PluginManager：LoadedPlugin 表（manifest+enabled），挂到 AppState
├── manifest.rs   # serde 类型 + validate_manifest()（含路径安全检查）
├── install.rs    # zip/目录安装（zip 读取复用 theme/install.rs 的 LimitedZipReader 模式）
└── storage.rs    # {appData}/plugins/{id}/.storage.json 读写（原子写）
```

`AppState`（commands/mod.rs:65）增加字段：

```rust
pub plugins: Arc<PluginManager>,   // 内部 RwLock<HashMap<String, LoadedPlugin>>
```

### 2.2 manifest 类型与校验（manifest.rs）

```rust
pub const PLUGIN_API_VERSION: u32 = 2;

#[derive Deserialize] #[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginManifest {
    pub id: String, pub name: String, pub version: String,
    pub api_version: u32,
    #[serde(default)] pub author: Option<String>,
    #[serde(default)] pub description: Option<String>,
    #[serde(default)] pub entry: Option<String>,
    pub contributes: Contributions,
    #[serde(default)] pub permissions: Vec<Permission>,
    #[serde(default)] pub backend: Option<serde_json::Value>, // v1 必须 None
}

#[derive Deserialize] #[serde(rename_all = "camelCase")]
pub struct Contributions {
    #[serde(default)] pub pages: Vec<PageContribution>,   // {id,title,icon,show_in}
    #[serde(default)] pub themes: Vec<ThemeContribution>, // {id,name,tokens_css,modes,preview_image}
}

// Permission: enum，serde 从字符串解析：
// ContextConnections | CommandInvoke | StorageLocal | UiNotify —— deny_unknown 保证拼错即失败
```

校验规则（`validate_manifest` + 安装期两段执行）：

1. `id` 匹配 `<publisher>.<name>`：`^[a-z0-9][a-z0-9-]{0,30}\.[a-z][a-z0-9-]{1,31}$`（如 `acme.bill-audit`）；目录名 == `manifest.id`。
2. `api_version != PLUGIN_API_VERSION` → 错误文案含「需要 DataZen ≥ x.y.z」。
3. `version` 过 semver 解析；`backend` 非 None → 明确报错（P2 前不支持）。
4. 有 `pages` 贡献时 `entry` 必填且文件存在；page.id 字符集 `[a-z0-9-_]`，长度 ≤64。
5. 所有声明路径（entry/icon/tokens_css/preview_image）：相对路径、无 `..` 组件、解析后必须位于插件目录内（对齐 `validate_theme_zip_path` 的穿越防护思路）。
6. 文件扩展名白名单 `html|js|mjs|css|json|svg|png|webp|woff2|woff`；总大小 ≤50MB、文件数 ≤2000（复用 theme zip 限额常量风格）。
7. 权限集合：纯主题插件允许空 permissions；含 pages 时至少不要求强制组合，但未声明权限的运行时调用一律拒绝（§4）。

### 2.3 安装器（install.rs）

- 入口命令：`install_plugin_with_dialog`（仿 `install_theme_pack_with_dialog`，dialog 选 zip 或目录）、`install_plugin_from_path(path)`（E2E 用）。
- 流程：解压/拷贝到 `{appData}/plugins/.staging-{uuid}` → 校验（§2.2 全量）→ 原子改名 `.staging-*` → `{id}`（已存在则先备份为 `{id}.old.bak`，成功后删除）→ 注册进 PluginManager 并落盘 enabled=true。
- zip 条目名走 `validate_theme_zip_path` 同款检查（拒绝绝对路径/`..`/symlink 名）。

### 2.4 自定义协议 datazen://（mod.rs）

```rust
```rust
tauri::Builder::default()
    .register_uri_scheme_protocol("datazen", handle_datazen_request)
```

- **URL 语法**：`datazen://<publisher>.<extension-name>/<path-or-command>?<query-params>`
- **path 形态**（v1 资产服务）：`datazen://acme.bill-audit/index.html`、`datazen://acme.bill-audit/assets/icon.svg`。解析：host == `manifest.id` → PluginManager 查存在且 enabled → path 组件校验（无 `..`、无反斜杠、非隐藏文件，`.storage.json` 拒绝）→ 读文件按扩展名回 Content-Type（MIME 表复用 `themePackApply.ts` 的映射，Rust 侧同表复制）。
- **command 形态**（宿主拦截动作，v1 仅保留语法位）：`datazen://acme.bill-audit/open?page=quota-check&uid=123`。v1 实现 `open`（打开指定 page Tab 并把 query params 原样转发给 iframe 作为启动参数）；未知 command 返回 404。为 P2 深链/跨插件跳转预留。
- Windows 下 scheme 自动映射为 `http://datazen./...`，解析需兼容两种 host 形态。
- 响应头固定注入：`Content-Security-Policy: default-src 'self' datazen:; script-src 'self' datazen:; style-src 'self' datazen: 'unsafe-inline'; img-src 'self' datazen: data:; connect-src 'none'; font-src 'self' datazen:` 与 `X-Content-Type-Options: nosniff`。（BUG-F9-04：macOS WebKit 对自定义 scheme 文档不匹配 `'self'`，须显式枚举 `datazen:` 源——对齐 VSCode webview 做法；保留 `'self'` 兼容 Windows/WebView2 的 `http(s)://datazen.<host>/` 映射形态。宿主 `tauri.conf.json` 相应增补 `frame-src` 与 `img-src` 的 `datazen:`。）
- 404/403 一律返回无 body 状态码，不泄露目录结构。

### 2.5 IPC 命令组（commands/plugins.rs）

全部走 `CommandError` + `CmdExt` 日志约定：

| 命令 | 说明 |
|------|------|
| `list_plugins()` | 摘要数组：id/name/version/enabled/contributes 摘要 |
| `install_plugin_with_dialog()` / `install_plugin_from_path(path)` | §2.3 |
| `remove_plugin(id)` | 删目录 + 关闭相关 Tab 由前端监听事件处理 |
| `set_plugin_enabled(id, enabled)` | 落盘 `{appData}/plugins/{id}/.enabled`；禁用后 datazen:// 直接 403 |
| `get_plugin_manifest(id)` | 管理页详情/权限复查 |
| `read_plugin_file(id, rel_path)` | Host 前端读取 tokens.css/资产字节（替代原 `read_theme_pack_file` 调用路径，供 blob URL 重写）；受 enabled + 路径校验约束 |
| `plugin_storage_get/set/remove(plugin_id, key, value?)` | **plugin_id 由前端从自身注册表解析传入**，Rust 侧再校验存在性；storage.rs 按 key 命名空间隔离 |

事件（emit 到前端）：`plugins:changed`（安装/卸载/启停后）。

### 2.6 Capabilities

`src-tauri/capabilities/default.json.host` 增补上述命令标识（tauri-build 由 `#[tauri::command]` 自动生成 permission identifier，形如 `allow-list-plugins`）。不新增任何全局 fs/shell 能力。

## 3. 桥接协议规范

### 3.1 信封与握手

```ts
// 双向统一信封（target 预留 P2 后端路由）
type Envelope = { ch:'ui-plugin'; type:string; reqId?:string; target:'host'; payload?:unknown };
```

```text
iframe 加载 ──► plugin.ready {apiVersion}
           ◄── host.ready   {apiVersion, locale, dark, tokens}   // 版本不符则 SDK 进入降级提示态
主题变更   ◄── theme.apply  {v, dark, tokens}                     // datazen:theme-pack-changed 时重推
请求/响应  ──► {type, reqId, payload}  ◄── {type+'.ok'|'.err', reqId, payload}
```

- `reqId` 由 SDK 生成（自增）；宿主 30s 超时回 `.err{code:'E_TIMEOUT'}`。
- 并发限流：每插件 ≤20 个未完成请求，超出回 `E_RATE_LIMIT`。

### 3.2 v1 API 与权限映射

| type | 权限 | payload → 返回 |
|------|------|----------------|
| `context.getConnections` | `context:connections` | `{}` → `{connections:[{id,name,dbType}]}`（服务端白名单字段） |
| `context.getActiveConnection` | `context:connections` | `{}` → `{connection|null}` 同上 |
| `command.invoke` | `command:invoke` | `{configId, command, args}` → execute_driver_command 原样结果；审计日志加 `[ui-plugin:{id}]` 前缀 |
| `storage.get` / `set` / `remove` | `storage:local` | `{key[, value]}` → `{value|null}` / `{}` |
| `ui.notify` | — | `{title, body}` → 系统通知（≥5s/次限频） |
| `i18n.getString` | — | `{key}` → 插件自带 locales 查表 |

错误码：`E_PERMISSION` / `E_NOT_FOUND`(configId/command) / `E_TIMEOUT` / `E_RATE_LIMIT` / `E_PLUGIN_DISABLED`。错误一律 `.err{code,message}`，不透传 Rust 错误栈。

## 4. Host 前端设计

### 4.1 类型与状态

```ts
// ConnectionPage.tsx:86 扩展
type WorkspaceMode = 'connections' | 'workflow' | 'dashboard' | 'workspace' | 'plugins';
```

新 store（对齐 src/stores/ 既有 Zustand 风格）：

```ts
// stores/pluginStore.ts —— 已装插件、启用态；监听 plugins:changed 刷新
interface PluginStore {
  plugins: PluginSummary[]; loaded: boolean; error?: string;
  fetch(): Promise<void>;
  setEnabled(id: string, enabled: boolean): Promise<void>;
  remove(id: string): Promise<void>;
}

// stores/workspaceTabsStore.ts —— 独立 Tab 体系
interface WorkspaceTab { key:string; pluginId:string; pageId:string; title:string; icon?:string; version:string }
interface WorkspaceTabsStore {
  tabs: WorkspaceTab[]; activeKey: string | null;
  open(tab): void; close(key): void; activate(key): void;
  closeByPlugin(pluginId): void;   // 卸载/停用时批量关
}
```

### 4.2 组件与文件落点

```text
src/windows/workspace/
├── WorkspaceNavigator.tsx    # 左侧导航栏：已启用 pages 列表（图标+名称+描述），仿 ConnectionNavigatorTree 样式
├── WorkspaceTabBar.tsx       # 独立 Tab 条（无固定 Tab）
├── WorkspaceDefaultCards.tsx # 无 Tab 时默认卡片视图 grid（minmax(240px,1fr)）
└── PluginPageShell.tsx       # 页面壳：懒挂载 iframe、桥生命周期、token 注入、崩溃恢复条
src/windows/plugins/
├── PluginManagementPage.tsx  # 管理：搜索/过滤(全部|Workspace|主题)/卡片网格/安装/启停/卸载（主题卡不提供应用动作）
└── InstallPluginDialog.tsx
src/windows/settings/
└── AppearanceSection.tsx     # 「外观」菜单项：主题选择器（读 pluginStore 中已启用 themes 贡献，单选即应用）+ 外观配置占位
src/lib/uiPluginBridge.ts     # postMessage 路由器 + 权限表 + token 快照推送
src/lib/themeTokens.ts        # THEME_TOKENS 契约常量 + buildThemeSnapshot()
```

ConnectionPage 改动点（最小侵入）：

1. aside 在 dashboard 与 plugins 之间插入两个 `WorkspaceModeButton`（复用现有组件，ConnectionPage.tsx:61）：`nav.workspacePages`(testId `workspace-nav-workspace-pages`)、`nav.plugins`(testId `workspace-nav-plugins`)。
2. 内容区分支：`workspaceMode==='workspace'` → `<WorkspaceNavigator/> + <WorkspaceTabBar/> + panels/default cards`；`'plugins'` → `<PluginManagementPage/>`。
3. `settingsReturnModeRef` 逻辑天然兼容新增 mode，无需改动。

### 4.3 PluginPageShell 生命周期

- **懒挂载**：Tab 首次激活才创建 `<iframe sandbox="allow-scripts" src="datazen://{pluginId}/{entry}?v={version}">`；非激活 Tab `display:none` 保留状态；关闭 Tab 即卸载。
- **消息校验**：只接受 `event.source === iframe.contentWindow` 的消息；发送用 `iframe.contentWindow.postMessage(env, '*')`（opaque origin 无法指定更严 targetOrigin，靠 source 校验兜底）。
- **权限判定**：从 pluginStore 取该插件 manifest.permissions，路由表查 `type→requiredPermission`，缺失直接回 `E_PERMISSION`（不进入任何业务处理）。
- **崩溃恢复**：iframe `onerror`/加载超时显示「重新加载插件页」按钮（重建 iframe）。
- **禁用联动**：订阅 `plugins:changed`，插件被停用/卸载时调用 `closeByPlugin`。

### 4.4 token 快照（themeTokens.ts）

- `THEME_TOKENS`：契约数组，收录 `--c-*` 全集与 `--dt-*` 全集（从 `themePackApply.ts`/`dataTypeColors.ts` 整理成共享常量，放 `src/lib/themeTokens.ts`，SDK 文档同步导出清单）。
- `buildThemeSnapshot()`：`getComputedStyle(document.documentElement)` 读取 + `classList.contains('dark')`。
- 推送时机：shell 的 iframe load 完成（握手后）、`datazen:theme-pack-changed`（themePackApply.ts:96 已有事件）、`useThemeSync` 明暗切换回调。快照带 `v=UI_PLUGIN_PROTOCOL_VERSION`。
- iframe 内由 SDK 写到自身 `:root` 并派发同名 DOM 事件；body 默认透明背景。

## 5. SDK（packages/ui-plugin-sdk）

```text
packages/ui-plugin-sdk/
├── package.json        # name "@datazen/ui-plugin-sdk"，private，仅 TS 无运行时依赖
├── src/index.ts        # export createClient(), useTheme(), definePage()
├── src/bridge.ts       # reqId 自增、Promise 化 RPC、30s 超时、E_* 错误类型
├── src/theme.ts        # 监听 theme.apply → 写 :root 变量 + 派发 datazen:theme-pack-changed
├── src/theme.css       # .dz-btn/.dz-input/.dz-table 等基础控件，全部消费 --c-*/--dt-*
└── __tests__/          # vitest：bridge 编解码、theme 应用（属 Host 侧包，允许进 npx vitest run）
```

`createClient()` 返回类型化对象：

```ts
const dz = createClient();
await dz.ready();                                   // plugin.ready/host.ready 握手
const conns = await dz.context.getConnections();
const rows = await dz.command.invoke({ configId, command:'query', args:{ sql } });
await dz.storage.set('lastUid', '58043285');
const { dark } = dz.useTheme();                     // React hook（可选 react peerDep）
```

插件构建方式不限（裸 ESM 即可）；文档提供 Vite library 模板（external 无、单 bundle 输出 `index.html+assets`）。

## 6. 安全设计

### 6.1 威胁模型与缓解

| 威胁 | 缓解 |
|------|------|
| 恶意脚本逃逸 iframe | `sandbox="allow-scripts"`（无 allow-same-origin/forms/popups/top-navigation），opaque origin 无法访问宿主 DOM/localStorage/Tauri IPC |
| 伪造宿主→插件消息 | SDK 校验 `event.source === window.parent`；宿主校验反向同理 |
| 未授权 API 调用 | 路由表 type→permission 硬编码映射，deny-by-default，缺失即 `E_PERMISSION`（先于任何业务逻辑） |
| 路径穿越读取任意文件 | 安装期 + 协议期双重校验：条目名检查（复用 `validate_theme_zip_path`）+ 运行时 path 组件检查 + `.storage.json`/`.enabled` 隐藏文件拒绝 |
| 凭据泄露 | 桥接白名单字段（id/name/dbType）在 Rust 侧构造返回对象，密码/host/port/用户名字段**物理上不进入**响应结构 |
| 远程载荷 | CSP `connect-src 'none'` + 无远程字体/资源（对齐主题包立场）；协议仅服务本地文件 |
| DoS / 消息洪泛 | 每插件 ≤20 并发请求、通知 ≥5s 限频、storage 单插件 ≤1MB |
| 审计缺失 | `command.invoke` 日志加 `[ui-plugin:{id}]` 前缀，走既有 `CmdExt`/log_redact 链路 |

信任锚：v1 无签名（Q5 已决），安装时权限确认弹窗是唯一人工审查点；管理页可随时复查权限清单。

## 7. 测试方案（按 AGENTS.md 分层）

| 层 | 位置 | 覆盖 |
|----|------|------|
| Rust 单测 | `plugins/*.rs` 内 `#[cfg(test)]` | id/api_version/backend 校验、路径穿越用例集、zip 条目攻击样例、白名单与大小上限、storage 隔离与原子写 |
| Rust 集成 | `src-tauri/tests/plugins_*.rs` | 安装→list→enable/disable→remove 全流程（tempdir appData） |
| Host 前端单测 | `src/lib/__tests__/uiPluginBridge.test.ts` 等 | reqId 关联、权限 allow/deny、超时、token 快照构建；`workspaceTabsStore` 开关/批量关闭 |
| Host E2E | `e2e/specs/plugins.spec.ts` + fixture | 见下 |

E2E fixture：`e2e/fixtures/sample-plugin/`（纯静态 manifest+index.html，无构建步骤）。Journeys：

1. 安装 zip → 管理页卡片出现 → 权限徽标可见
2. Workspace 导航栏出现条目 → 点击 → Tab 打开 → storage 往返断言（桥连通性，不依赖真实 DB）
3. 关闭全部 Tab → 默认卡片视图恢复；两套 Tab 条独立性（连接模式开关不影响 workspace Tab）
4. 停用 → Tab 移除 + 导航栏消失 → 卸载 → 卡片消失
5. `datazen://<id>/../etc` 类非法 URL → iframe 加载失败态展示

例外登记 `docs/e2e-coverage.md`：系统通知限频、iframe 崩溃恢复（手动验证）。

## 8. 实施顺序（文件级 checklist）

**M1 统一基座**
- [ ] Rust：`plugins/{mod,manifest,install,storage}.rs`、`commands/plugins.rs`、`AppState.plugins`、lib.rs 注册命令 + `register_uri_scheme_protocol("datazen")`
- [ ] Capabilities：`default.json.host` 增补
- [ ] 前端：`types/plugin.ts`、`stores/pluginStore.ts`、`stores/workspaceTabsStore.ts`
- [ ] ConnectionPage：WorkspaceMode 扩展、aside 两按钮、内容分支
- [ ] `windows/workspace/{WorkspaceNavigator,WorkspaceTabBar,WorkspaceDefaultCards,PluginPageShell}.tsx`（shell 先静态）
- [ ] `windows/plugins/{PluginManagementPage,InstallPluginDialog}.tsx`
- [ ] i18n：`en.ts` 新增 nav.workspacePages/nav.plugins 及管理页文案（zh-CN 可选）
- [ ] E2E：fixture + journeys 1/4/5

**M2 桥**
- [ ] `lib/uiPluginBridge.ts` 路由 + 权限 + 限流；PluginPageShell 接入 RPC/storage/context/command
- [ ] Rust storage IPC + command.invoke 审计前缀
- [ ] E2E journey 2/3

**M3 SDK 与主题联动**
- [ ] `packages/ui-plugin-sdk/{index,bridge,theme}.{ts,css}` + `__tests__`
- [ ] `lib/themeTokens.ts` 契约常量；快照推送接线（theme-pack-changed/useThemeSync）
- [ ] Settings 新增 `settings.appearance` 菜单项 + `AppearanceSection.tsx` 主题切换器
- [ ] 示例插件完善（含 themes 贡献演示）；i18n.getString 下发

**M4 加固收尾**
- [ ] 崩溃恢复条、日志脱敏核查、E2E 补齐与例外登记
- [ ] 架构文档 `docs/architecture/backend/plugins.md`；`docs/plugin-development.md` 增「运行时 UI 插件」章

## 9. 风险与开放点

| # | 风险/开放点 | 应对 |
|---|------------|------|
| R1 | Windows 下 scheme 映射为 `http://datazen./` 的缓存行为差异 | 响应头 `Cache-Control: no-cache` + 入口 URL 带 `?v={version}` |
| R2 | 插件自带 React/组件库致包体偏大（不允许远程 CDN） | 文档引导轻量依赖；共享运行时留 P2 评估 |
| R3 | opaque origin 下 localStorage 不可用 | 已规避：SDK storage 走宿主 KV |
| R4 | 未来子窗口嵌入插件页 | v1 仅主窗口（Q2）；信封 target 字段已预留 |
| R5 | token 清单演进导致旧插件样式缺变量 | 快照带协议版本；SDK 内置默认值兜底 |
| O1 | `datazen://` command 形态的指令集边界 | v1 仅实现 `open`；扩展需 bump PLUGIN_API_VERSION |
| O2 | 签名校验 | Q5 已决 P2 评估 ed25519 |
