# cr-p0-ext-install — 进度

**轨 ID：** cr-p0-ext-install  
**分支：** feature/cr-p0-ext-install  
**状态：** 编码完成（待 R 回归 E2E）  
**Commit：** `ac3cb10e`

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
| J1-001-R | 插件安装对话框改用 dialog injection + folder browse（替代 typed path） | 两步入驻 review → confirm → card 可见 | R 阶段 `plugins.spec.ts` 更新 |
| J1-001-R-alt | webdriver `override_path` 直连 `inspect_extension_package_with_dialog` / `install_extension` | IPC 安装 fixture 目录成功 | R 阶段可选 |

> 当前 `e2e/specs/plugins.spec.ts` J1-001 仍 typed path，**留待 R 回归**（dialog injection 或 override_path）。

## 测试结果

| 套件 | 结果 | 备注 |
|------|------|------|
| `cargo test -p datazen extensions::tests --lib` | 11 passed | |
| `vitest extensions.test.ts + InstallExtensionDialog.test.tsx` | 19 passed | |

## 遗留

- R 阶段：`plugins.spec.ts` J1-001 迁移至 dialog injection / override_path。
- 非 en 语系 `plugins.install.*` 新 key（`browseZip` / `browseFolder` / `pickPrompt`）待 i18n-sync。
