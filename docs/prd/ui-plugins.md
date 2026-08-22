# DataZen 插件系统 PRD（统一主题包与 UI 插件）

| 项目 | 内容 |
|------|------|
| 产品模块 | 插件系统（Plugins：UI 页面插件 + 主题包插件，预留后端 Rust 插件） |
| 文档版本 | v0.6 |
| 创建日期 | 2026-08-21 |
| 更新日期 | 2026-08-22 |
| 状态 | Draft（v0.6 决议已并入：同页单实例；v0.5 反馈已并入：Settings→外观主题切换入口） |
| 关联分支 | `feature/ui-plugins`（worktree：`../datazen-ui-plugins`） |
| UI 原型 | [prototypes/ui-plugins.html](./prototypes/ui-plugins.html) |
| 关联文档 | [插件开发指南（编译时驱动）](../plugin-development.md)、[独立插件开发指南](../independent-plugin-development.zh-CN.md)、[架构文档](../architecture/README.md) |

### v0.6 变更记录（评审决议）

1. **同页多开改单实例**：v1 每个插件页复用同一 Tab（点击已打开项仅聚焦既有 Tab）；「每 Tab 独立 iframe 多开」留待后续版本评估。原 §4.2/§4.4 多开条款与 §4.4 表格 `key={pluginId}:{pageId}` 幂等定义自相矛盾（BUG-F4-02），本版统一按单实例语义修订。

### v0.5 变更记录（评审反馈）

1. Settings 保留主题切换入口并归入新「外观」菜单项（仅切换，不管安装卸载；后续外观配置收拢于此），已决 Q10；管理页主题卡片不再提供应用按钮。

### v0.4 变更记录（评审反馈）

1. **去掉迁移逻辑**：一次性切换，不兼容旧主题包（`{appData}/themes/` 不再读取），已决 Q8。
2. **协议语法确定**：`datazen://<publisher>.<extension-name>/<path-or-command>?<query-params>`，manifest.id 改为 `<publisher>.<name>` 格式（如 `acme.bill-audit`），已决 Q9。

### v0.3 变更记录（评审反馈）

1. 侧边栏新增**「Workspace」独立入口**，位于「数据看板」与「插件」之间。
2. 点击 Workspace → ContentView 切换为 **Workspace 列表页**（展示所有注册为 workspace 的插件页面）。
3. 点击列表项 → 在 **独立的 Workspace Tab 条**中打开；Workspace Tab 条与连接 Tab 条**相互独立、互不影响**。

### v0.2 变更记录（评审反馈）

1. 主题包与 UI 插件**统一为同一套插件系统**（manifest 多贡献类型，主题包成为其中一种贡献）。
2. 预留**前端 + 后端 Rust 插件**的扩展能力（manifest `backend` 字段与 RPC 路由设计前置）。
3. 插件管理入口放主窗口左侧边栏，与「连接 / 工作流 / 数据看板」平级（VSCode 扩展图标模式）。
4. 点击入口打开**插件管理页面**；页面内容视图展示所有注册为 workspace 的插件卡片。
5. 点击 workspace 插件卡片 → 在工作区 Tab 条中以 Tab 加载该插件页面。
6. 开放问题 Q1–Q7 全部按建议拍板，转为「已决事项」（见 §8）。

---

## 1. 背景与问题

### 1.1 现状

DataZen 存在两套彼此独立的扩展机制：

- **编译时驱动插件**：Rust crate + `ui/` React 组件位于 `packages/drivers/<id>/`，构建前由 `resolve-drivers.mjs` 选型注入。面向数据库驱动，需重编宿主。
- **运行时主题包**：zip 安装到 `{appData}/themes/{id}/`，纯声明式 token 包（`manifest.json` + `tokens.css` + 资产），Rust 校验、前端 `themePackApply.ts` 注入。

业务团队基于 DataZen 定制领域工具页（实例：「账单额度核对」「AFI 定价策略查看器」）目前只能 clone 仓库改源码。

### 1.2 问题

1. **源码魔改锁死升级**：业务方无法跟随主线，每次合并靠人工移植。
2. **无法独立分发**：内部小工具必须走完整宿主发布流程。
3. **扩展机制割裂**：主题包与未来的 UI 插件各自一套 manifest/安装/管理链路，用户心智与实现成本双输。
4. **无安全边界**：魔改代码拥有宿主全部权限。
5. **能力错位**：编译时驱动机制面向「数据库接入」，不适合承载业务工具页。

### 1.3 目标

建立**统一的运行时插件系统**：

- 一个 manifest 格式、一个安装根目录、一个管理界面；**主题包成为插件的贡献类型之一**。
- 插件以声明式 manifest 向主窗口贡献**工作区工具页 Tab**（沙箱 iframe 渲染）。
- 通过受控 postMessage 桥访问连接上下文与 Driver Command API。
- manifest 与 RPC 协议**前置预留后端 Rust 插件**扩展点，后续版本可在不破坏前端契约的情况下补齐。

---

## 2. 产品定位

### 2.1 一句话

DataZen 插件系统是运行时的统一扩展机制：一份 manifest 声明插件贡献了什么（工作区页面 / 主题；未来：后端逻辑），宿主提供壳与受控桥，插件在沙箱中运行。

### 2.2 不是什么

| 是 | 不是 |
|----|------|
| 运行时装卸的统一插件体系（UI 页 + 主题） | 编译时驱动插件（`packages/drivers/*`）的替代品；两者长期并存 |
| 沙箱 Web 页面 + 受控 API 桥 | 任意原生代码执行环境（v1 不含插件自有原生代码） |
| 一个管理入口管所有运行时插件 | 应用市场 / 在线分发平台（明确不做） |
| 为后端插件预留协议位 | v1 实现后端 Rust 插件执行 |

### 2.3 设计原则（对齐既有架构纪律）

1. **零硬编码**：宿主不感知具体插件 id，一切经 manifest 动态发现。
2. **复用 Command API**：插件取数一律走既有 `execute_driver_command`，禁止旁路 IPC。
3. **协议版本化**：`PLUGIN_PROTOCOL_VERSION`（编译时）/ `UI_PLUGIN_PROTOCOL_VERSION`（运行时）各自握手。
4. **最小权限**：manifest 声明权限，capability 白名单只放行专用命令组。

---

## 3. 统一模型

### 3.1 插件 = 目录 + manifest + 贡献

安装根目录统一为 `{appData}/plugins/{id}/`（目录名必须等于 `manifest.id`，对齐原主题包做法）：

```text
{appData}/plugins/{id}/
├── manifest.json          # 必需
├── index.html             # 有 pages 贡献时必需
├── assets/                # 白名单：css/js/html/json/svg/png/webp/woff2/woff
└── themes/                # 有 themes 贡献时：<name>/tokens.css 等
```

`manifest.json`（v2 统一 schema）：

```jsonc
{
  "id": "bill-audit",                // ^[a-z][a-z0-9-]{1,31}$ 全局唯一
  "name": "账单额度核对",
  "version": "1.0.0",                // semver
  "apiVersion": 2,                   // 必须 == 宿主 UI_PLUGIN_PROTOCOL_VERSION
  "author": "...",
  "description": "...",
  "entry": "index.html",             // pages 贡献的入口（相对路径）
  "contributes": {
    "pages": [                       // 贡献类型一：工作区工具页
      {
        "id": "quota-check",         // 全局页 id "{plugin-id}:{page-id}"
        "title": "账单额度核对",
        "icon": "assets/icon.svg",
        "showIn": "workspace"        // v1 仅支持 workspace
      }
    ],
    "themes": [                      // 贡献类型二：主题（原主题包整体迁入）
      {
        "id": "midnight-blue",
        "name": "暗夜蓝",
        "tokensCss": "themes/midnight-blue/tokens.css",
        "modes": ["dark"],
        "previewImage": "themes/midnight-blue/preview.png"
      }
    ]
    // 未来：P2 增加 "backend"、"workflows"、"commands" 等贡献类型
  },
  "permissions": [                   // 仅 pages 相关贡献需要；纯主题插件可为空
    "context:connections",
    "command:invoke",
    "storage:local"
  ],
  "backend": null                    // P2 预留字段；v1 必须为 null/缺省
}
```

约束：

- 文件类型白名单、大小上限、路径穿越/符号链接拒绝：合并原主题包校验规则并覆盖 js/html。
- `apiVersion` 不匹配 → 拒绝启用并在管理页明确提示原因。
- 纯主题插件（只有 themes 贡献）不需要 entry/index.html，不进沙箱加载流程。

### 3.2 与旧主题包的关系（一次性切换）

- **不做兼容与迁移**：`{appData}/themes/` 旧目录不再读取；主题以新插件格式（`contributes.themes` 贡献）重新安装。
- Settings **保留主题切换入口**：移入新「外观」菜单项（见 4.5），仅承担切换，不承担安装/卸载；插件治理统一在插件管理页。
- 编译时驱动插件不受影响。

### 3.3 后端 Rust 插件预留（P2，本期只做协议位）

| 预留点 | v1 动作 |
|--------|---------|
| manifest `"backend"` 字段 | schema 定义占位（`{ "kind": "...", "entry": "..." }`），v1 值必须为空 |
| RPC 信封路由 | 信封含 `target` 字段（`"host"`），未来可扩展 `"plugin-backend"`，桥层转发逻辑不动 |
| 权限模型 | permissions 键空间预留 `backend:*` 前缀 |
| 进程/执行形态 | 不预设（sidecar / wasm 二选一留待 P2 RFC）；编译时驱动插件不受影响 |

---

## 4. 主窗口集成（导航与页面）

### 4.1 左侧边栏入口（对齐 VSCode 扩展图标模式）

现有侧边栏（ConnectionPage.tsx aside）：连接 / 工作流 / 数据看板，底部设置。新增：

```text
[数据库图标] 连接          workspaceMode = 'connections'
[工作流图标] 工作流         workspaceMode = 'workflow'
[看板图标]   数据看板       workspaceMode = 'dashboard'
[布局图标]   Workspace     workspaceMode = 'workspace'   ← 新增：位于「数据看板」与「插件」之间
[拼图图标]   插件           workspaceMode = 'plugins'    ← 新增：平级
─────────
[齿轮]      设置           （保持底部）
```

- 「插件」图标用拼图（puzzle）隐喻，i18n 键 `nav.plugins`，testId `workspace-nav-plugins`。
- 「Workspace」图标用布局/网格隐喻，i18n 键 `nav.workspacePages`，testId `workspace-nav-workspace-pages`。
- 无已启用插件时图标不加角标；有可用更新或禁用态异常时加角标提示（P1）。

### 4.2 Workspace 列表页（ContentView 替换）

点击侧边栏「Workspace」→ 右侧 ContentView 切换为 **Workspace 模式**：左侧导航栏 + 右侧 Tab 条与内容区（与连接模式的「左侧连接树 + 右侧查询 Tab」布局对齐）。

```text
┌──────────────────────────────────────────────────────┐
│ 已连接 │                                              │
│        │  Workspace                                   │
│ 助贷库  │  点击左侧导航栏或下方卡片打开插件               │
│  └表   │  ┌─────────────┐  ┌─────────────┐           │
│  └视图  │  │ 🔍 账单额度核对│  │ 📊 AFI 定价  │           │
│         │  │ v1.0.0 · 数据│  │ v1.0.4 · 产品│           │
│─────────│  └─────────────┘  └─────────────┘           │
│ 导航栏  │                                              │
│ 🔍 账单  │                                              │
│   额度核对│                                              │
│ 📊 AFI  │                                              │
└──────────────────────────────────────────────────────┘
```

- **左侧导航栏**（类似连接树）：宽度 180px，展示所有已启用的 Workspace 插件列表（图标 + 名称 + 一句话描述）；hover 高亮，点击即打开对应 Tab。
- **右侧 Tab 条 + 内容区**：点击导航栏项 → Tab 条新增 Tab（标题 = 插件名，可关闭）；Tab 内容为宿主页面壳（标题、`v{version}` 角标），壳内 `<iframe sandbox="allow-scripts">`。
- 关闭所有 Tab 后右侧为空态（导航栏仍在，可再次点击打开）。
- 同一插件页复用同一 Tab（点击已打开项聚焦既有 Tab）；多开留待后续版本评估。
- 停用/卸载插件时自动关闭其对应 Tab、从导航栏移除。
- 切换到连接模式再切回 workspace 模式：两边状态各自保持。

### 4.3 插件管理页面

点击侧边栏「插件」→ 内容区切换为**插件管理页**（同 settings 一样整页替换 ContentView 区域）：

```text
┌──────────────────────────────────────────────────────┐
│ 插件                                    [ 安装插件… ] │
│ 搜索框…        [全部] [Workspace] [主题]              │
├──────────────────────────────────────────────────────┤
│ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐     │
│ │ 卡片：   │ │ 卡片：   │ │ 卡片：   │ │ 卡片：   │     │
│ │ 图标/名称 │ │ ...     │ │ ...     │ │ ...     │     │
│ │ 版本/作者 │ │         │ │ [应用中] │ │ (未启用) │     │
│ │ 权限徽标  │ │         │ │         │ │         │     │
│ │ 启用开关  │ │         │ │         │ │         │     │
│ │ [打开]   │ │         │ │         │ │         │     │
│ └─────────┘ └─────────┘ └─────────┘ └─────────┘     │
└──────────────────────────────────────────────────────┘
```

- **内容视图主体即「注册为 workspace 的插件」卡片列表**（默认过滤器 Workspace）；「主题」过滤片列出主题类插件；「全部」混合展示并分组。
- 卡片信息：图标、名称、版本、作者、一句话描述、权限徽标（悬停显示说明）、启用开关、更新/卸载菜单。
- workspace 类卡片带 **[打开]** 快捷按钮（与 Workspace 导航栏/默认卡片同效，见 4.4）；主题类卡片**不提供应用动作**（切换统一在 设置 → 外观，避免双入口），卡片上提示「在 设置 → 外观 中切换」。
- 安装流程：选择 zip/目录 → 校验 → 展示名称/版本/**权限清单**确认 → 写入 → 列表刷新。校验失败错误可复制。
- apiVersion 不匹配的插件卡片置灰 + 「需要 DataZen x.y.z 以上」提示。

### 4.4 独立 Workspace Tab 条（与连接 Tab 条相互独立）

**Workspace 的右侧区域（Tab 条 + 内容区）是独立于连接模式的另一套 Tab 体系**，两者状态互不影响：

| | 连接模式（现状） | Workspace 模式（新增） |
|--|--|--|
| 布局 | 左侧连接树 + 右侧 Tab 条与内容区 | 左侧导航栏 + 右侧 Tab 条与内容区 |
| 左侧内容 | 连接树（表/视图/Schema） | Workspace 插件列表 |
| Tab 条 | 连接/查询 Tab | 插件页面 Tab（无固定 Tab） |
| key | configId 等 | `{pluginId}:{pageId}` |
| 状态持久 | 各自独立：切换模式不清空/合并 | 同左 |

行为细则：

- **右侧区域**：Tab 条 + 内容区；**未打开任何 Tab 时显示默认卡片视图**（网格布局，卡片包含图标、名称、描述、版本、作者），点击卡片即打开对应 Tab。
- **点击左侧导航栏的插件项** → 右侧 Tab 条新增 Tab，激活并显示插件内容（页面壳 + iframe）。
- Tab 标题 = 插件名，可关闭；关闭 Tab 后导航栏项仍在，可再次点击打开。
- 关闭所有 Tab 后右侧为空态，导航栏仍可操作。
- 同一插件页复用同一 Tab（点击已打开项聚焦既有 Tab）；多开留待后续版本评估。
- 停用/卸载插件时自动关闭其对应 Tab、从导航栏移除。
- 切换到连接模式再切回 workspace 模式：两边 Tab 条各自保持原状（不合并、不清空）。
- 插件页不进入连接树右键菜单、命令面板（v1）。

### 4.5 Settings「外观」菜单项

- SettingsPage 新增一级菜单项「外观」（i18n 键 `settings.appearance`），后续外观相关配置（密度、字号等）统一收拢于此。
- v1 内容：**主题选择器**——列出所有已启用主题贡献（名称、预览图、modes），单选高亮当前主题，点击即应用（复用既有 `themePackApply.ts` 应用管线与 surface 缓存同步）；空态引导「去插件管理页安装主题」。
- 明确不做：安装/卸载/启停（引导至插件管理页）、明暗基础模式切换（保留在现有 ThemeToggle 位置）。

---

## 5. 沙箱与加载

- `<iframe sandbox="allow-scripts">`：opaque origin、无同源访问、无表单提交/弹窗/顶级导航。
- 资产与深链服务：自定义协议 `datazen://<publisher>.<extension-name>/<path-or-command>?<query-params>`（Rust 注册；path 形态严格限制在 `{appData}/plugins/{id}/` 内、拒绝遍历；command 形态 v1 仅实现 `open` 深链）。【已决 Q3/Q9】
- CSP：iframe 响应头 `script-src 'self'`；禁止远程资源加载（离线优先，与主题包拒绝远程字体同一立场）；宿主自身 CSP 不放宽。

---

## 6. RPC 桥

iframe ↔ 宿主 postMessage，统一信封：

```ts
{ ch: 'ui-plugin', type: string, reqId?: string, target: 'host', payload?: unknown }
```

- 请求-响应带 `reqId`；超时 30s；每插件并发限流。
- 每个 `type` 映射所需 permission，未声明即拒绝并回明确错误。
- `target` 字段为 P2 后端路由预留（见 §3.3）。

v1 API 面【权限确认时机已决：安装时一次性，详情页可复查 —— Q1】：

| 方法 | 所需权限 | 说明 |
|------|---------|------|
| `host.ready` / `plugin.ready` | — | 双向握手，交换协议版本与清单 |
| `context.getConnections` | `context:connections` | 连接摘要（id/名称/dbType；绝不含密码） |
| `context.getActiveConnection` | `context:connections` | 当前激活连接（若有） |
| `command.invoke` | `command:invoke` | 转发 `execute_driver_command`，复用既有审计日志【权限粒度到「可调用」级，细粒度白名单 P2 —— Q6】 |
| `storage.get/set/remove` | `storage:local` | 按 pluginId 命名空间隔离的 KV，落盘 JSON |
| `ui.notify` | — | 经宿主通知，限频 |
| `theme.get` / 广播 | — | token 快照（§7） |
| `i18n.getString` | — | 宿主按当前语言返回插件翻译键 |

明确不提供（v1）：文件系统读写、shell、跨插件通信、宿主 store 直读、Tauri invoke 透传、MCP 暴露【Q7 排除，另行立项】。SDK 以 workspace path 包形式提供【Q4】，签名校验不做【Q5，P2 评估 ed25519】。v1 仅主工作区，不进子窗口【Q2】。

---

## 7. 主题整合——token 桥

原则：主题包已是插件体系的贡献类型（§3），token 桥只解决「沙箱 iframe 读不到宿主 CSS 变量」。

- 宿主维护固定 token 契约清单（`--c-*`、`--dt-*` 等），作为协议常量导出。
- iframe load 时推快照 `{ v, dark, tokens }`；监听 `datazen:theme-pack-changed`（src/lib/themePackApply.ts:96）与明暗切换重推。
- SDK 落到 iframe 自身 `:root`（变量名与宿主一致），派发同名 DOM 事件；随附 `theme.css` 基础控件样式，插件零适配跟随任意主题。
- iframe body 透明背景；未知 token 忽略、缺失回退 SDK 默认值。
- 主题包新增 token 时 bump `UI_PLUGIN_PROTOCOL_VERSION`。
- 插件不得贡献 CSS 变量到宿主（单向）；如需配套主题，做成自己 manifest 里的 themes 贡献即可——这正是统一后的自然形态。

---

## 8. 已决事项（原开放问题）

| # | 决议 |
|---|------|
| Q1 | 权限确认：安装时一次性确认，管理页详情可复查 |
| Q2 | 插件仅出现在主工作区，不进子窗口 |
| Q3 | 资产服务采用自定义协议 `datazen://` |
| Q4 | SDK 为 workspace 内 TS 包 + Vite 模板文档，不发 npm |
| Q5 | v1 不做签名；P2 评估 ed25519 |
| Q6 | command.invoke 权限到「可调用」级别 |
| Q7 | 插件页不暴露给 MCP Server，另行立项 |
| Q8 | 兼容策略：一次性切换，不做旧主题包迁移 |
| Q9 | 协议语法：`datazen://<publisher>.<extension-name>/<path-or-command>?<query-params>`，manifest.id 采用 `<publisher>.<name>` 格式 |
| Q10 | 主题切换入口：Settings → 外观（仅切换）；管理页不提供应用动作，治理与消费分离 |

---

## 9. 示例插件（P1）

仓库内置最小示例插件（fixtures 性质，不默认安装）：1 个 workspace 页面 + 1 次 `command.invoke`（list_tables）+ 1 个演示主题贡献 + token 展示块。用途：开发调试、E2E fixture、第三方模板。

## 10. 非功能需求

| 维度 | 要求 |
|------|------|
| 安全 | opaque origin 沙箱；未声明权限即拒绝；路径穿越防护；文件白名单；无远程资源；敏感字段永不进桥接消息；日志脱敏（log_redact 立场） |
| 性能 | Tab 懒创建、关闭即销毁 |
| 稳定性 | 插件崩溃不影响宿主，提供「重新加载插件页」 |
| 兼容 | 一次性切换，不做旧主题包兼容/迁移（Q8）；编译时驱动插件零影响 |
| i18n | 宿主文案仅改 `en.ts`（可选 `zh-CN.ts`）；`i18n-sync-check` 通过 |
| E2E | 硬性规则：侧边栏入口、管理页增删启停、Tab 打开关闭、RPC 链路全部纳入 Host E2E；示例插件作 fixture；例外登记 e2e-coverage.md |
| 测试分层 | Host 只测宿主能力（manifest 校验、桥接路由、权限判定、token 快照）；具名业务插件逻辑不得进 Host 测试 |

## 11. 里程碑

| 阶段 | 内容 | 验收标准 |
|------|------|---------|
| **M1 统一基座** | 插件安装器（manifest v2 校验）、侧边栏两入口（Workspace / 插件）、Workspace 导航栏 + 默认卡片视图 + 独立 Tab 条、插件管理页（卡片/过滤/安装/启停/卸载）、`datazen://` 协议、沙箱壳、静态 hello-world | 手工+E2E：装→列表可见→开 Tab→关→停用→卸载全程 |
| **M2 桥** | RPC 信封/路由/权限判定、context.*、command.invoke、storage、notify | 示例插件完成一次真实跨库查询；越权调用被拒且错误明确 |
| **M3 SDK 与主题联动** | SDK 包、token 快照桥、theme.css、i18n 下发、Settings→外观主题切换器、示例插件完善 | 切换主题包/暗色实时跟随无需刷新；外观页一键切换已启用主题 |
| **M4 加固收尾** | 限流、崩溃恢复、脱敏核查、E2E 补齐与登记、架构文档 docs/architecture/backend/plugins.md | `pnpm test:unit` + `cargo test -p datazen --lib` 绿；E2E 绿 |

## 12. 影响面清单

- Rust：`src-tauri/src/plugins/`（新：安装器/manifest/storage）、`commands/plugins.rs`（新 IPC 组）、`theme/` 读文件入口切到插件目录、capabilities（`default.json.host` 增补）、自定义协议 `datazen://` 注册
- 前端：aside 两个新按钮（`workspaceMode='workspace'` / `'plugins'`）、`windows/workspace/*`（导航栏/Tab 条/默认卡片/页面壳）、`windows/plugins/PluginManagementPage.tsx`、Settings 新增「外观」菜单项 + `AppearanceSection.tsx`（ThemePackSection 改造为纯切换器，移除安装/卸载逻辑）、`lib/uiPluginBridge.ts`
- 新包：`packages/ui-plugin-sdk`
- 文档：本 PRD → 架构文档 + 插件开发指南增章（区分编译时驱动 vs 运行时插件）
