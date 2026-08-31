# DataZen 扩展开发指南（Extension）

> 面向**第三方扩展开发者**。开发 DataZen 的运行时扩展（工作区页面 + 主题）**不需要**获取或编译 DataZen 源码——你只需要按本文构建一个扩展包（一个目录 + `manifest.json`），最终用户即可在 DataZen 里安装它。本文从零演示「开发 → 打包 → 安装 → 调试」的完整闭环。
>
> 相关文档：
> - 架构设计：[docs/architecture/backend/extensions.md](../architecture/backend/extensions.md)
> - 可安装的**示例包源码**：[packages/extensions/](../../packages/extensions/)（`datazen.playground` 全功能示例、`community.slate-blue` 纯主题示例）
> - 插件侧 SDK：`@datazen/extension-sdk`

---

## 0. 什么是「扩展」，什么不是

DataZen 有两套完全不同的插件机制，别混淆：

| | **运行时扩展（本文）** | 数据库驱动插件 |
|---|---|---|
| 面向前端用户 | 工作区页面、主题 | 数据库类型 / 方言能力 |
| 需要 DataZen 源码 | **不需要** | 需要（编译期注入） |
| 加载方式 | 运行时：沙箱 iframe + postMessage 桥 | 编译期链接进 binary |
| 文档 | 本文 | `docs/development/independent-driver-development.*.md` |

**一句话**：你只要写一份 `manifest.json` + 页面/主题资源，用户就能在 DataZen 的「插件」列表里安装，得到一个新的**工作区页面**（比如「账单看板」「运维监控」）或一套**主题**。它跑在宿主开的**沙箱 iframe**里——没有 `window.tauri`、不直接访问文件系统/网络/数据库连接，所有能力（取数、读连接、存储、通知）都通过一个受控的 `postMessage` 桥向宿主申请。这既是安全边界，也是能力上限。

---

## 1. 快速上手（5 分钟）

### 1.1 建包目录

包就是一个普通文件夹，**目录名必须等于 `manifest.id`**（形如 `<publisher>.<name>`）：

```bash
mkdir com.example.bill-hud && cd com.example.bill-hud
```

### 1.2 写 `manifest.json`

```jsonc
{
  "id": "com.example.bill-hud",          // 必须 == 目录名
  "name": "Bill HUD",
  "version": "1.0.0",                    // semver
  "apiVersion": 2,                       // 必须 = 2（当前协议版本）
  "icon": "assets/icon.png",             // 可选：插件方形品牌图标（png|webp|svg）
  "description": "…",                    // 可选
  "author": "…",                         // 可选
  "entry": "index.html",                 // 有 contributes.pages 时必需
  "contributes": {
    "pages": [
      {
        "id": "bill-hud",                // ^[a-z0-9-_]{1,64}$
        "title": "Bill HUD",
        "icon": "assets/icon.svg",       // 可选
        "showIn": "workspace"            // 目前仅支持 workspace
      }
    ],
    "themes": [
      {
        "id": "night-hud",
        "name": "Night HUD",
        "tokensCss": "themes/night-hud/tokens.css",
        "modes": ["dark"]                // 非空，只允许 light|dark
      }
    ]
  },
  "permissions": ["context:connections", "command:invoke", "storage:local", "ui:notify"]
}
```

### 1.3 写页面入口并打包

页面入口是一个普通 HTML（后面讲怎么接桥）。打完包后直接把它目录压成 zip 就是安装物——**扩展零构建，源码目录就是安装物**：

```bash
cd com.example.bill-hud
zip -rqX ../com.example.bill-hud.zip . -x '.DS_Store'
# zip 顶层必须是包内容（manifest.json 一定在根）
```

### 1.4 安装验证

1. 打开 DataZen → 左侧栏「**插件**」→「**安装插件…**」。
2. 填入 zip 或目录的路径。
3. 预览卡片（名称 / 版本 / 权限清单）→「安装」，出现卡片即成功。
4. 页面：左侧栏「**Workspace**」→ 点你的页面 Tab。
5. 主题：「设置」→「外观」→ 选它。

> 安装是**拷贝语义**：改了源码要**重新安装**才能生效（同 id 重装会覆盖旧包并备份为 `{id}.old.bak`）。停用 / 卸载在插件管理页操作。

---

## 2. 包结构速查

```text
<publisher>.<name>/
├── manifest.json          # 必需
├── index.html             # 有 contributes.pages 时必需（沙箱页面入口）
├── assets/                # 可选：脚本 / 样式 / 图标（页面内相对引用）
├── locales/<locale>.json  # 可选：翻译表（桥 i18n.getString 用，en 兜底）
└── themes/<theme-id>/     # 有 contributes.themes 时：tokens.css（可 url() 引用包内相对资产）
```

**硬性规则（违反会被安装校验拒绝）**：

- `id` 形如 `<publisher>.<name>`：`^[a-z0-9][a-z0-9-]{0,30}\.[a-z][a-z0-9-]{1,31}$`（小写 + 连字符）。
- **文件类型白名单**：`html / js / mjs / css / json / svg / png / webp / woff2 / woff`。
- 安全上限：SVG ≤ 256 KB 且禁止 `<script>`、事件处理器属性（`onload=` 等）、`javascript:` URL；整包 ≤ 50 MB、≤ 2000 个文件；拒绝符号链接；`manifest` 声明的所有相对路径禁止 `..` 穿越（必须落在包目录内）。
- `apiVersion` = 2。

---

## 3. `manifest.json` 字段详解

### 3.1 顶层

| 字段 | 必填 | 说明 |
|------|------|------|
| `id` | ✔ | 必须等于目录名，格式见上 |
| `name` | ✔ | 插件的显示名 |
| `version` | ✔ | semver |
| `apiVersion` | ✔ | **2**（协议版本，与研究宿主一致） |
| `entry` | 有 pages 时 | 页面入口 HTML 相对路径（如 `index.html`） |
| `icon` | 否 | 插件卡片方形图标（`png/webp/svg`），声明了就必须存在 |
| `description` / `author` | 否 | 展示用 |
| `contributes` | 否 | 见下 |
| `permissions` | 否 | 见下；deny-by-default，用到才声明 |
| `backend` | 否 | **v1 必须留空/不写**（预留位） |

### 3.2 `contributes`

- **`pages[]`**：可在工作区打开的页面。每项 `{ id, title, icon?, showIn? }`，`showIn` 只能是 `workspace`。有 pages 就必须有顶层 `entry`。
- **`themes[]`**：主题贡献。`tokensCss` 必填，`modes` 非空且只含 `light|dark`。可选字段兼容旧主题包能力（单个资产坏了只降级对应切片，不崩整体）：
  - `editorJson`：CodeMirror 编辑器配色覆盖
  - `chartsJson`：图表系列配色 `{seriesKey: [color…]}`
  - `iconsDir`：语义图标覆盖目录，文件名 `<semanticId>.svg|.webp|.png`
  - `previewImage`：主题预览图

### 3.3 `permissions`（deny-by-default）

| 权限 | 解锁的桥能力 |
|------|-------------|
| `context:connections` | `context.getConnections` / `getActiveConnection`（只读连接摘要） |
| `command:invoke` | `command.invoke`（经宿主执行数据库/驱动命令） |
| `storage:local` | `storage.get/set/remove`（每插件命名空间 KV，≤ 1 MB） |
| `ui:notify` | 系统通知（**无需声明**即可用，宿主 ≥5s 冷却） |

> 原则：用到 `context/command/storage` 就声明对应权限；`ui.notify` 不用声明。声明了却没用的权限也合法，但会出现在权限清单里让用户知情。

---

## 4. 页面侧：postMessage 桥（推荐用 SDK）

页面跑在**沙箱 iframe**：**没有** `window.tauri`，**不能** `fetch` 直连网络，资产只能通过 `datazen://{pluginId}/{path}` 拿到。取数能力**全部走桥**。

### 4.1 用官方 SDK `@datazen/extension-sdk`

> 纯 TS、零运行时依赖（React 绑定是可选）。开发产物建议把你的代码连同 SDK 一起**打包**进页面（零构建时可以直接从 `datazen://…` 引用 SDK 源码，见示例；或把 SDK 编译进你的 bundle）。

```ts
import { createClient } from '@datazen/extension-sdk';

const dz = createClient();
// 必须先握手；成功返回宿主上下文 { apiVersion, locale, dark, tokens }
const ctx = await dz.ready();

// ① context（需 context:connections）
const conns = await dz.context.getConnections();        // ConnectionSummary[]
const active = await dz.context.getActiveConnection();  // ConnectionSummary | null
// ConnectionSummary = { id, name, dbType }  // 白名单只读字段

// ② command（需 command:invoke）—— 转发宿主数据库命令
const result = await dz.command.invoke({
  connectionId: active?.id ?? conns[0]?.id ?? '',
  command: 'query',               // 驱动 Command API，如 query/execute/list_objects…
  args: { sql: 'SELECT 1' },
});

// ③ storage（需 storage:local）—— 值会 JSON 序列化
await dz.storage.set('myKey', { n: 1 });
const v = await dz.storage.get('myKey');   // 读回 {n:1}
await dz.storage.remove('myKey');

// ④ notify（无需权限；宿主限流 ≥5s）
await dz.notify({ title: '完成', body: '…' });

// ⑤ i18n（查包内 locales/<locale>.json，en 兜底；查不到返回 null）
const s = await dz.i18n.getString('greeting');

dz.detach(); // 可选：拆桥（卸载监听 + 中止挂起请求）
```

**SDK 行为要点**：

- `ready()` 手动开始，只需成功一次；握手失败/超时 reject。
- 每个请求 **30s 超时**，宿主**并发上限 20**（超出抛 `E_RATE_LIMIT`）。
- 所有失败统一抛 **`ExtensionError`**，带 `code`：`E_PERMISSION` / `E_NOT_FOUND` / `E_TIMEOUT` / `E_RATE_LIMIT` / `E_PLUGIN_DISABLED` / `E_BAD_REQUEST` / `E_NOT_IMPLEMENTED` / `E_INTERNAL`，以及 SDK 本地的 `EXTENSION_VERSION_MISMATCH` / `EXTENSION_DETACHED`。
- **反伪造**：SDK 只信任 `event.source === parent` 的消息，不要信任来自别处的消息。

### 4.2 不用打包器、纯 JS 也能写（裸桥）

直接在页面里 postMessage 同样的信封协议即可（完整可跑的最小实现就是示例包 `datazen.playground/assets/app.js`）。但**强烈建议用 SDK**，避免信封细节出错。

---

## 5. 主题一致性（页面铁律）

扩展页渲染在宿主窗口的 iframe 里，**必须和 DataZen 视觉保持一致**：宿主切深色，你的页面不能逗留在浅色。

1. **禁止硬编码配色**。取色一律用 token：语义组 `--c-*`（面/文字/强调）与 `--dt-*`（DataTable 单元格类型）。SDK 帮你把 token 写进页面 `:root` 并同步 `.dark` class。
2. 推荐用 SDK 的 React 绑定订阅主题快照：

   ```ts
   import { useTheme } from '@datazen/extension-sdk/react';
   const { dark, tokens } = useTheme(); // 宿主每次切主题都会重渲
   ```
   非 React：`startThemeListener()`（SDK）或订阅 `datazen:theme-pack-changed` DOM 事件。
3. CSS 用 token 语义值 + 中性 fallback（仅供**握手前首帧**）：

   ```css
   :root { color-scheme: light dark; }
   .card {
     color: var(--c-fg, #111);
     background: var(--c-surface, transparent);
   }
   ```

**token 语义组**（宿主握手 `theme.apply` 时推出、SDK 写入页面；`--c-*` 是面/文字/强调，`--dt-*` 是 DataTable 单元格）：

- 面/文字：`--c-surface` `--c-surface-alt` `--c-surface-raised` `--c-surface-inset` `--c-edge` `--c-fg` `--c-fg-secondary` `--c-fg-muted`
- 强调/反馈：`--c-accent` `--c-success` `--c-warning` `--c-danger` `--c-query-run`
- 标题栏：`--c-titlebar` `--c-titlebar-fg` `--c-titlebar-fg-muted` `--c-titlebar-hover`
- DataTable 单元格：`--dt-null` `--dt-bool` `--dt-number` `--dt-datetime` `--dt-json` `--dt-text` `--dt-binary`

> 不要用字面颜色当常驻配色——只允许「快照到手前」的一瞬 fallback。完整示例见 `datazen.playground`（`index.html` + `assets/app.js` 的 `applyHostTheme()`）。

---

## 6. 主题扩展（只发主题不页面）

纯主题扩展只需 `contributes.themes`，可省略 `entry` 与 `permissions`：

```jsonc
{
  "id": "com.example.palette",
  "name": "Palette",
  "version": "1.0.0",
  "apiVersion": 2,
  "contributes": {
    "themes": [
      {
        "id": "soft",
        "name": "Soft",
        "tokensCss": "themes/soft/tokens.css",
        "modes": ["light", "dark"]
      }
    ]
  }
}
```

`tokens.css` 里定义要覆盖的 token。DataZen 自带一个纯主题示例 `community.slate-blue`，直接照着它的目录结构写即可（含 `chartsJson` / `iconsDir` 的 tab）。主题应用时 `url()` 引用的包内相对资产会自动被重写为 blob，无需专门声明字体文件。

```css
/* themes/soft/tokens.css */
:root  { --c-accent: #6366f1; … }   /* light 值 */
.dark  { --c-accent: #818cf8; … }   /* dark 值 */
```

> 只有 **light** 的主题请把变量写在 `:root`；只有 **dark** 写在 `.dark`；两者都要就都写。DataZen 在主窗口对主题也采用同一套 `--c-*`/`--dt-*` token。

---

## 7. 打包（可安装 zip）

```bash
cd com.example.bill-hud
zip -rqX ../com.example.bill-hud.zip . -x '.DS_Store'
```

**zip 顶层就是包内容**（`manifest.json` 一定在根，不要套一层外层目录）。不需要编译——零构建。

---

## 8. 一步一步：一个能跑的完整页面示例

下面是一个完整可跑的零构建页面（对应 **§1 的 `index.html`**），它握手、读连接、跑一条 `SELECT`、存次数、发通知，并消费宿主 token 保证主题一致。

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>Bill HUD</title>
    <style>
      /* 第 5 节铁律：token + 中性 fallback + color-scheme */
      :root { color-scheme: light dark; }
      body {
        font-family: var(--font-sans, system-ui, sans-serif);
        color: var(--c-fg, #111827);
        background: var(--c-surface, #ffffff);
      }
    </style>
  </head>
  <body>
    <h1>Bill HUD</h1>
    <div id="status">handshaking…</div>
    <button id="run">Run SELECT</button>
    <pre id="out">—</pre>
    <script type="module">
      import { createClient } from './assets/sdk.mjs'; // 把 SDK 打进你的包

      const dz = createClient();
      const out = document.getElementById('out');
      const st = document.getElementById('status');

      try {
        const ctx = await dz.ready();  // { apiVersion, locale, dark, tokens }
        // 若用 SDK 的 applyThemeSnapshot/useTheme（§5），这里会自动把 tokens
        // 写到 :root 并同步 .dark class；纯 JS 可调用 applyThemeSnapshot(ctx)。
        st.textContent = ctx.dark ? 'ready (dark)' : 'ready (light)';
      } catch (e) {
        st.textContent = '握手失败: ' + e.message;
      }

      document.getElementById('run').addEventListener('click', async () => {
        try {
          const conns = await dz.context.getConnections();
          const id = conns[0]?.id;
          if (!id) { out.textContent = '没有可用连接'; return; }
          const result = await dz.command.invoke({ connectionId: id, command: 'query', args: { sql: 'SELECT 1 AS one' } });
          out.textContent = JSON.stringify(result, null, 2);
        } catch (e) {
          out.textContent = 'error: ' + e.message;
        }
      });
    </script>
  </body>
</html>
```

> 说明：上面用了 `import ... from './assets/sdk.mjs'`（假定你把 SDK 也打包进你的 `assets/`）。真实开发建议用 bundler 把 `@datazen/extension-sdk` 一起打包；零构建的话把 SDK 源码或 dist 放进包内相对引用即可。主题应用请直接用 SDK 的 `applyThemeSnapshot` / `useTheme()`（§5），不要手搓。

---

## 9. 检查清单 / 常见坑

- ✔ 目录名 == `manifest.id`；`apiVersion = 2`；semver 版本号。
- ✔ 用到 `context/command/storage` 就声明对应权限；`ui.notify` 不用声明。
- ✔ 页面入口 `index.html` → 沙箱。任何 `fetch` / `<script src=http…>` 都会被宿主 CSP 拦下；**资产只能** 通过 `datazen://{pluginId}/{path}` 拿到。
- ✔ CSS 不写死主题色（见 §5）。
- ✔ 包 ≤ 50MB、≤ 2000 文件，SVG ≤ 256KB 无 `script`/事件属性/`javascript:`，无符号链接。
- ✔ 纯扩展省略 `entry`。
- 测试：直接把 zip/目录发给一个装了 DataZen 的人安装，开页面、切主题，手动过一遍就算「过」。所有决定都在宿主侧，本地没有跑起来的单元测试。

---

## 10. 卸载 / 重装行为

- **卸载**：删除该插件整个目录（**含用户存在 `storage.*` 里的数据**），确认弹窗会明说。
- **重装**：同 id 安装覆盖旧版，旧目录备份为 `{id}.old.bak`。

---

## 11. 交付前检查清单

1. 目录名、`id`、`apiVersion=2` 三者一致。
2. 权限仅为用到的能力，最小化。
3. 生产 token 配色，无写成死的主题色。
4. `zip` 顶层就是包内容。
5. 安装 → 开页面 → 切主题 → 卸载，全部手工过一遍。