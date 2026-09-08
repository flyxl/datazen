# DataZen EP 特权扩展点热插拔与独立打包改造 — 协调实施计划

> **PRD / 方案依据**：`docs/todo/hot-plug-prd.md`（第 3、6、7、8 节）、架构方案讨论记录  
> **体系**：特权扩展点（Host Extension Points, EP）生命周期、热插拔与独立打包体系  
> **Playbook**：`docs/development/subagent-dev-playbook.md` + `docs/development/subagent/`（Coordinator / Coder / Tester / Rescuer）  
> **集成分支**：`feat/sql-editor-clean`  
> **私有增强仓**：`https://github.com/flyxl/datazen-extension-sql-editor-pro`（映射在 `packages/pro-extensions/sql-editor-pro`）  
> **协调总览**：`docs/development/coordination/hub.md`（只读生成物，`node scripts/aggregate-hub.mjs`）  
> **轨目录**：`docs/development/coordination/tracks/<track-id>/`（各轨仅维护本轨 `progress.md` + `bugs.md`）

---

## 0. 角色与基线约定

| 角色 | 约束 |
|---|---|
| **协调者 (Coordinator)** | 不直接写大段业务代码；统筹波次、编写简报、派发子代理、监控活性、裁决合流与清理；关键节点向用户汇报 |
| **编码代理 (Coder)** | 仅在指定工作区（`.worktrees/datazen-<track-id>`）工作；全新实例；严禁修改主检出与 `hub.md`；完成后提交并置 `READY_FOR_TEST` |
| **测试代理 (Tester)** | 全新实例（严禁复用 Coder）；独立复测；只测不修；代码审查 + 覆盖率驱动补齐（≥80%）+ E2E 登记；全部通过后置 `TEST_DONE` |
| **调度时序** | 同一 Wave 所有 Coder 在单条消息中并发派发；某个 Coder 完成后立即为该轨启动 Tester，无需等待同 Wave 其他轨 |
| **基准分支** | 所有轨道以集成分支 `feat/sql-editor-clean` 最新 HEAD 为基线拉出 |

---

## 1. 波次与轨道编排

### Wave 1：运行时核心与 Pro 独立包构建（2 轨并行）
*依据：文件冲突面完全正交。轨 1 专注宿主运行时扩展点与编辑器隔室；轨 2 专注 Pro 扩展包独立工程化配置与生命周期入口导出。*

| Track | 任务摘要 | 主要写路径 | 依赖 | 状态 |
|---|---|---|---|---|
| **ep-core-runtime** | **扩展点热插拔运行时**：在 `@datazen/extension-points` 完善 `ExtensionModule` / `ExtensionContext` / `Disposable` 契约，实现动态注册/反注册/订阅机制与熔断保护；在 `src/components/sql-editor/` 改造 CodeMirror 隔室热重配（Gutter/Hover/Intention/Completion/Linter/Paste 等），保证无重载热拔插且光标/历史无损。 | `packages/extension-points/src/`、`src/components/sql-editor/` | 无 | **待启动** |
| **ep-pro-bundle** | **Pro 扩展独立打包规范**：在 `packages/pro-extensions/sql-editor-pro` 配置 Vite Library 模式打包脚本，Externalize 宿主共享库（react, @codemirror/*, @datazen/ui 等），输出 ESM 模块，规范化 `manifest.json` 与 `activate(ctx)` / `deactivate()` 生命周期入口。 | `packages/pro-extensions/sql-editor-pro/` | 无 | **待启动** |

### Wave 2：安全验签门禁与打包 CI 流水线（2 轨并行）
*依据：在 Wave 1 的独立包与运行时就绪后，接入安全验签与自动化流水线。*

| Track | 任务摘要 | 主要写路径 | 依赖 | 状态 |
|---|---|---|---|---|
| **ep-security-gate** | **数字签名与验签门禁**：实现官方签名工具 `scripts/sign-ep.mjs`；在宿主实现扩展验签门禁与兼容性检查；实现私有插件加载方案（开发者模式未签名放行、企业自定义公钥池 `{appData}/trusted-keys/`、本地目录直挂调试）。 | `scripts/sign-ep.mjs`、`src/lib/extensionSecurity.ts`、`src/windows/settings/`、测试套件 | Wave 1 | **待启动** |
| **ep-packaging-ci** | **打包工具与 CI 发布流水线**：实现 `scripts/pack-ep.mjs`（生成 `.dzx` 与 staging 资源）；改造 `scripts/resolve-pro.mjs` 与 `scripts/with-driver-inject.mjs` 将 Pro 扩展作为预置静态资源 `builtin-ep` 注入；配置 `src-tauri/tauri.conf.json`；更新 `.github/workflows/release.yml` 确保 Pro 版客户端和 `.dzx` 资产自动产出。 | `scripts/pack-ep.mjs`、`scripts/resolve-pro.mjs`、`src-tauri/tauri.conf.json`、`.github/workflows/release.yml` | Wave 1 | **待启动** |

### Wave 3：全量合流与 R 阶段回归（1 轨串行）

| Track | 任务摘要 | 主要写路径 | 依赖 | 状态 |
|---|---|---|---|---|
| **ep-r-regression** | **R 阶段全量回归**：编写热插拔端到端用户旅程测试（连续无感激活、卸载、重新激活）；验证 Community 与 Pro 双版本完整构建；运行全套测试套件验证零回归。 | `src/windows/connection/__tests__/`、全量构建、CI 验证 | Wave 1, 2 | **待启动** |

---

## 2. 详细验收标准 (Definition of Done)

1. **热插拔零重载**：在不刷新页面、不重启应用的前提下，调用扩展激活/停用，编辑器各能力（Gutter/波浪线/悬浮提示/高级粘贴）平滑开启与撤销，光标、选区、文本及撤销历史 100% 保持无损。
2. **独立打包解耦**：Pro 扩展可独立构建出标准的 ESM Bundle（`index.esm.js`），不再直接强耦合进宿主 Vite 构建单体。
3. **安全门禁有效**：官方扩展经签名校验后静默就位；未签名或篡改扩展默认拦截；支持开发者模式在显式风险确认后加载私有扩展。
4. **CI 自动化打包**：`.github/workflows/release.yml` 在配置 `edition: pro` 时能自动完成扩展独立构建、签名并注入安装包，产出开箱即用的 Pro 客户端与独立 `.dzx` 补丁包。
5. **测试覆盖率**：所有新增与重构模块测试覆盖率 ≥ 80%，全量单元测试与构建检查 100% 通过。
