# Track ep-packaging-ci: Standalone Packager, Tauri Builtin Bundling & GitHub Actions Release

> Status: **TEST_DONE**
> Branch: `feature/ep-packaging-ci`
> Base branch: `feat/sql-editor-clean`
> LastHeartbeat: 2026-09-08T18:39:00+08:00
> Coder Commit: `56d62ae44`
> Tester Commit: `pending`

---

## Task Summary
1. 扩展打包工具 `scripts/pack-ep.mjs`：
   - 自动编译 `packages/pro-extensions/sql-editor-pro`（Library 模式）。
   - 调用签名工具对产物进行数字签名（生成 `signature.sig`）。
   - 支持两种输出模式：
     - 打包成标准 `.dzx` (ZIP 归档，包含 `manifest.json`, `dist/index.esm.js`, `signature.sig`, `locales/`)，用于独立分发与在线更新。
     - 解压/同步至 Tauri 静态资源目录 `src-tauri/resources/builtin-ep/sql-editor-pro/`。
2. 宿主编译与预置集成改造：
   - 改造 `scripts/resolve-pro.mjs`：
     - 当 `--edition=pro` 时，调用 `pack-ep.mjs` 将已签名扩展产物同步到 `src-tauri/resources/builtin-ep/sql-editor-pro/`。
     - `writeProCodegen` 不再静态 `import @datazen/extension-sql-editor-pro`，而是通过运行时微内核自动检测 `builtin-ep` 或由 `HostExtensionLoader` 激活。
     - 宿主 Vite 打包不再将 Pro 源码打包进 `index.html` 的单个 chunk 中，实现物理代码解耦。
   - 配置 `src-tauri/tauri.conf.json`：
     - 确保 `bundle.resources` 包含 `"resources/builtin-ep": "builtin-ep"`。
3. GitHub Actions 发布流水线 (`.github/workflows/release.yml`) 适配：
   - 验证 matrix 中 `edition: pro` 的打包阶段能自动生成已签名的内置 Pro 扩展并打入各平台客户端安装包（DMG, EXE, AppImage, deb, rpm 等）。
   - 在构建产物中增加独立上传 `sql-editor-pro-${version}.dzx` 到 Release 资产。
4. 编写自动化单测与流水线测试：
   - `scripts/__tests__/pack-ep.test.ts`：测试 `pack-ep.mjs` 正确打包与签名 `.dzx`，测试解压与文件结构符合规范。

---

## Deliverables

| Item | Status |
|---|---|
| `scripts/pack-ep.mjs` packager utility | ✅ Done |
| `scripts/sign-ep.mjs` signing helper (invoked by pack-ep) | ✅ Done |
| `scripts/resolve-pro.mjs` decoupled codegen & staging | ✅ Done |
| `src-tauri/tauri.conf.json` builtin-ep resource mapping | ✅ Done |
| `.github/workflows/release.yml` pro packaging & .dzx upload | ✅ Done |
| Packaging unit tests (`scripts/__tests__/pack-ep.test.ts`) pass | ✅ Done |

---

## Coder Self-Verification

| Suite | Result |
|---|---|
| `npx vitest run scripts/__tests__/pack-ep.test.ts` | **7/7 passed** (incl. sql-editor-pro integration build) |
| `npx vitest run scripts/__tests__/resolve-pro.test.ts` | **7/7 passed** |
| `npx tsc --noEmit` | **0 errors** |

---

## Tester Verification

| Suite | Coder | Tester (独立复验) |
|---|---|---|
| `npx vitest run scripts/__tests__/pack-ep.test.ts scripts/__tests__/resolve-pro.test.ts` | 14/14 | **35/35 passed** |
| `npx tsc --noEmit` | 0 errors | **0 errors** |

### 脚本覆盖率（v8, `--coverage.include=scripts/{pack-ep,resolve-pro,sign-ep}.mjs`）

| File | Lines | Stmts | Branches |
|---|---|---|---|
| `pack-ep.mjs` | 92.5% | 91.8% | 87.4% |
| `resolve-pro.mjs` | 72.9% | 72.9% | 86.4% |
| `sign-ep.mjs` | 81.4% | 81.4% | 73.3% |
| **Aggregate** | **83.3%** | **83.0%** | **82.8%** |

> `resolve-pro.mjs` 未覆盖分支主要为 `ensureProCheckout` 的 git clone / `/tmp` fallback 与 CLI 入口（需网络或外部目录，留待 R 阶段集成验证）。

### 代码审查摘要

| 文件 | 结论 |
|---|---|
| `scripts/pack-ep.mjs` | ✅ 正确实现 build → stage → sign → `.dzx`/staging 双模式；`REQUIRED_PACKAGE_PATHS` 与 ZIP 结构符合规范 |
| `scripts/resolve-pro.mjs` | ✅ Pro codegen 已解耦静态 import，改为 `hostExtensionLoader.loadFromUrl` + `builtin-ep` 路径解析；community restore 清理 staging |
| `src-tauri/tauri.conf.json` | ✅ `bundle.resources` 含 `"resources/builtin-ep": "builtin-ep"` |
| `.github/workflows/release.yml` | ✅ Pro matrix 传 `--edition=pro`；linux-x64 构建后 `--skip-build` 打 `.dzx`；`artifacts/*` 上传含 dzx |

### Tester 新增测试（`[tester]` 块）

- `resolve-pro.test.ts`：community/restore staging 清理、`ensureProCheckout` 显式路径、`stageProExtension`、`resolvePro` pro 全链路 staging（当 pro-extensions 存在时）
- `pack-ep.test.ts`：`parsePackArgs`、manifest 校验、unknown mode、mode=both、locales 复制、`sign-ep` 辅助函数与错误路径

---

## E2E 用例登记

| ID | 场景 | 前置条件 | 执行方式 |
|---|---|---|---|
| E2E-EP-PKG-001 | Pro 版 `pnpm tauri:build:pro` 后安装包内含 `builtin-ep/sql-editor-pro/`（manifest + dist + signature.sig） | Pro 私仓 checkout + 签名密钥 | 【留待 R 回归】 |
| E2E-EP-PKG-002 | Community 版构建不含 `builtin-ep/sql-editor-pro/` | `--edition=community` | 【留待 R 回归】 |
| E2E-EP-PKG-003 | Pro 客户端启动后 SQL Editor Pro 能力（Hover/Completion）无需静态 bundle 即可激活 | Pro 安装包 + Postgres 连接 | 【留待 R 回归】 |
| E2E-EP-PKG-004 | GitHub Release Pro 流水线产出 `sql-editor-pro-{version}.dzx` 且 ZIP 内含 manifest/dist/signature/locales | `DATAZEN_EP_SIGNING_PRIVATE_KEY` secret | 【留待 R 回归】 |
| E2E-EP-PKG-005 | 独立 `.dzx` 热加载（开发者模式）替换内置扩展 | Wave 2 ep-security-gate 合流后 | 【留待 R 回归】 |

---

## Phase

`TEST_DONE` — 2026-09-08，Tester 独立复验通过，无 Bug 登记。
