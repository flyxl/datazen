# DataZen Extensions

运行时插件（UI 页面 + 主题）的**源码包目录**。每个子目录是一个完整、可直接安装的 extension 包，遵循统一插件系统规范（PRD：[ui-plugins.md](../../docs/prd/ui-plugins.md)，技术方案：[ui-plugins-implementation.md](../../docs/prd/ui-plugins-implementation.md) §2.2）。

> 历史注记：本目录取代 `packages/themes/`（旧 v1 ThemePack 源码树）。社区主题 `community.slate-blue` 已按新规范改造为纯主题 extension；`fixtures/themes/community.fixture-dark` 保留原位，仅服务遗留 theme 模块的单测。

## 包结构

```text
packages/extensions/<publisher>.<name>/
├── manifest.json          # 必需；apiVersion 必须等于宿主 PLUGIN_API_VERSION (=2)
├── index.html             # 有 contributes.pages 时必需
├── assets/                # 图标 / 脚本 / 样式等
└── themes/<theme-id>/     # 有 contributes.themes 时：tokens.css（可含 url() 相对资产）
```

`manifest.json` 要点：

```jsonc
{
  "id": "<publisher>.<name>",      // ^[a-z0-9][a-z0-9-]{0,30}\.[a-z][a-z0-9-]{1,31}$，且必须等于所在目录名
  "name": "显示名",
  "version": "1.0.0",              // semver
  "apiVersion": 2,
  "entry": "index.html",           // 仅 pages 需要；纯主题插件可省略
  "contributes": {
    "pages":  [{ "id": "page-id", "title": "…", "icon": "assets/icon.svg", "showIn": "workspace" }],
    "themes": [{ "id": "theme-id", "name": "…", "tokensCss": "themes/theme-id/tokens.css", "modes": ["dark"] }]
  },
  "permissions": ["context:connections", "command:invoke", "storage:local", "ui:notify"]
}
```

文件白名单：`html/js/mjs/css/json/svg/png/webp/woff2/woff`；SVG ≤256KB 且禁止 `<script>`/事件处理器/`javascript:` URL；整包 ≤50MB、≤2000 文件；拒绝符号链接与路径穿越。

## 现有包

| Id | 类型 | 内容 |
|----|------|------|
| `community.slate-blue` | 纯主题 | Slate & sky-blue 主题（light + dark），由旧 ThemePack 迁移 |
| `datazen.playground` | 页面 + 主题 | 全功能示例：桥接握手、context.getConnections/getActiveConnection、command.invoke（query）、storage KV 计数器、ui.notify、实时主题快照，附 Playground Night 暗色主题 |

## 安装测试

1. 启动 DataZen（`pnpm tauri:dev`）
2. 左侧边栏 → **插件** → **安装插件…**
3. 在弹窗中粘贴包的**绝对目录路径**（如 `…/DataZen/packages/extensions/datazen.playground`），或先打 zip：

   ```bash
   cd packages/extensions/datazen.playground && zip -r ../../datazen.playground.zip .
   ```

4. 预览确认（名称/版本/权限清单）→ Install → 卡片出现即成功
5. 测试页面：左侧边栏 → **Workspace** → 点击 *Extension Playground* 打开 Tab
6. 测试主题：设置 → **外观** → 选择 *Playground Night* 或 *Slate Blue*

> 注意：安装是**拷贝语义**——修改源码后需重新安装才能生效（同 id 重装会覆盖并备份旧包为 `{id}.old.bak`）。停用/卸载在插件管理页操作。

## 主题完整能力（legacy ThemePack 对齐）

`contributes.themes[]` 除必需字段外支持三个**可选**字段，完整保留旧 ThemePack 的全部能力：

| 字段 | 说明 | 旧格式对应 |
|------|------|-----------|
| `editorJson` | CodeMirror 配色覆盖（键集见 `themeEditorColors.ts`） | 根目录 `editor.json` |
| `chartsJson` | 图表系列配色（`{seriesKey: [color...]}`） | 根目录 `charts.json` |
| `iconsDir` | 语义图标覆盖目录，文件名 `<semanticId>.svg|.webp|.png`（id 见 `src/lib/iconIds.ts`，含驱动 `db.*` 图标） | 根目录 `icons/` |

示例（`community.slate-blue`）：

```jsonc
"themes": [{
  "id": "slate-blue",
  "tokensCss": "themes/slate-blue/tokens.css",
  "modes": ["light", "dark"],
  "chartsJson": "themes/slate-blue/charts.json",
  "iconsDir": "themes/slate-blue/icons"
}]
```

语义与旧管线一致：三者均**软失败**——单个资产损坏只降级对应切片并在 console 告警，不影响主题整体应用；切换/清除主题时由 `resetPackState()` 统一回收。字体沿用 tokens.css 内 `url(...)` 相对引用（自动 blob 重写），无需独立 fonts.css 字段。

## 守护测试

- Rust：`cargo test -p datazen --lib plugins::fixture_tests` —— 所有包过 manifest 规则全集（规则 1–7）
- Vitest：`src/lib/__tests__/extensionThemes.test.ts` —— 结构、token 契约、README 缺口说明防漂移

## SDK

正式开发建议使用 [`packages/ui-plugin-sdk`](../ui-plugin-sdk/)（`createClient()` 类型化 RPC、`useTheme()`、`theme.css` 基础控件）；本目录示例刻意零构建，便于直接安装验证。
