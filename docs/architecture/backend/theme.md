# 运行时主题包

> [返回架构总览](../README.md)

与驱动插件**完全独立**：安装路径、注册表、生命周期均分离；不受 `plugins-registry.json`、`.plugins/` 或 `DATAZEN_PLUGINS` 影响。

## 安装路径

```
{appData}/themes/{packId}/
├── manifest.json
├── tokens.css          # 按 mode 的 CSS 变量（:root / .dark）
├── fonts.css           # 可选 @font-face
├── editor.json         # 可选 CodeMirror 色板覆盖
├── charts.json         # 可选图表调色板
└── icons/              # 语义 ID 或 db.<type> 图标
    ├── nav.settings.svg
    └── db.postgresql.svg
```

- ZIP 通过 `install_theme_pack_with_dialog` 解压到上述目录。
- 删除包目录或 `remove_theme_pack` 不影响驱动插件或 `.plugins/`。

## 包内容与校验

白名单扩展名：`.css`、`.json`、`.svg`、`.png`、`.webp`、`.woff2`、`.woff`。

拒绝：`.js`、`.mjs`、`.ts`、`.wasm`、`.ico`、`.icns`、路径穿越、超限体积/文件数。

| 模块 | 文件 | 职责 |
|------|------|------|
| 校验 | `src-tauri/src/theme/validate.rs` | manifest 解析、扩展名白名单、路径守卫、体积限制 |
| 安装 | `src-tauri/src/theme/install.rs` | ZIP 解压、原子写入 `{appData}/themes/{id}/` |
| 入口 | `src-tauri/src/theme/mod.rs` | 导出 `ThemeManifest`、`THEME_API_VERSION` |

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

详见 [持久化存储](store.md)。

## 与驱动插件对比

| | 驱动插件 | 主题包 |
|---|---------|--------|
| 安装 | 编译期 / `plugins-registry.json` | 运行时 `{appData}/themes/` |
| 注册 | `inventory` + `DB_REGISTRY` | 文件系统扫描 + manifest |
| 内容 | Rust crate + 前端 meta | 声明式 CSS/JSON/静态资源 |
| 失败回退 | 缺少驱动 = 无该连接类型 | 缺少/无效包 = 内置外观 |

## 相关文档

- 前端应用与 IconResolver：[../frontend/components.md#3-主题系统](../frontend/components.md#3-主题系统)
- 设计规格：[../../superpowers/specs/2026-08-08-runtime-theme-packs-design.md](../../superpowers/specs/2026-08-08-runtime-theme-packs-design.md)
- 测试样例：`fixtures/themes/community.fixture-dark/`
