# Plugin 命名统一改造 — 进度跟踪

> 启动时间：2026-08-27
> 计划文档：`docs/development/plugin-naming-refactor.md`

## 功能总览表

| 编号 | 功能 | 状态 | 编码 commit | 测试 commit |
|------|------|------|------------|------------|
| F1 | Track A：Cargo feature / plugin_init → driver | 已完成 | 377c23e5 | 377c23e5 |
| F2 | Track B：plugins/ → extensions/ + 前端重命名 | 已完成 | （本分支 commit） | （本分支 commit） |
| F3 | Track C：Legacy ThemePack 清理 | 未开始 | — | — |

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

### 测试结果

```
scripts/__tests__/  78 passed
cargo check -p datazen --features driver-*  OK
```

### 遗留

- `plugin-file-stash.mjs` 脚本名与 `.plugin-file-stash/` 目录保持（stash 机制，非 driver 命名）

## F2：Track B — plugins/ → extensions/ + 前端重命名

**Status:** ✅ COMPLETED  
**Branch:** `feature/rename-extension`

### 范围摘要

| 层 | 变更 |
|----|------|
| Rust | `src-tauri/src/plugins/` → `extensions/`；`ExtensionManager` / `ExtensionManifest`；`commands/extensions.rs` + IPC `list_extensions` 等 |
| 前端 | `extensionCommands` / `useExtensionStore` / `types/extension.ts`；`windows/extensions/`；`ExtensionPageShell` |
| E2E | `e2e/specs/plugins.spec.ts` IPC 命令名更新 |
| 文档 | `AGENTS.md`；`docs/architecture/backend/extensions.md` |

### 测试结果

```
cargo test -p datazen --lib     1140 passed, 0 failed, 2 ignored
npx vitest run (extension 相关) 139 passed
npx vitest run (全量)           2026 passed, 7 failed（与 main 基线相同，非本轨引入）
tsc --noEmit                    OK
```

### 不改动（by design）

- `{appData}/plugins/` 磁盘目录名
- `plugins:changed` / `plugins:open-page` 事件字符串
- `src/plugins/generated.ts`（编译时 driver codegen）
- i18n key 前缀 `plugins.page.*`（用户可见文案）

## F3：Track C — Legacy ThemePack 清理

### 范围
见计划文档 §Track C（低优先级，未启动）

### 测试结果
（未开始）
