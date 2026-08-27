# Plugin 命名统一改造 — 进度跟踪

> 启动时间：2026-08-27
> 计划文档：`docs/development/plugin-naming-refactor.md`

## 功能总览表

| 编号 | 功能 | 状态 | 编码 commit | 测试 commit |
|------|------|------|------------|------------|
| F1 | Track A：Cargo feature / plugin_init → driver | 已完成 | 377c23e5 | deab51ee |
| F2 | Track B：plugins/ → extensions/ + 前端重命名 | 已完成 | a7251e0f | b5e93a5c |
| F3 | Track C：Legacy ThemePack 清理 | 已完成 | 43db00a6 | 93a5f822 |

## Bug 台账

| Bug ID | 所属功能 | 描述 | 状态 | 记录时间 | 验证记录 |
|--------|---------|------|------|---------|---------|
| — | — | — | — | — | — |

## 测试约定

- Rust：`cargo test -p datazen --lib`
- 前端：`npx vitest run`
- 类型：`tsc --noEmit`
- 覆盖率：改动文件 ≥80% 实测
- E2E：功能级测试轮只登记用例，留待 R 回归

## F1：Track A — Cargo feature / plugin_init → driver

**Status:** ✅ COMPLETED  
**Branch:** `feature/rename-driver`（已合并 main）  
**F1 测试代理：** 2026-08-27 独立复验（main @ ba6028e6 基线）

### 测试结果

```
node scripts/resolve-drivers.mjs --codegen-only --drivers=basic  exit 0
  → features: driver-postgres,driver-mysql,driver-sqlite,driver-redis
  → 生成 .driver-features.json / driver_init.rs

cargo check -p datazen                    exit 0（66s；31 warnings，均为既有 dead_code/unused_import + codegen-only 未注入 Cargo.toml 导致的 unexpected-cfg driver-*，非失败）

npx vitest run scripts/__tests__/         10 files, 78 passed, 0 failed（1.06s）
```

### grep 验收（对照计划 §Track A）

| 检查项 | 结果 |
|--------|------|
| `plugin-` in `src-tauri/Cargo.toml`（feature 名） | ✅ 无（仅 `tauri-plugin-*` 外部 crate） |
| `plugin_init` in `src-tauri/src/`、`scripts/` | ✅ 无 |
| `cfg(feature = "plugin-")` in `src-tauri/src/` | ✅ 无 |
| `driver_init.rs` 存在 / `plugin_init.rs` 不存在 | ✅ |
| `drivers-registry.json` feature 前缀 | ✅ 全部 `driver-*` |
| `<<driver-*>>` markers（root + src-tauri Cargo.toml） | ✅ |
| `.gitignore` → `driver_init.rs` / `.driver-features.json` | ✅ |
| `lib.rs` → `mod driver_init` / `register_drivers` | ✅ |
| `resolve-drivers.mjs` 生成 `driver_init.rs` / `.driver-features.json` | ✅ |
| `scripts/__tests__/` 无 `plugin-postgres` 等旧断言 | ✅ |
| CI（`.github/workflows/ci.yml`）读 `.driver-features.json` | ✅ |

### 遗留（by design / 非缺陷）

- `driver-file-stash.mjs` 脚本名与 `.driver-file-stash/` 目录保持（stash 机制，非 driver feature 命名）
- 工作区未跟踪 `.plugin-features.json`（旧格式本地残留；codegen 已写 `.driver-features.json`，无代码引用旧文件名）

### Bug 台账（F1）

无

## F2：Track B — plugins/ → extensions/ + 前端重命名

**Status:** ✅ COMPLETED（F2 独立复验通过）  
**Branch:** `main`（已合并 `feature/rename-extension`）  
**编码 commit:** `a7251e0f`（`refactor: rename runtime plugins module to extensions`）  
**测试 commit:** （本 commit）

### 范围完整性（F2 复验 grep）

| 检查项 | 结果 |
|--------|------|
| `src-tauri/src/plugins/` 不存在 | ✅ 已迁移为 `extensions/`（8 源文件） |
| `PluginManager` / `mod plugins` / `commands::plugins`（Rust） | ✅ 无匹配 |
| `usePluginStore` / `pluginCommands` / `pluginStore` / `commands/plugins`（TS，排除 `src/plugins/generated*`） | ✅ 无匹配 |
| `list_plugins` / `PluginSummary` / `LoadedPlugin`（源码） | ✅ 无残留（E2E `plugins.spec.ts` 局部类型别名 + `list_extensions` invoke 为预期） |

### 范围摘要

| 层 | 变更 |
|----|------|
| Rust | `src-tauri/src/plugins/` → `extensions/`；`ExtensionManager` / `ExtensionManifest`；`commands/extensions.rs` + IPC `list_extensions` 等 |
| 前端 | `extensionCommands` / `useExtensionStore` / `types/extension.ts`；`windows/extensions/`；`ExtensionPageShell` |
| E2E | `e2e/specs/plugins.spec.ts` IPC 命令名更新 |
| 文档 | `AGENTS.md`；`docs/architecture/backend/extensions.md` |

### 测试结果（F2 独立复验，2026-08-27）

```
node scripts/resolve-drivers.mjs --codegen-only --drivers=basic   OK
cargo test -p datazen --lib                                       1140 passed, 0 failed, 2 ignored
npx vitest run（extension 核心 10 文件）                          125 passed, 0 failed
npx vitest run（extension 关联 15 文件）                          175 passed, 0 failed
npx vitest run（全量）                                            2026 passed, 7 failed（4 files）
tsc --noEmit                                                      OK
```

**全量 7 fail 明细（与 main 基线相同，非 Track B 引入）：**

1. `WorkflowPanel.test.tsx` — `shows workflow error and failed execution via history`
2. `BatchExportDialog.test.tsx` — `shows error when export fails`
3. `ConnectionNavigatorTree.test.tsx` — `wires optional toolbar buttons...`（4 条同文件）
4. `ContentView.test.tsx` — `shows toolbar buttons for SQL connections`

### Bug 台账（F2）

无新增 bug。

### 不改动（by design）

- `{appData}/plugins/` 磁盘目录名
- `plugins:changed` / `plugins:open-page` 事件字符串
- `src/plugins/generated.ts`（编译时 driver codegen）
- i18n key 前缀 `plugins.page.*`（用户可见文案）

## F3：Track C — Legacy ThemePack 清理

**Status:** ✅ COMPLETED（F3 独立复验通过）  
**Branch:** `feature/track-c`（已合并 main）  
**编码 commit:** `43db00a6`  
**测试 commit:** `93a5f822`

### 范围摘要

| 层 | 变更 |
|----|------|
| Rust | `validate_theme_zip_path` → `util/theme_zip.rs`；删除 `theme/validate.rs`、`theme/install.rs`；`theme/` 仅 `mod.rs` + `surface_bg.rs`；`commands/theme.rs` 仅保留 `set_surface_background` |
| 前端 | 删除 `commands/theme.ts`、`types/themePack.ts`、`ThemePackSection.tsx`；`themePackApply.ts` 仅 extension 路径 |
| i18n | 删除 en/zh-CN 中 ThemePack 专属 key |
| 文档 | `theme.md`、`components.md`、`commands.md`、`extensions.md`、`AGENTS.md` |

### 测试结果（F3 独立复验）

```
cargo test -p datazen --lib     1113 passed, 0 failed, 2 ignored
npx vitest run                  2018 passed, 7 failed（与 main 基线相同）
tsc --noEmit                    OK
theme 相关 vitest               32 passed
```

### Bug 台账（F3）

无

## F4：脚本命名收尾（driver inject / ACL）

**Status:** ✅ COMPLETED  
**Branch:** `main`

### 范围摘要

| 变更 | 说明 |
|------|------|
| `with-plugin-inject.mjs` → `with-driver-inject.mjs` | 含 `runWithDriverInject` / `planDriverInjectLifecycle` |
| `DATAZEN_PLUGIN_INJECT_ACTIVE` → `DATAZEN_DRIVER_INJECT_ACTIVE` | 嵌套 inject 环境变量 |
| `PLUGIN_ACL_IDS` → `DRIVER_ACL_IDS` | `driver-deinject.mjs` / `driver-stash-precommit.mjs` |
| 引用更新 | `package.json`、CI release、e2e 脚本、文档 |

### 测试结果

```
npx vitest run scripts/__tests__/with-driver-inject.test.ts scripts/__tests__/stash-workflow.test.ts scripts/__tests__/driver-stash-precommit.test.ts
  3 files, 34 passed, 0 failed
```

### 不改动（by design）

- `src/plugins/generated.ts`（driver codegen 路径）
- Git 驱动 crate 名 `datazen-plugin-*`（外部仓库）
- `{appData}/plugins/` 磁盘目录、`plugins:changed` 事件字符串

