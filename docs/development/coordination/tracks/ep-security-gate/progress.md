# Track ep-security-gate: Signature Verification Gate & Private Plugin Trust

> Status: **READY_FOR_TEST**
> Branch: `feature/ep-security-gate`
> Base branch: `feat/sql-editor-clean`
> LastHeartbeat: 2026-09-08T18:35:00+08:00
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
*(待 Coder 完成后由 Tester 独立复测登记)*
