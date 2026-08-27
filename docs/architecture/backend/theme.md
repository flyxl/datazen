# 运行时主题与首屏背景

> [返回架构总览](../README.md)

## 扩展主题（当前路径）

主题由**运行时扩展**的 `contributes.themes[]` 贡献，安装于 `{appData}/plugins/{publisher}.{name}/`。Settings「外观」仅列出已启用扩展的主题；`settings.theme.packId` 持久化为 `plugin:{extensionId}:{themeId}`。

应用管线见 [extensions.md](extensions.md) 与前端 `src/lib/themePackApply.ts`（`applyPluginTheme` / `encodePluginThemePackId`）。

## 首屏背景缓存

扩展主题在 IPC 之后才注入 CSS，首屏无法读取 pack token。`syncWebviewBackgroundFromTokens` 把当前 `--c-surface` 经 `set_surface_background` 写入 `{appData}/surface-bg.json`。`surface-boot` plugin 的 `initialization_script` 在 **每个** webview（含 `tauri.conf.json` 创建的 main）HTML parse 前 bake 该 hex，并调用 `set_background_color`。子窗口 native `backgroundColor` 同样读这份缓存。缺失时回退 `#0f172a`。

| 模块 | 路径 | 职责 |
|------|------|------|
| 缓存 | `src-tauri/src/theme/surface_bg.rs` | 读写 `{appData}/surface-bg.json` |
| IPC | `src-tauri/src/commands/theme.rs` | `set_surface_background` |
| 前端 | `src/lib/surfaceBgCache.ts` | 解析 `--c-surface` 并 invoke IPC |

详见 [持久化存储](store.md)。

## 相关文档

- 扩展主题贡献：[extensions.md](extensions.md)
- 前端应用与 IconResolver：[../frontend/components.md#3-主题系统](../frontend/components.md#3-主题系统)
