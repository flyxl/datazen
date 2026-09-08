# Track ep-security-gate: Signature Verification Gate & Private Plugin Trust

> Status: **TEST_DONE**
> Branch: `feature/ep-security-gate`
> Base branch: `feat/sql-editor-clean`
> LastHeartbeat: 2026-09-08T18:38:00+08:00
> Coder Commit: `0ac438a3e`
> Tester Commit: `pending`

---

## Task Summary
1. 实现官方签名工具与规范：
   - 编写 `scripts/sign-ep.mjs`：基于 Node.js 原生 `crypto`（Ed25519 或 ECDSA）对扩展产物（`manifest.json` 与 `dist/index.esm.js`）计算 SHA-256 并生成数字签名文件 `signature.sig`。
   - 支持从环境密钥或本地密钥签名。
2. 宿主验签门禁与兼容性检查：
   - 在 `packages/extension-points/src/security.ts`（或 `src/lib/extensionSecurity.ts`）实现完整的验签与兼容性检测核心：
     - 官方内置公钥校验（验证 `signature.sig` 是否由官方私钥签发，防篡改）。
     - 校验 `manifest.json` 中的 `engines.extensionPointsVersion` 与宿主当前契约版本是否匹配。
3. 用户私有插件分级信任支持：
   - **开发者模式（Developer Mode）**：设置项 `allowUnsignedExtensions` 或环境变量 `DATAZEN_ALLOW_UNVERIFIED_EP=1`。开启后允许加载未签名的本地 `.dzx` 或模块，并携带 `[Unverified / 未经验证]` 标识。
   - **企业信任公钥池（Enterprise Trusted Keys）**：支持从 `{appData}/trusted-keys/` 目录加载额外的 `.pub` 公钥，通过企业公钥验签的私有扩展视作受信并正常加载。
   - **本地目录直挂调试（Local Directory Linking）**：识别本地源码/开发目录直接挂载，绕过生产签名门禁。
4. 编写全量单元测试：
   - `packages/extension-points/src/__tests__/security.test.ts`（或 `src/lib/__tests__/extensionSecurity.test.ts`）：
     - 测试合法官方签名通过。
     - 测试被篡改产物（hash 不匹配或签名错误）被明确拒绝。
     - 测试开发者模式开启时放行未签名扩展。
     - 测试自定义企业公钥签名通过。
     - 测试引擎版本不兼容时安全回退。
   - 覆盖率 ≥ 80%。

---

## Deliverables

| Item | Status |
|---|---|
| `scripts/sign-ep.mjs` signing utility | ✅ Done |
| Security verification gate & engine check in extension points | ✅ Done |
| Private plugin trust policies (Developer mode, Trusted keys pool, Local dev linking) | ✅ Done |
| Unit tests verifying valid sign, tampered reject, dev mode, enterprise keys | ✅ Done |
| `npx tsc --noEmit` 0 errors | ✅ Done |

---

## Coder Self-Verification

| Suite | Result |
|---|---|
| `npx vitest run packages/extension-points` | ✅ 42/42 passed (4 files) |
| `npx tsc --noEmit` | ✅ 0 errors |
| `node scripts/sign-ep.mjs --dir <fixture>` | ✅ signature.sig produced |

---

## Tester Verification

| Suite | Coder 自报 | Tester 独立实测 |
|---|---|---|
| `npx vitest run packages/extension-points` | 42/42 | **42/42** ✅ |
| `npx vitest run scripts/__tests__/sign-ep.test.mjs` | — | **2/2** ✅ |
| **合计** | 42/42 | **46/46** ✅ |
| `npx tsc --noEmit` | 0 errors | **0 errors** ✅ |

### 核心模块覆盖率（`security.ts` + `signaturePayload.ts`）

| 指标 | Coder 自报 | Tester 实测 |
|---|---|---|
| Lines | ≥80% | **100%** (104/104) |
| Statements | ≥80% | **100%** (109/109) |
| Branches | ≥80% | **93.65%** (59/63) |
| Functions | ≥80% | **100%** (19/19) |

### 代码审查摘要

- **`scripts/sign-ep.mjs`**：Ed25519 签名；SHA-256 覆盖 `manifest.json` 与 `dist/index.esm.js`；canonical payload（sorted keys + version）与 `signaturePayload.ts` 一致；支持 `DATAZEN_EP_PRIVATE_KEY` 环境变量与内置测试密钥。
- **`packages/extension-points/src/security.ts`**：验签门禁完整——官方/企业公钥链、digest 篡改检测、引擎版本精确匹配、开发者模式/环境变量未签名放行、local-link 绕过生产门禁。
- **跨运行时一致性**：`[tester] sign-ep.mjs cross-runtime parity` 验证 Node `crypto.sign` 产物可被 Web Crypto `verifyExtensionPackage` 接受。
- **审查备注（非 Bug）**：宿主 Settings UI 尚未接线 `allowUnsignedExtensions`；当前为库级 API + 配置注入，E2E 留待 Wave 3 / R 回归。

### Tester 新增测试

| 测试 | 覆盖路径 |
|---|---|
| `[tester] verifyExtensionPackage maps parseSignatureFile errors…` | `verifyExtensionPackage` catch → `invalid-signature` |
| `[tester] parseSignatureFile rejects invalid digest hex…` | `signaturePayload.ts` digest 正则校验 |
| `[tester] sign-ep.mjs cross-runtime parity` | Node 签名 ↔ 宿主验签端到端 |
| `[tester] rejects signing when required bundle file is missing` | `sign-ep.mjs` 缺失 bundle 错误路径 |

---

## E2E 用例登记

| ID | 场景 | 前置条件 | 状态 |
|---|---|---|---|
| EP-SEC-E2E-001 | Pro 版启动后内置已签名 EP 静默加载，SQL Editor Pro 能力可用 | Pro build + builtin signed `.dzx` | 【留待 R 回归】 |
| EP-SEC-E2E-002 | 未签名 `.dzx` 默认拦截并提示启用开发者模式 | Community/Pro，开发者模式关闭 | 【留待 R 回归】 |
| EP-SEC-E2E-003 | Settings 开启「允许未验证扩展」后加载未签名包，UI 显示 `[Unverified / 未经验证]` | 宿主 Settings 接线完成 | 【留待 R 回归】 |
| EP-SEC-E2E-004 | `{appData}/trusted-keys/` 放置企业 `.pub` 后，企业签名私有扩展正常加载 | 企业密钥 + 已签名 `.dzx` | 【留待 R 回归】 |
| EP-SEC-E2E-005 | 本地目录直挂（local-link）开发扩展跳签名门禁 | `sourceKind: local-link` + dev 挂载 | 【留待 R 回归】 |
| EP-SEC-E2E-006 | 篡改 `manifest.json` 或 bundle 后验签失败并拒绝激活 | 已签名包 + 手动篡改 | 【留待 R 回归】 |

---

## Bugs

无（见 `bugs.md`）
