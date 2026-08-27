# Plugin 命名统一改造计划

> 目标：消除项目中 "plugin" 一词多义的混淆，统一术语为 **driver**（编译时数据库驱动）和 **extension**（运行时 UI/主题扩展）。

## 改造范围

### Track A：Cargo feature / plugin_init.rs → driver（高优先级）

**变更内容：**
1. `drivers-registry.json`：所有 `"feature": "plugin-<id>"` → `"driver-<id>"`
2. `src-tauri/Cargo.toml`：feature 名 `plugin-*` → `driver-*`，标记注释 `# <<plugin-*>>` → `# <<driver-*>>`
3. `Cargo.toml`（root）：`# <<plugin-patches>>` → `# <<driver-patches>>`
4. `src-tauri/src/plugin_init.rs` → `driver_init.rs`，内部 `register_plugins` → `register_drivers`，所有 `#[cfg(feature = "plugin-*")]` → `driver-*`
5. `src-tauri/src/lib.rs`：`mod plugin_init` → `mod driver_init`，调用点更新
6. `src-tauri/src/redis_flush_gate.rs`：`#[cfg(feature = "plugin-redis")]` → `driver-redis`
7. `src-tauri/src/transfer/adapter_registry.rs`：所有 `#[cfg(feature = "plugin-*")]` → `driver-*`
8. `scripts/resolve-drivers.mjs`：生成逻辑中的 `plugin-` 前缀 → `driver-`
9. `scripts/plugin-deinject.mjs`：marker 名 `plugin-*` → `driver-*`
10. `scripts/plugin-stash-precommit.mjs`：检测逻辑更新
11. `scripts/ensure-generated-drivers.mjs`：注释更新
12. `scripts/check-managed-stubs.mjs`：注释更新
13. `scripts/run-e2e-minimal.sh`：feature 引用更新
14. `scripts/new-feature-worktree.sh`：注释更新
15. `.gitignore`：`src-tauri/src/plugin_init.rs` → `driver_init.rs`
16. `.plugin-features.json` → `.driver-features.json`（生成文件）
17. `scripts/__tests__/`：fixture 和测试断言更新
18. 文档：`AGENTS.md`、`CONTRIBUTING.md`、`docs/development/independent-plugin-development.*.md`、`docs/development/e2e-testing.md`
19. CI：`.github/workflows/ci.yml`、`.github/workflows/release.yml`
20. Shell：`scripts/run-regression.sh`、`scripts/sync-repos.sh`、`e2e/record-demo.sh`

**不触碰：**
- `src-tauri/src/plugins/`（运行时插件模块，Track B 处理）
- `packages/drivers/` 下的 crate 名（`datazen-driver-*` 已经正确）
- Git 驱动 crate 名（`datazen-plugin-*` 是外部仓库命名，不在本项目改）

### Track B：Rust plugins/ → extensions/ + 前端重命名（中优先级）

**变更内容：**
1. `src-tauri/src/plugins/` → `src-tauri/src/extensions/`（整个目录重命名）
2. `src-tauri/src/plugins/mod.rs` 中所有类型/常量重命名：
   - `PluginManager` → `ExtensionManager`
   - `LoadedPlugin` → `LoadedExtension`
   - `PluginManifest` → `ExtensionManifest`
   - `PLUGIN_API_VERSION` → `EXTENSION_API_VERSION`
   - `MAX_PLUGIN_FILES` → `MAX_EXTENSION_FILES`
   - `MAX_PLUGIN_UNCOMPRESSED` → `MAX_EXTENSION_UNCOMPRESSED`
   - `PLUGINS_OPEN_PAGE_EVENT` → `EXTENSIONS_OPEN_PAGE_EVENT`
   - `ENABLED_MARKER_FILE` 保持不变（通用名）
   - 函数名：`is_valid_plugin_id` → `is_valid_extension_id`、`allowed_plugin_extension` → `allowed_extension_file_ext`、`validate_plugin_dir` → `validate_extension_dir` 等
3. `src-tauri/src/plugins/manifest.rs`：结构体和函数名更新
4. `src-tauri/src/plugins/install.rs`：函数名更新，`crate::theme::validate_theme_zip_path` 调用保持（Track C 再处理）
5. `src-tauri/src/plugins/storage.rs`：常量和函数名更新
6. `src-tauri/src/plugins/protocol.rs`：常量和函数名更新
7. `src-tauri/src/plugins/*_tests.rs`：测试引用更新
8. `src-tauri/src/commands/plugins.rs` → `commands/extensions.rs`：
   - `PluginPageSummary` → `ExtensionPageSummary`
   - `PluginThemeSummary` → `ExtensionThemeSummary`
   - `PluginSummary` → `ExtensionSummary`
   - 所有 `plugin_*_impl` → `extension_*_impl`
   - Tauri 命令名：`list_plugins` → `list_extensions`、`install_plugin_from_path` → `install_extension_from_path` 等
   - `PLUGINS_CHANGED_EVENT` → `EXTENSIONS_CHANGED_EVENT`
9. `src-tauri/src/commands/mod.rs`：`mod plugins` → `mod extensions`，`pub use plugins::*` → `pub use extensions::*`，`PluginManager` → `ExtensionManager`
10. `src-tauri/src/lib.rs`：`mod plugins` → `mod extensions`，所有 `plugins::` 路径 → `extensions::`，命令注册名更新
11. `src-tauri/src/commands/context.rs`：`plugins:` 字段 → `extensions:`，`PluginManager` → `ExtensionManager`

**前端 TypeScript 重命名：**
12. `src/commands/plugins.ts` → `src/commands/extensions.ts`：
    - `pluginCommands` → `extensionCommands`
    - `PLUGINS_CHANGED_EVENT` → `EXTENSIONS_CHANGED_EVENT`
    - 所有 IPC invoke 命令名更新（Tauri 自动映射 camelCase↔snake_case）
13. `src/stores/pluginStore.ts` → `src/stores/extensionStore.ts`：
    - `usePluginStore` → `useExtensionStore`
    - `PluginStore` → `ExtensionStore`
    - `ensurePluginsChangedListener` → `ensureExtensionsChangedListener`
14. `src/types/plugin.ts` → `src/types/extension.ts`：
    - `PluginPermission` → `ExtensionPermission`
    - `PluginManifest` → `ExtensionManifest`
    - `PluginPageSummary` → `ExtensionPageSummary`
    - `PluginThemeSummary` → `ExtensionThemeSummary`
    - `PluginSummary` → `ExtensionSummary`
    - `EXTENSION_API_VERSION` 保持不变（已经正确）
15. `src/lib/extensionBridge.ts`：更新 import 路径和类型引用
16. `src/lib/extensionI18n.ts`：更新 import
17. `src/lib/themePackApply.ts`：更新 import
18. `src/lib/themeTokens.ts`：更新 import
19. `src/windows/workspace/PluginPageShell.tsx` → `ExtensionPageShell.tsx`：
    - `PluginPageShell` → `ExtensionPageShell`
    - `PluginPageShellProps` → `ExtensionPageShellProps`
20. `src/windows/workspace/workspacePages.ts`：更新 import 和类型引用
21. `src/windows/workspace/WorkspaceView.tsx`：更新 import 和 JSX
22. `src/windows/plugins/` → `src/windows/extensions/`：
    - `PluginManagementPage.tsx` → `ExtensionManagementPage.tsx`
    - `InstallPluginDialog.tsx` → `InstallExtensionDialog.tsx`
    - `permissionLabels.ts` 保持不变
23. `src/windows/connection/ConnectionPage.tsx`：更新 import
24. `src/windows/settings/PluginSettingsSection.tsx`：更新 import
25. `src/windows/settings/AppearanceSection.tsx`：更新 import
26. `src/stores/settingsStore.ts`：`pluginSettings` 字段保持（语义正确，是插件的设置）
27. `src/types/index.ts`：`pluginSettings` 字段保持
28. `src/plugin-sdk/settings.ts`：`PluginSettingsContribution` 保持（这是宿主侧 SDK，给 plugin 用的）
29. 测试文件：`src/commands/__tests__/plugins.test.ts`、`src/stores/__tests__/pluginStore.test.ts`、`src/types/__tests__/plugin.test.ts`、`src/windows/plugins/__tests__/*`、`src/windows/workspace/__tests__/*`、`src/lib/__tests__/extensionBridge*`
30. 文档：`docs/architecture/backend/plugins.md` → `extensions.md`，其他引用更新

**不触碰：**
- `packages/extension-sdk/`（已经是正确的 "extension" 命名）
- `packages/extensions/`（已经是正确的目录名）
- `src/plugin-sdk/`（宿主侧 SDK，给 plugin/extension 用的，名字合理）
- `src/plugins/generated.ts`（gitignore 的生成文件，由 resolve-drivers 生成，内容关于 driver 不是 extension）

### Track C：Legacy ThemePack 清理（低优先级，可后续独立做）

**注意**：`plugins/install.rs` 调用 `crate::theme::validate_theme_zip_path`，ThemePack 模块不能直接删除，需要先迁移该函数。

**变更内容（如执行）：**
1. 将 `validate_theme_zip_path` 迁移到共享位置（如 `src-tauri/src/util/` 或内联到 `extensions/install.rs`）
2. 删除 `src-tauri/src/theme/validate.rs`、`src-tauri/src/theme/install.rs`
3. 保留 `src-tauri/src/theme/surface_bg.rs`（活跃使用）+ `src-tauri/src/theme/mod.rs`（仅导出 surface_bg）
4. 删除 `src-tauri/src/commands/theme.rs` 中的 ThemePack IPC（保留 surface_bg 相关）
5. 删除 `src/commands/theme.ts`、`src/types/themePack.ts`
6. 简化 `src/lib/themePackApply.ts`（删除 legacy packId 分支）
7. 删除 `src/windows/settings/ThemePackSection.tsx`
8. 更新 `src-tauri/src/lib.rs` 和 `commands/mod.rs` 中的注册
9. 清理测试和 i18n keys

## 分轨策略

### 冲突面分析

| 文件 | Track A | Track B | Track C | 冲突？ |
|------|---------|---------|---------|--------|
| `src-tauri/src/lib.rs` | L13, L724 | L14, L618-619, L728-729, L965-974 | L23, L945-948 | 不同行，无冲突 |
| `src-tauri/src/commands/mod.rs` | 无 | L20,47,62,89 | L26,53 | 不同行，无冲突 |
| `src-tauri/Cargo.toml` | 特性/依赖块 | 无 | 无 | 无冲突 |
| `scripts/resolve-drivers.mjs` | 全文 | 无 | 无 | 无冲突 |
| `drivers-registry.json` | 全文 | 无 | 无 | 无冲突 |

**结论：Track A 和 Track B 可以完全并行。** Track C 建议串行在 B 之后（因依赖 `extensions/install.rs` 的最终形态）。

### 并行编排

```
Wave 1（并行）:
  Track A（driver 重命名）──→ 测试 → 合并
  Track B（extension 重命名）──→ 测试 → 合并

Wave 2（串行，可选）:
  Track C（ThemePack 清理）──→ 测试 → 合并
```

## 验收标准

### Track A
- [ ] `grep -r "plugin-" src-tauri/Cargo.toml` 无匹配（除注释说明历史）
- [ ] `grep -r "plugin_init" src-tauri/src/ scripts/` 无匹配
- [ ] `grep -r 'cfg(feature = "plugin-' src-tauri/src/` 无匹配
- [ ] `cargo check -p datazen` 通过（需先 codegen）
- [ ] `scripts/__tests__/` 测试全部通过
- [ ] `pnpm tauri:dev --drivers=basic` 能正常启动

### Track B
- [ ] `src-tauri/src/plugins/` 目录不存在（已迁移为 `extensions/`）
- [ ] `grep -r "PluginManager" src-tauri/src/` 无匹配
- [ ] `grep -r "usePluginStore" src/` 无匹配（已改为 `useExtensionStore`）
- [ ] `grep -r "pluginCommands" src/` 无匹配（已改为 `extensionCommands`）
- [ ] `cargo check -p datazen` 通过
- [ ] `npx vitest run` 通过
- [ ] `tsc --noEmit` 通过

### Track C
- [ ] `src-tauri/src/theme/` 仅保留 `mod.rs` + `surface_bg.rs`
- [ ] `grep -r "ThemePackSummary" src-tauri/src/ src/` 无匹配
- [ ] `ThemePackSection.tsx` 已删除
