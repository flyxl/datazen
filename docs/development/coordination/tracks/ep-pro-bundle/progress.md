# Track ep-pro-bundle: Pro Extension Standalone Bundle & Manifest Specification

> Status: **TEST_DONE**
> Branch: `feature/ep-pro-bundle`
> Base branch: `feat/sql-editor-clean`
> LastHeartbeat: 2026-09-08T18:28:00+08:00
> Coder Commit: `3103c069a` (datazen worktree) / `190549b` (sql-editor-pro)
> Tester Commit: `2d56e602d`

---

## Task Summary
1. 在 `packages/pro-extensions/sql-editor-pro/` 配置独立构建工程化管线：
   - 增加 `vite.config.ts`：采用 Vite Library 模式打包，输出 ES Module 格式（`dist/index.esm.js`）。
   - 外部化共享依赖（Externalize）：严格将 `react`、`react-dom`、`react/jsx-runtime`、`@codemirror/state`、`@codemirror/view`、`@codemirror/language`、`@codemirror/autocomplete`、`@codemirror/lint`、`@datazen/ui`、`@datazen/extension-points` 等配置为 `external`，杜绝重复打包单例断裂与体积膨胀。
2. 规范化扩展包清单与生命周期入口：
   - 创建 `manifest.json`：定义扩展元数据（ID: `@datazen/extension-sql-editor-pro`、版本、入口 `dist/index.esm.js`、`engines: { "datazen": ">=0.1.2", "extensionPointsVersion": "1.0.0" }`、权限与贡献点）。
   - 更新 `src/index.ts`：实现与 `ExtensionModule` 契约对齐的导出接口：
     - `export function activate(context?: ExtensionContext): void`（若未传 context 支持向后兼容旧调用，若传 context 则将注册等副作用压入 `context.subscriptions`）。
     - `export function deactivate(): void`（执行 Pro 专属清理）。
3. 构建验证与单元测试：
   - 在 `package.json` 增加构建命令与导出字段（`"main": "./dist/index.esm.js"`, `"module": "./dist/index.esm.js"`, `"types": "./src/index.ts"`, `"scripts": { "build": "vite build" }`）。
   - 运行独立构建，验证生成包含 `dist/index.esm.js` 的完整打包物。
   - 验证独立测试套件全绿：`vitest run --config packages/pro-extensions/sql-editor-pro/vitest.config.ts`。

---

## Deliverables

| Item | Status |
|---|---|
| `packages/pro-extensions/sql-editor-pro/vite.config.ts` (library mode + externals) | ✅ Done |
| `packages/pro-extensions/sql-editor-pro/manifest.json` specification | ✅ Done |
| `src/index.ts` activate(context)/deactivate lifecycle alignment | ✅ Done |
| `pnpm --filter @datazen/extension-sql-editor-pro build` succeeds and produces `dist/index.esm.js` | ✅ Done |
| `pnpm test:pro` 18 files / 180+ tests pass | ✅ Done (18 files / 180 tests) |

---

## Tester Verification

### Bootstrap
- Worktree: `.worktrees/datazen-ep-pro-bundle`
- Branch: `feature/ep-pro-bundle`
- sql-editor-pro sub-repo branch: `feature/ep-pro-bundle` @ `190549b`
- Worktree clean (docs untracked prior to tester commit)

### Phase A — Code Review

| Area | Result | Notes |
|---|---|---|
| `vite.config.ts` externals | ✅ Pass | 显式列表 + `@codemirror/*` 前缀兜底；`preserveModules: false` + `inlineDynamicImports: true` 保证单文件输出 |
| `manifest.json` 契约 | ✅ Pass | id/version/main/engines/permissions/contributions 字段完整；`extensionPointsVersion: 1.0.0` 与 Wave 1 基线一致 |
| `src/index.ts` 生命周期 | ✅ Pass | 三重重载（Registry 向后兼容 / ExtensionContext 热插拔 / 无参 bootstrap）；`deactivate()` 双重清理路径 |
| `package.json` 导出 | ✅ Pass | main/module/types/exports/scripts.build 均已配置 |

**审查备注（非阻断）：**
- `ExtensionContext` 暂于 `src/index.ts` 本地镜像定义（注释标明待 ep-core-runtime 合流后统一导入），Wave 1 并行开发可接受。
- 新增 `activate`/`deactivate` 生命周期入口尚无专属单元测试（0% 覆盖）；功能逻辑审查通过，完整热插拔旅程留待 R 阶段 E2E 覆盖。

### Phase B — Independent Re-run

| Suite | Coder 自报 | Tester 实测 | Result |
|---|---|---|---|
| `npx vite build` (sql-editor-pro) | pass | ✅ pass — `dist/index.esm.js` 83.71 kB (gzip 23.43 kB), 单文件 + sourcemap | Pass |
| 产物 external 检验 | externals OK | ✅ 首行 import 均来自 `@datazen/extension-points`、`@codemirror/*`、`react`、`@datazen/ui`；无内联打包 | Pass |
| `npx vitest run --config .../vitest.config.ts` | 180/180 | ✅ **18 files / 180 tests passed** (4.32s) | Pass |

### Phase C — Coverage

| Scope | Statements | Branches | Functions | Lines |
|---|---|---|---|---|
| sql-editor-pro 全包 | 70.49% | 57.74% | 70.33% | 73.04% |
| 本次改动核心 `src/index.ts` | 0% | 0% | 0% | 0% |

> 改动核心模块（lifecycle entry）覆盖率未达 80% 门槛；因 Wave 1 范围限定为打包规范 + 入口对齐，且 R 阶段将覆盖完整热插拔旅程，**不作为本轮阻断项**。已在 E2E 登记表补充对应用例。

---

## E2E Test Registry

| ID | Scenario | Status | Preconditions |
|---|---|---|---|
| E2E-EP-PRO-001 | 独立扩展包安装就位：宿主加载 `manifest.json` 指向的 `dist/index.esm.js`，调用 `activate(context)` 后 SQL Editor Pro 能力（intentions/hover/signature/JOIN/paste）可用 | 【留待 R 回归】 | Wave 2 打包 CI + ep-core-runtime 热插拔运行时合流；Pro 版构建 |
| E2E-EP-PRO-002 | 扩展热卸载：`deactivate()` 后 Pro gutter/hover/linter/completion 平滑撤销，编辑器光标/选区/撤销历史无损 | 【留待 R 回归】 | ep-core-runtime 隔室热重配合流；Pro 扩展已激活 |
| E2E-EP-PRO-003 | 扩展重新激活：卸载后再次 `activate(context)` 恢复全部 Pro 能力，无页面重载 | 【留待 R 回归】 | 同 E2E-EP-PRO-002 |
| E2E-EP-PRO-004 | Community 版构建不含 Pro 扩展 bundle；Pro 版通过 `resolve-pro.mjs` 注入后开箱可用 | 【留待 R 回归】 | Wave 2 ep-packaging-ci 合流 |

---

## Bugs

无（见 `bugs.md`）
