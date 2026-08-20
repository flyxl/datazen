# 运行时主题包

> [返回架构总览](../README.md)

与驱动插件**完全独立**：安装路径、注册表、生命周期均分离；不受 `drivers-registry.json`、Git 驱动 clone 目录或 `DATAZEN_DRIVERS` 影响。

## 安装路径

```
{appData}/themes/{packId}/
├── manifest.json
├── tokens.css          # 按 mode 的 CSS 变量（:root / .dark；含可选 --dt-* DataTable 单元格色）
├── fonts.css           # 可选 @font-face
├── editor.json         # 可选 CodeMirror 色板覆盖
├── charts.json         # 可选图表调色板
└── icons/              # 语义 ID 或 db.<type> 图标（见 src/lib/iconIds.ts）
    ├── nav.connections.svg
    ├── nav.settings.svg
    └── db.postgresql.svg
```

- ZIP 通过 `install_theme_pack_with_dialog` 解压到上述目录。
- 删除包目录或 `remove_theme_pack` 不影响驱动插件或 `packages/drivers/`。

## 包内容与校验

白名单扩展名：`.css`、`.json`、`.svg`、`.png`、`.webp`、`.woff2`、`.woff`。

拒绝：`.js`、`.mjs`、`.ts`、`.wasm`、`.ico`、`.icns`、路径穿越、超限体积/文件数。

| 模块 | 文件 | 职责 |
|------|------|------|
| 校验 | `src-tauri/src/theme/validate.rs` | manifest 解析、扩展名白名单、路径守卫、体积限制 |
| 安装 | `src-tauri/src/theme/install.rs` | ZIP 解压、原子写入 `{appData}/themes/{id}/` |
| 入口 | `src-tauri/src/theme/mod.rs` | 导出 `ThemeManifest`、`THEME_API_VERSION` |

### tokens.css 契约

主题包应覆盖 Host `src/styles/themes.css` 中的语义变量（社区包单测强制 surface / cm 全集）：

- **Surface / text**：`--c-surface`、`--c-surface-alt`、`--c-surface-raised`、`--c-surface-inset`、`--c-edge`、`--c-fg`、`--c-fg-secondary`、`--c-fg-muted`、`--c-accent`、`--c-success`、`--c-warning`、`--c-danger`、`--c-titlebar`、`--c-titlebar-fg`、`--c-titlebar-fg-muted`、`--c-titlebar-hover`、`--c-query-run`
- **Fonts**：`--font-sans`、`--font-mono`、`--font-editor`
- **CodeMirror**：`--cm-*`（也可经 `editor.json` 覆盖）
- **DataTable 单元格色（推荐）**：`--dt-null`、`--dt-bool`、`--dt-number`、`--dt-datetime`、`--dt-json`、`--dt-text`

`--dt-*` 为可选覆盖：省略时使用 Host 默认。前端 `CellRenderer` 通过 Tailwind `text-dt-*`（`tailwind.config.ts` → `var(--dt-*)`）着色。社区样例包已全部定义这些变量。

Dark 包写在 `.dark { … }`；Light 包写在 `:root { … }`。

## IPC 命令

| 命令 | 说明 |
|------|------|
| `list_theme_packs` | 扫描 `{appData}/themes/`，返回已安装包摘要（无效目录跳过并 warn） |
| `install_theme_pack_with_dialog` | 原生文件对话框选 `.zip`，校验并安装 |
| `remove_theme_pack` | 删除包目录；若当前启用则清空 `settings.theme.packId` |
| `read_theme_pack_file` | 按相对路径读取包内文件（路径遍历防护） |

实现：`src-tauri/src/commands/theme.rs`。

## 设置持久化

`AppSettings.theme` 为嵌套结构（Rust `ThemePreference`，serde 兼容旧版扁平字符串）：

```json
{
  "theme": {
    "mode": "dark",
    "packId": "community.fixture-dark"
  }
}
```

- 迁移：`"theme": "dark"` → `{ "mode": "dark", "packId": null }`。
- `packId: null` 表示仅 Host 内置 token，不加载主题包。

主题包在 IPC 之后才注入 CSS，首屏无法读取 pack token。`syncWebviewBackgroundFromTokens` 把当前 `--c-surface` 经 `set_surface_background` 写入 `{appData}/surface-bg.json`。`surface-boot` plugin 的 `initialization_script` 在 **每个** webview（含 `tauri.conf.json` 创建的 main）HTML parse 前 bake 该 hex，并调用 `set_background_color`。子窗口 native `backgroundColor` 同样读这份缓存。缺失时回退 `#0f172a`。

详见 [持久化存储](store.md)。

## 与驱动插件对比

| | 驱动插件 | 主题包 |
|---|---------|--------|
| 安装 | 编译期 / `drivers-registry.json` | 运行时 `{appData}/themes/` |
| 注册 | `inventory` + `DB_REGISTRY` | 文件系统扫描 + manifest |
| 内容 | Rust crate + 前端 meta | 声明式 CSS/JSON/静态资源 |
| 失败回退 | 缺少驱动 = 无该连接类型 | 缺少/无效包 = 内置外观 |

## 相关文档

- 前端应用与 IconResolver：[../frontend/components.md#3-主题系统](../frontend/components.md#3-主题系统)
- 测试样例：`fixtures/themes/community.fixture-dark/`
