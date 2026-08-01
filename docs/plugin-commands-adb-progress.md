# 插件命令扩展 + Android SQLite 拉取 — 开发进度

> 跟踪 `feat/plugin-commands-adb` 分支上所有功能模块的开发和测试状态。
> 每完成一个功能模块后更新此文件。

## 总体进度

| # | 功能模块 | 状态 | 单元测试 | E2E 测试 | 提交 |
|---|---------|------|---------|---------|------|
| 1 | Plugin 命令基础设施（构建脚本 + plugin_init.rs 生成） | ✅ 已完成 | ✅ 3 pass | — | — |
| 2 | Kiwi 命令迁移到插件工程 | ✅ 已完成 | ✅ 2 pass | 🔲 | — |
| 3 | Android ADB 拉取 SQLite 数据库（后端） | 🔲 未开始 | 🔲 | 🔲 | — |
| 4 | Android ADB 拉取 SQLite 数据库（前端） | 🔲 未开始 | 🔲 | 🔲 | — |

## 状态说明

- 🔲 未开始
- 🔨 开发中
- ✅ 已完成
- ❌ 测试不通过（需修复）
- 🐛 有已知 Bug

## 详细记录

### 功能 1: Plugin 命令基础设施
- **完成时间**: 2026-08-02
- **变更**:
  - `plugins-registry.json` — 新增 `tauriPlugin` 字段（`id`, `initFn`, `commands`）
  - `scripts/resolve-plugins.mjs` — 新增 `generateRustPluginInit()` 函数，生成 `plugin_init.rs`；扩展 `generateFrontendRegistry()` 输出 `PLUGIN_COMMANDS`、`pluginInvoke()`、`hasPluginCommand()`
  - `src-tauri/src/plugin_init.rs`（自动生成）— `register_plugins()` 函数，按 cfg feature 条件注册插件 Tauri Plugin
  - `src-tauri/src/lib.rs` — 引入 `mod plugin_init`，在 builder 链中调用 `plugin_init::register_plugins(builder)`
  - `src/plugin-sdk/index.ts` — 导出 `pluginInvoke`、`hasPluginCommand`、`PluginCommandMeta`
- **单元测试**: 3 pass（plugin_init.rs 生成正确、generated.ts 包含命令、空插件模式正确）
- **编译验证**: Rust `cargo check` + TypeScript `tsc --noEmit` 均通过

### 功能 2: Kiwi 命令迁移到插件工程
- **完成时间**: 2026-08-02
- **变更**:
  - `.plugins/kiwi/Cargo.toml` — 新增 `tauri` 可选依赖，`tauri-plugin` feature
  - `.plugins/kiwi/src/commands.rs`（新建）— `login` 和 `list_instances` Tauri 命令
  - `.plugins/kiwi/src/lib.rs` — 新增 `pub fn init<R>() -> TauriPlugin<R>`，条件编译 `commands` 模块
  - `Cargo.toml`（workspace 根）— 添加 kiwi patch 到本地路径，添加 `.plugins/` 到 exclude
  - `src-tauri/src/commands/kiwi.rs` — 删除
  - `src-tauri/src/commands/mod.rs` — 移除 `mod kiwi` 和 `pub use kiwi::*`
  - `src-tauri/src/lib.rs` — 从 `generate_handler!` 中移除 `kiwi_login`、`kiwi_list_instances`
  - `src/components/connection/useConnectionForm.ts` — `handleKiwiLogin` 和 `loadKiwiInstances` 改用 `pluginInvoke('kiwi', ...)`
- **单元测试**: 2 pass（`test_login_command_compiles`、`test_list_instances_command_compiles`）
- **编译验证**: Rust `cargo check` (±kiwi) + TypeScript `tsc --noEmit` 均通过

---

*此文件随开发进度持续更新。*
