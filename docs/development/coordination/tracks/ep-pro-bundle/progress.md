# Track ep-pro-bundle: Pro Extension Standalone Bundle & Manifest Specification

> Status: **READY_FOR_TEST**
> Branch: `feature/ep-pro-bundle`
> Base branch: `feat/sql-editor-clean`
> LastHeartbeat: 2026-09-08T18:24:00+08:00
> Coder Commit: `190549b` (sql-editor-pro sub-repo)
> Tester Commit: `pending`

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
| `packages/pro-extensions/sql-editor-pro/vite.config.ts` (library mode + externals) | Done |
| `packages/pro-extensions/sql-editor-pro/manifest.json` specification | Done |
| `src/index.ts` activate(context)/deactivate lifecycle alignment | Done |
| `npx vite build` in sql-editor-pro succeeds and produces `dist/index.esm.js` | Done (83.7 kB single bundle) |
| `vitest run --config packages/pro-extensions/sql-editor-pro/vitest.config.ts` 18 files / 180 tests pass | Done |

---

## Coder Self-Verification

| Check | Result |
|---|---|
| `npx vite build` (packages/pro-extensions/sql-editor-pro) | PASS — `dist/index.esm.js` (83.7 kB), externals verified |
| `npx vitest run --config packages/pro-extensions/sql-editor-pro/vitest.config.ts` | PASS — 18 files, 180 tests |

---

## Tester Verification
*(待 Coder 完成后由 Tester 独立复测登记)*
