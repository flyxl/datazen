# Track ep-packaging-ci: Standalone Packager, Tauri Builtin Bundling & GitHub Actions Release

> Status: **READY_FOR_TEST**
> Branch: `feature/ep-packaging-ci`
> Base branch: `feat/sql-editor-clean`
> LastHeartbeat: 2026-09-08T18:36:00+08:00
> Coder Commit: `pending`
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
*(待 Coder 完成后由 Tester 独立复测登记)*
