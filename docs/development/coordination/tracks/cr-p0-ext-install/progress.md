# cr-p0-ext-install — 进度

**轨 ID：** cr-p0-ext-install  
**分支：** feature/cr-p0-ext-install  
**状态：** R 阶段 J1-001-R 已迁移（dialog injection）  
**Commit：** `41c3cf34`（编码）

## 范围

扩展安装/检查路径仅来自原生对话框 + 一次性 pick token；移除 webview 任意 path IPC（`install_extension_from_path` / `inspect_extension_package`）。

## 设计决策

- **Inspect：** `inspect_extension_package_with_dialog(packageKind, overridePath?)` — 原生 zip/文件夹 picker → validate-only inspect → 返回 `{ pickToken, packageLabel, manifest }`（路径不暴露给 webview）。
- **Install：** `install_extension(pickToken, overridePath?)` — 生产仅接受 prior inspect 的 opaque token；E2E `override_path` 走 `resolve_override_path` webdriver 门闸。
- **Pick helper：** `pick_extension_package_with_dialog`（Rust `pub(crate)`）经 `commands/dialog.rs` 统一网关。
- **前端：** `InstallExtensionDialog` 移除 `PathInput` 与手动 path；browse ZIP / browse folder 按钮。

## E2E 用例

| ID | 场景 | 断言 | 执行时机 |
|----|------|------|----------|
| J1-001-R | 插件安装对话框改用 dialog injection + folder browse（替代 typed path） | 两步入驻 review → confirm → card 可见 | ✅ `e2e/specs/plugins.spec.ts` J1-001（R 回归 2026-08-29） |
| J1-001-R-alt | webdriver `override_path` 直连 `inspect_extension_package_with_dialog` / `install_extension` | IPC 安装 fixture 目录成功 | 可选（未实现） |

## 测试结果

| 套件 | 结果 | 备注 |
|------|------|------|
| `cargo test -p datazen extensions::tests --lib` | 11 passed | 测试代理 2026-08-29 复验 |
| `vitest extensions.test.ts + InstallExtensionDialog.test.tsx` | 19 passed | 2 files；测试代理 2026-08-29 复验 |
| `e2e/specs/plugins.spec.ts` J1-001 | 待 R 全量 E2E | dialog injection + browse folder（follow-up 2026-08-29） |

## 复验记录（测试代理）

**时间：** 2026-08-29  
**编码 commit：** `41c3cf34`

### 验收标准

| # | 标准 | 结果 | 证据 |
|---|------|------|------|
| 1 | webview 不可传任意 path（旧 IPC 移除） | ✅ | `lib.rs` 仅注册 `inspect_extension_package_with_dialog` / `install_extension` |
| 2 | 路径来自原生对话框 + opaque pick token | ✅ | `pick_extension_package_with_dialog` → `dialog::open_file` / `pick_folder` |
| 3 | 前端移除直接 path 入口 | ✅ | `InstallExtensionDialog` 仅 browse ZIP/folder + `pickToken` 安装 |
| 4 | Host 单测 + E2E 登记 | ✅ | 11+19 单测通过；J1-001-R 已迁移至 `plugins.spec.ts` |

### Bug

无（`bugs.md` 空表）。

## 遗留

- 非 en 语系 `plugins.install.*` 新 key（`browseZip` / `browseFolder` / `pickPrompt`）待 i18n-sync。
