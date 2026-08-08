# 设计：运行时主题包（外观插件）

**日期：** 2026-08-08  
**状态：** 已批准  
**实现分支：** `feat/runtime-theme-packs`（worktree: `.worktrees/runtime-theme-packs`）

## 目标

1. 用户可从商店**运行时下载并启用**外观包（无需重新编译应用）。
2. **驱动插件与主题包完全独立**（安装路径、注册表、生命周期分离）。
3. 支持**可定制图标**：
   - 功能 UI 图标（语义 ID）
   - 数据库类型角标（`db.<databaseType>`）
   - 格式：**SVG / PNG / WebP**（见「图标资源格式」）
4. 支持主题包**定制字体**（UI 与等宽/编辑器族；见「字体」）。
5. **禁止**主题包修改操作系统 / 安装包 / 托盘等**应用图标**。

## 非目标

- 通过 `plugins-registry.json` / Cargo feature 做编译期主题包。
- 主题包内执行任意 JavaScript（v1）。
- 取代现有的 light / dark / system **模式**轴（主题包与模式组合使用）。
- 超出系统能力的 macOS 原生菜单外观定制。
- 为换肤而改动驱动协议 / IPC。

## 已批准决策

| 议题 | 选择 |
|------|------|
| 分发方式 | 运行时商店下载 → 安装到应用数据目录 |
| 与驱动关系 | 独立通道；不共享 registry |
| 包模型 | **声明式 ThemePack（方案 A）** |
| UI 图标 | 语义 ID → **SVG / PNG / WebP**（主题可覆盖） |
| DB 角标 | 驱动自带默认 → 主题有则覆盖（同样支持 SVG / PNG / WebP） |
| 字体 | **UI + 编辑器默认**：主题包提供 `@font-face` + CSS 变量；用户显式设置优先 |
| 应用图标 | 主题永久不可修改 |
| 代码执行 | v1 不允许主题 JS（仅 CSS + JSON + 图像/字体静态资源） |

## 图标解析顺序

### 功能 UI

```
主题包 icons[<semanticId>]  →  Host 内置 Lucide（或现有组件）  →  空占位
```

语义 ID 为 Host 稳定契约（示例；完整表由 Host 文档维护）：

- `nav.settings`、`nav.connections`、`query.run`、`query.stop`、`ai.chat`、…

### 数据库类型角标

```
主题包 icons["db." + databaseType]  →  驱动自带默认图标  →  Host 占位（shortLabel + 色块）
```

- **驱动**：在前端 meta / 资源中自带默认 SVG（或等价资源）。
- **主题包**：可选择性覆盖 `db.postgresql`、`db.kiwi`、`db.superset` 等。
- 无主题美术资源的新驱动仍使用驱动默认图标。

### 禁止项

任何指向应用 / bundle / 托盘 / `.icns` / `.ico` 的包字段，在校验时忽略并拒绝。

### 图标资源格式

同一语义 ID / `db.<type>` 可提供多种扩展名；Host 按优先级选用**一个**文件：

```
.svg  →  .webp  →  .png
```

| 格式 | 用途 | 说明 |
|------|------|------|
| **SVG** | 首选 | 可跟随 `currentColor` / CSS 着色（若资源如此制作） |
| **WebP** | 等价位图 | 需约定基准尺寸（建议 20×20 逻辑像素，可带 `@2x` 命名约定） |
| **PNG** | 等价位图 | 同上；不保证可像 SVG 一样染色 |
| **ICO / ICNS** | 禁止 | 属于应用图标范畴 |

驱动默认图标同样允许 SVG / PNG / WebP，解析优先级一致。

## 字体

**支持范围（已确认）：UI + 编辑器默认。** 主题包可同时设定界面无衬线族与 SQL/代码编辑器等宽默认族；不取代用户在设置中的显式选择。

### 包内结构

```
fonts/
  UISans.woff2
  UIMono.woff2
tokens.css          # 含 @font-face 与 --font-sans / --font-mono / --font-editor 等
```

或单独 `fonts.css`，由 Host 与 `tokens.css` 一并注入。

### 契约（v1）

| CSS 变量 | 用途 |
|----------|------|
| `--font-sans` | 通用 UI 文案 |
| `--font-mono` | 等宽场景（表格、日志等） |
| `--font-editor` | SQL / CodeMirror 编辑器默认族（未设置 `editorFontFamily` 时生效） |

`editor.json` 可不含字体字段；编辑器默认字体走 CSS 变量，与现有设置项 `editorFontFamily` / `editorFontSize` 配合。

### 优先级

```
用户设置中已保存的 editorFontFamily /（若有）UI 字体偏好
  → 当前启用主题包的 --font-sans / --font-mono / --font-editor
  → Host 内置默认栈
```

即：**用户显式字体设置优先于主题包**；用户未改字体时，启用主题即同时改变 UI 与编辑器外观。

### 安全与格式

- 字体白名单：`.woff2`（首选）、`.woff`；v1 默认**不收** `.ttf` / `.otf` / `.eot`（体积与兼容性），可后续放开。
- `@font-face` 的 `src` 仅允许包内相对路径（经 Host 解析为本地/`blob:`），禁止远程 URL。
- 限制字体文件总大小。

## 包格式

```
{packId}/
  manifest.json
  tokens.css          # 覆盖 --c-*、--font-*（及文档化的别名）；可含 @font-face
  fonts/              # 可选：.woff2 / .woff
  icons/
    nav.settings.svg  # 或 .webp / .png
    query.run.svg
    db.postgresql.svg
    db.kiwi.webp
    …
  editor.json         # 可选：CodeMirror 语法色
  charts.json         # 可选：图表系列色板
  preview.png         # 可选：商店 / 设置预览图
```

### `manifest.json`（v1）

```json
{
  "id": "community.dracula",
  "name": "Dracula",
  "version": "1.0.0",
  "apiVersion": 1,
  "modes": ["dark"],
  "author": "…",
  "description": "…"
}
```

- `apiVersion`：Host 拒绝不兼容的包。
- `modes`：该包提供 token 的外观模式（`light` | `dark`；可同时包含）。

## Host 架构

```
商店下载
    → 校验 zip（大小、扩展名白名单、无路径穿越、无 .js/.wasm）
    → 解压到 {appData}/themes/{id}/
ThemeService
    → listInstalled() / enable(id) / disable()
    → apply(mode × pack)：
         从 tokens.css（及 fonts.css）注入 <style id="datazen-theme-pack">
         向 IconResolver 注册图标映射（SVG / WebP / PNG）
         若存在则重配 CodeMirror + 图表色板
Settings
    → mode: light | dark | system  （已有）
    → packId: string | null        （新增；null = 内置默认）
    → editorFontFamily 等用户字体设置优先于主题
```

### 与驱动插件的独立性

| | 驱动 | 主题包 |
|--|------|--------|
| 安装 | 编译期 / `plugins-registry` | 运行时 `{appData}/themes` |
| 代码 | Rust + UI 模块 | 仅 CSS / SVG·PNG·WebP / JSON / WOFF2 字体 |
| 发现 | `generated.ts` / inventory | `ThemeService` 扫描 + 设置 |
| 失败 | 缺少驱动 = 无该连接类型 | 缺少主题 = 回退内置外观 |

主题包不得依赖驱动 crate，也不得要求 `DATAZEN_PLUGINS`。驱动不得依赖主题包才能工作。

## 前置条件（丰富主题前的 Host 加固）

否则主题包只能部分生效：

1. 统一语义 CSS token；状态色映射到 `--c-success` / `--c-danger` / …  
2. CodeMirror / SqlCodeBlock 从 CSS 变量或 `editor.json` 契约读色。  
3. 图表 / ER 变量对齐 `--c-*`；可选 `charts.json`。  
4. `applyTheme()` 支持 `mode × pack`；同步 webview 背景（不是 OS 应用图标）。  
5. 引入按语义 ID 的 `IconResolver`（工具栏 / 导航；支持 SVG/WebP/PNG）；DB 角标组件使用解析器 + 驱动默认。
6. UI / 等宽 / 编辑器默认字体走 `--font-sans` / `--font-mono` / `--font-editor`；用户显式设置优先。

分期：

1. **基础** — token + IconResolver + 驱动默认 SVG 挂钩  
2. **本地包** — 从文件 / 数据目录安装，设置页选择  
3. **商店** — 浏览 / 下载 / 更新（独立产品面）

## 安全

- 白名单：`.css`、`.svg`、`.png`、`.webp`、`.json`、`.woff2`、`.woff`。
- 拒绝：`.js`、`.mjs`、`.ts`、`.wasm`、`.ico`、`.icns`、原生二进制、逃逸根目录的符号链接。
- 限制解压体积与文件数量（含字体总大小）。
- 净化 SVG（v1 无 `<script>`、无外链请求）。
- CSP：优先注入本地文件内联样式 / `blob:` 字体；默认不允许远程 stylesheet 或远程字体 URL。

## 设置 / 持久化

扩展设置（前端 + Rust `AppSettings`）：

```ts
theme: {
  mode: 'light' | 'dark' | 'system';  // 从扁平 theme 字符串迁移
  packId: string | null;
}
```

迁移：既有 `theme: 'dark'` → `{ mode: 'dark', packId: null }`。

## 测试

- 单元：图标解析顺序（主题 → 驱动 → 占位）。
- 单元：包校验（拒绝 JS、路径穿越、超限体积）。
- 单元：token 应用会切换 `:root` / `.dark` 下的 CSS 变量。
- E2E（后续）：从 fixture zip 安装包、启用，断言语义表面 + 至少一个 DB 角标覆盖。

## 成功标准

- 用户无需重新编译即可安装主题包。
- 启用后改变 CSS token / 字体变量，并覆盖所列语义 / `db.*` 图标（含 PNG/WebP）。
- 无主题美术的驱动仍显示自带默认图标。
- 任意主题包下应用图标不变；用户显式字体设置不被主题静默覆盖。
- 禁用 / 删除主题包恢复内置外观，且不影响驱动安装。

## 后续（v1 之后）

- 商店 CDN、签名与更新通道。
- 按需支持按模式区分的图标变体（如 `icons/dark/nav.settings.svg`）。
- 与 Host `apiVersion` 对齐的社区包 lint CLI。
