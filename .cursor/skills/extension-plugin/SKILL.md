---
name: extension-plugin
description: Create, scaffold, modify, debug, package and test a DataZen UI Extension (a runtime plugin that contributes a workspace page and/or themes). Use this skill when a developer wants to build an extension for the DataZen desktop database tool (installed into the host app's Plugins list), write a plugin page against the sandboxed postMessage bridge, contribute a theme, package the extension into an installable zip, or troubleshoot install / theme / security issues. Everything is self-contained here — no DataZen source checkout required.
---

# 构建 DataZen 运行时扩展（Extension/Plugin）

DataZen 是一个面向**终端用户**的桌面数据库工具（宿主应用）。第三方开发者**不需要**获取 DataZen 源码，只需按本文档构建一个扩展包（一个目录 + manifest），用户就能在 DataZen 里安装它，获得一个**工作区页面**和/或**主题**。

一个扩展 = 一份 `manifest.json` 声明「贡献了什么」+ 包内页面/主题资源。宿主把页面跑在**沙箱 iframe**（opaque origin，无 Tauri API、无数据直连），页面通过受控 `postMessage` 桥与宿主对话。这些全是**运行时**能力，与「数据库驱动插件」（需要编译进 DataZen）无关。

> 扩展只做 UI 呈现与桥接取数，**不能**直接访问文件系统/网络/数据库连接——所有能力都走桥（见下）。这既是安全边界也是能力上限。

---

## 1. 包结构

在任意目录建一个文件夹，名字必须等于 `manifest.id`（形如 `<publisher>.<name>`）：

```text
<publisher>.<name>/
├── manifest.json          # 必需
├── index.html             # 有 contributes.pages 时必需（沙箱页面入口）
├── assets/                # 可选：脚本 / 样式 / 图标（页面内相对引用）
├── locales/<locale>.json  # 可选：你的语言翻译表（桥 i18n.getString 用，en 兜底）
└── themes/<theme-id>/     # 有 contributes.themes 时：tokens.css（可 url() 引用包内相对资产）
```

- **id 规则**：`^[a-z0-9][a-z0-9-]{0,30}\.[a-z][a-z0-9-]{1,31}$`，例如 `acme.bill-audit`、"publisher.name" 小写+连字符。
- **仅这些文件类型会被接受**：`html / js / mjs / css / json / svg / png / webp / woff2 / woff`。
- **安全硬限制**：SVG ≤ 256 KB，且禁止 `<script>`、事件处理器属性（`onload=` 等）、`javascript:` URL；整包 ≤ 50 MB、≤ 2000 个文件；拒绝符号链接；不允许 `..` 路径穿越（所有 manifest 里声明的相对路径都落在包目录内）。

---

## 2. manifest.json

```jsonc
{
  "id": "com.example.bill-hud",     // 必须 == 目录名
  "name": "Bill HUD",
  "version": "1.0.0",               // semver
  "apiVersion": 2,                  // 必须 = 2（协议版本）
  "icon": "assets/icon.png",        // 可选：插件卡片方形图标（png|webp|svg）
  "entry": "index.html",            // 有 contributes.pages 时必需
  "contributes": {
    "pages": [
      {
        "id": "bill-hud",           // [a-z0-9-_]{1,64}
        "title": "Bill HUD",
        "icon": "assets/icon.svg",  // 可选
        "showIn": "workspace"       // 目前仅支持 workspace
      }
    ],
    "themes": [
      {
        "id": "night-hud",
        "name": "Night HUD",
        "tokensCss": "themes/night-hud/tokens.css",
        "modes": ["dark"]           // 非空，只允许 light|dark
      }
    ]
  },
  "permissions": ["context:connections", "command:invoke", "storage:local", "ui:notify"]
}
```

**contributes 字段**

- `pages[]`：可在工作区打开的页面。`showIn` 只能是 `workspace`；有 pages 必须有 `entry`（页面入口 HTML）。
- `themes[]`：主题。`tokensCss` 必填（token 定义 CSS）；`modes` 非空，只含 `light|dark`。
  可选对齐旧主题包的能力（软失败：单个资产坏只降级对应切片，不崩整体）：
  - `editorJson`：CodeMirror 编辑器配色覆盖
  - `chartsJson`：图表系列配色 `{seriesKey: [color…]}`
  - `iconsDir`：语义图标覆盖目录（文件名 `<semanticId>.svg|.webp|.png`）
  - `previewImage`：主题预览图

**permissions（deny-by-default）**：

| 权限 | 解锁的能力 |
|------|-----------|
| `context:connections` | `context.getConnections / getActiveConnection`（只读连接摘要） |
| `command:invoke` | `command.invoke`（经宿主执行数据库/驱动命令） |
| `storage:local` | `storage.get/set/remove`（每插件命名空间 KV，≤1MB） |
| `ui.notify` | 系统通知（**无需**声明也可用，宿主 ≥5s 冷却） |

`backend` 字段 v1 **必须留空/不写**（预留位）。

---

## 3. 页面侧能力：postMessage 桥（推荐用 SDK）

页面跑在沙箱 iframe：**没有** `window.tauri`、没有 `fetch` 直连网络；取数能力一律走桥。正式开发用官方 npm 包 **`@datazen/extension-sdk`**（零运行时依赖；React 绑定可选）。

```ts
import { createClient } from '@datazen/extension-sdk';

const client = createClient();
// 必须先握手；成功得到宿主上下文 { apiVersion, locale, dark, tokens }
const ctx = await client.ready();

// context（需 context:connections）
const conns = await client.context.getConnections();          // ConnectionSummary[]
const active = await client.context.getActiveConnection();    // ConnectionSummary | null

// command（需 command:invoke）—— 转发宿主数据库命令
const result = await client.command.invoke({
  connectionId: active?.id ?? conns[0]?.id ?? '',
  command: 'query',
  args: { sql: 'SELECT 1' },
});

// storage（需 storage:local）
await client.storage.set('myKey', { n: 1 });   // 值会被 JSON 序列化
const v = await client.storage.get('myKey');  // 读回 {n:1}
await client.storage.remove('myKey');

// notify（无需权限）
await client.notify({ title: '完成', body: '…' });

// i18n（查包内 locales/<locale>.json，en 兜底；查不到返回 null）
const s = await client.i18n.getString('greeting');

client.detach(); // 可选：拆桥（卸载监听 + 中止挂起请求）
```

- `ready()` 是手动开始（只需成功一次）；握手失败/超时会 reject。
- 每个请求**30s 超时**，宿主并发上限 20（超出 `E_RATE_LIMIT`）。所有失败统一抛 `ExtensionError`，带 `code`（`E_PERMISSION` / `E_NOT_FOUND` / `E_TIMEOUT` / `E_RATE_LIMIT` / `E_PLUGIN_DISABLED` / `E_BAD_REQUEST` / `E_NOT_IMPLEMENTED` / `E_INTERNAL` / `EXTENSION_VERSION_MISMATCH` / `EXTENSION_DETACHED`）。
- **反伪造**：SDK 只信任 `event.source === parent` 的消息；不要信任来自别处的消息。

不用打包器、纯 JS 的页面也可直接写裸桥（同样的信封协议），但**强烈建议**用 SDK 以免出错。

---

## 4. 主题一致性（页面铁律）

扩展页渲染在宿主窗口的 iframe 里，**必须和 DataZen 保持视觉一致**：宿主切深色，你的页不能逗留在浅色。方法：

1. **禁止硬编码颜色**。取色一律用 token：语义组 `--c-*`（面/文字/强调）与 `--dt-*`（DataTable 单元格类型）。SDK 帮你把 token 写进页面 `:root` 并同步 `.dark` class。
2. 推荐用 SDK 的 React 绑定订阅主题快照：

```ts
// import 'useTheme' from '@datazen/extension-sdk/react'
const { dark, tokens } = useTheme(); // 宿主每次切换主题都会重渲
```

   非 React：`import { startThemeListener } from '@datazen/extension-sdk'` 或订阅 `datazen:theme-pack-changed` DOM 事件。

3. CSS 用 token 语义值，并带中性 fallback（仅供首帧）：`color: var(--c-fg, #111); background: var(--c-surface, transparent);`，且声明 `color-scheme: light dark`。

**token 语义组**（由宿主握手 `theme.apply` 时推出、SDK 写入页面）——写出你 CSS 里用到的变量名：

- 面/文字：`--c-surface` `--c-surface-alt` `--c-surface-raised` `--c-surface-inset` `--c-edge` `--c-fg` `--c-fg-secondary` `--c-fg-muted`
- 强调/反馈：`--c-accent` `--c-success` `--c-warning` `--c-danger` `--c-query-run`
- 标题栏：`--c-titlebar` `--c-titlebar-fg` `--c-titlebar-fg-muted` `--c-titlebar-hover`
- DataTable 单元格：`--dt-null` `--dt-bool` `--dt-number` `--dt-datetime` `--dt-json` `--dt-text` `--dt-binary`

> 不要用字面颜色当常驻配色——只允许「快照到手前一瞬」的 fallback 值。

---

## 5. 主题贡献（如果你发主题而非页面）

纯主题扩展只需 `contributes.themes`（可省略 `entry` 与 permissions）：

```jsonc
{
  "id": "com.example.palette", "name": "Palette", "version": "1.0.0", "apiVersion": 2,
  "contributes": { "themes": [{ "id": "soft", "name": "Soft", "tokensCss": "themes/soft/tokens.css", "modes": ["light","dark"] }] }
}
```

`tokensCss` 与 `themes/<id>/tokens.css` 里定义覆盖的值。DataZen 本身就带了一个纯主题扩展示例 `community.slate-blue`，可以直接照着它的结构写。

---

## 6. 打包成可安装 zip

DataZen 用户可选的安装包形态是**目录路径**或**zip**。你要做的就是把包目录打成一个 zip，且 **zip 顶层就是包内容（manifest.json 一定在根）**：

```bash
# 在包目录内执行
cd com.example.bill-hud
zip -rqX ../com.example.bill-hud.zip . -x '.DS_Store'
```

> 不需要编译——源码目录就是安装物（扩展包零构建）。改代码后让用户重新安装（同 id 重装会覆盖旧的）。

---

## 7. 安装验证（用户在 DataZen 里做）

1. 打开 DataZen → 左侧栏「插件」→「安装插件…」。
2. 选你给 zip 或目录。
3. 预览卡片（名称/版本/权限清单）→ 「安装」，出现卡片即成功。
4. 若贡献了页面：「工作区」→ 点页面 Tab。
5. 若贡献了主题：「设置」→「外观」→ 选它。
6. 卸载会删掉插件整个目录（**含用户存储在 storage.* 里的数据**），确认弹窗会明说。

---

## 8. 交付清单 / 常见坑

- 目录名 == `manifest.id`；`apiVersion=2`；semver 版本。
- 权限：用到 `context/command/storage` 就声明对应权限；`ui.notify` 不用声明。
- 页面入口 `index.html` 才 → 沙箱。任何 `fetch`/`<script src=http…>` 都会 被宿主 CSP 拦下；资产 只有 `datazen://{pluginId}/{path}` 能拿。
- 不要在 CSS 里写死主题色（见第 4 节）。
- 测试：把 zip 发给一个装了 DataZen 的人，让它安装、开页面、切主题，手动过一遍就是「过」。如果弄错了，本地不会有单元测试——所有决定都在宿主那一侧。