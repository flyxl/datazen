# 插件系统开发进度管理

> 流程：编码 agent 开发 + 单测 → commit → 新测试 agent 输出 E2E 用例与结果（覆盖率 ≥80%，只报不修）→ commit → bug 循环（验证不通过→修复中→待验证→已修复）。
> 分支：`feature/ui-plugins`（worktree：`../datazen-ui-plugins`）。PRD：[ui-plugins.md](./ui-plugins.md) v0.5；技术方案：[ui-plugins-implementation.md](./ui-plugins-implementation.md)。

## 功能工作项

| # | 功能 | 范围摘要 | 状态 | 开发 commit | 测试 commit |
|---|------|---------|------|------------|------------|
| F1 | Rust 插件基座 | plugins/{mod,manifest,install,storage}.rs、IPC 命令组、AppState、单测（capabilities 走既有 ACL 豁免，见测试记录） | 已完成 | 900b9330 | d9d265b3 |
| F2 | datazen:// 协议 | register_uri_scheme_protocol：path 资产服务 + open 深链 + CSP/403/404 | 待测试 | 4c75f1b0 | — |
| F3 | 前端状态与 IPC 封装 | types/plugin.ts、pluginStore、workspaceTabsStore、commands/plugins.ts | 未开始 | — | — |
| F4 | 主窗口集成 | WorkspaceMode 扩展、aside 两按钮、Workspace 导航栏/默认卡片/独立 Tab 条/页面壳（静态） | 未开始 | — | — |
| F5 | 插件管理页 | PluginManagementPage + InstallPluginDialog（卡片/过滤/安装/启停/卸载） | 未开始 | — | — |
| F6 | RPC 桥 | uiPluginBridge：信封路由、权限判定、限流超时、token 快照推送 | 未开始 | — | — |
| F7 | Settings 外观 | settings.appearance 菜单项 + AppearanceSection 主题切换器 | 未开始 | — | — |
| F8 | SDK 包 | packages/ui-plugin-sdk（bridge/theme/theme.css/useTheme） | 未开始 | — | — |
| F9 | 示例插件与 E2E | e2e/fixtures/sample-plugin + e2e/specs/plugins.spec.ts journeys 1-5 | 未开始 | — | — |

## Bug 跟踪

| ID | 功能 | 描述 | 重现步骤 | 状态 |
|----|------|------|---------|------|
| （暂无） | | | | |

Bug 状态流转：`新建 → 验证不通过(修复中) → 待验证 → 已修复`

## 测试记录

（每个功能测试完成后在此追加小节：用例清单、结果、覆盖率、bug 链接）

### F1（Rust 插件基座，commit 900b9330）

- 测试 agent 会话，2026-08-22。规格依据：ui-plugins-implementation.md §2.2/§2.3/§2.5/§2.6。
- 执行命令：
  - `cargo test -p datazen --lib plugins` → **60 passed / 0 failed**（54 既有单测 + 6 新增集成用例）
  - `cargo test -p datazen --lib commands::mcp -- --test-threads=1` → 6/6 PASS。注：mcp 组并行跑时因全局句柄竞态偶挂（`start_embedded_mcp_reports_running` 等 3 例），单线程即稳定通过；**既有 flaky，与 F1 无关**。

#### 新增测试文件

`src-tauri/src/plugins/integration_tests.rs` + `plugins/mod.rs` 内一行 `#[cfg(test)] mod integration_tests;` 接线（零发布代码影响）。落点说明：lib.rs 中 `mod plugins` 为 crate 私有，外部 `tests/` 目标无法访问 crate 内部；放宽可见性属功能代码改动被禁止，故集成套件以 lib 测试目标编译，运行入口仍为 `cargo test -p datazen --lib`。

#### 用例清单

既有单测（54 例，全部 PASS）：

| 编号组 | 模块 | 场景 | 数量 | 结论 |
|--------|------|------|------|------|
| U-01–U-12 | install.rs | zip/目录安装、顶层目录 zip、穿越条目拒绝、隐藏/.sh/.exe 条目拒绝、可配置大小限额、压缩比炸弹、重装备份清理、失败保留旧包+清 staging、缺源报错 | 12 | PASS |
| U-13–U-39 | manifest.rs | id 缺点号/大写/超长、apiVersion 失配、backend 非 null、semver 正反例、page id/showIn/modes、entry 缺失或文件不存在、目录名≠id、声明路径穿越/隐藏组件/非法扩展名、包扫描限额/symlink/恶意 svg/marker 跳过、未知权限字符串、顶层/page 多余字段、纯主题免 entry 免权限 | 27 | PASS |
| U-40–U-47 | storage.rs | set/get 往返、跨插件隔离、缺键 None、remove 存在性与空表清文件、超 1MB 拒绝、非法 id/key、8 线程并发原子写、无 tmp 残留 | 8 | PASS |
| U-48–U-54 | commands/plugins.rs | 全生命周期流程、目录安装+重装、无效包拒绝、storage 需已注册插件+命名空间、read_plugin_file 沙箱规则、load_from_disk 恢复 enabled 态、Summary camelCase 序列化 | 7 | PASS |

新增集成用例（6 例，全部 PASS）：

| 编号 | 场景 | 预期 | 实际 | 结论 |
|------|------|------|------|------|
| I-01 | 安装 zip→list→set_enabled(false)→list→重启模拟 reload→enable→remove→重复 remove 全流程 | summary/list/enabled 落盘正确；disabled 仍列出且读取被拒；reload 恢复 disabled；storage 数据跨启停保留；remove 删目录含 .storage.json；二次 remove 报 NotFound | 符合预期 | PASS |
| I-02 | 恶意 zip：`../` 穿越、嵌套 `assets/../../x.html`、绝对路径 `/etc/*.html`、Windows 盘符 `C:/Windows/*.html`、`.sh` 扩展名 | 安装全部拒绝、registry 保持为空、无 `.staging-*`/`*.old.bak` 残留 | 符合预期 | PASS |
| I-03 | 超 50MB 声明大小 zip（51 MiB deflated） | 基于中央目录尺寸在解压前拒绝（uncompressed size limit），无残留 | 符合预期 | PASS |
| I-04 | manifest 边界×9（经 parse→validate 与安装端到端双路径）：id 缺点号、id 双点号 `ac.me.bill`、apiVersion=1、apiVersion=3、`backend:{}`、未知权限串、顶层多余字段、page 对象多余字段、icon 文件缺失；规则 2 文案含 DataZen 版本指引 | 各项精确报错；安装路径无残留 | 符合预期 | PASS |
| I-05 | storage 跨插件隔离（双真实插件同 key）+ 未注册插件 IPC 拒绝 + 1MB 边界（MAX−512 可写、追加超限拒绝且原数据不损坏） | 命名空间互不可见；ghost 报 NotFound；超额写失败不影响已有键 | 符合预期 | PASS |
| I-06 | storage remove 幂等：底层 bool true→false、IPC 层两次 remove 均 Ok、空表删文件 | 幂等无错误、文件清理 | 符合预期 | PASS |
| I-07* | read_plugin_file 沙箱：正常/嵌套读取、`.enabled`/`.storage.json`/`assets/.secret.css` 拒绝、`../`、绝对路径、反斜杠穿越拒绝、缺失文件与未知插件 NotFound、未启用读取被拒 | 一律按约束拒绝且错误信息不含内部路径泄露 | 符合预期 | PASS |

\* I-02/I-03 合并为一个测试函数 `malicious_zips_rejected_without_side_effects`，I-07 对应 `read_plugin_file_enforces_sandbox_rules`；共 6 个 `#[tokio::test]` 函数。

#### 覆盖率结论（cargo llvm-cov 0.8.7，实测）

仅运行 `plugins` 过滤测试（`cargo llvm-cov --lib --summary-only -- plugins`）：

| 文件 | 行覆盖 | 区域覆盖 | 函数覆盖 |
|------|--------|---------|---------|
| src/plugins/mod.rs | 80.00% | 68.42% | 80.60% |
| src/plugins/manifest.rs | **92.44%** | 84.62% | 94.24% |
| src/plugins/install.rs | **87.11%** | 55.84% | 87.33% |
| src/plugins/storage.rs | **92.37%** | 84.00% | 94.61% |
| src/commands/plugins.rs | **86.33%** | 61.73% | 83.10% |

**行覆盖率 80%–92%，五个文件全部 ≥80% 达标**。区域（分支）覆盖未达 80% 的两处局限如实说明：install.rs 未覆盖分支集中在防御性溢出臂（checked_add overflow）、备份为文件的罕见分支、zip 炸弹 reader 的 entry 超长臂；commands/plugins.rs 未覆盖区域主要是 `#[tauri::command]` 宏包装层与 spawn_blocking JoinError 臂——均需 Tauri 运行时或人为构造 IO 故障才能触达。

#### Bug 列表

无功能缺陷（0 FAIL）。以下为规格偏差登记：

| ID | 类型 | 描述 | 重现步骤 | 期望 vs 实际 | 状态 |
|----|------|------|---------|-------------|------|
| DEV-F1-01 | 备注 | §2.3/§2.5 的 `install_plugin_with_dialog` 未实现，仅提供 `install_plugin_from_path` | `grep -rn install_plugin_with_dialog src-tauri/src/` 无结果 | 规格：dialog 选 zip/目录入口命令；实际：dialog 流程延后至 F5 InstallPluginDialog（前端发起 dialog + from_path）。功能等价，范围决策 | 备注（F5 承接） |
| DEV-F1-02 | 备注 | §2.6 要求 default.json.host 增补 `allow-list-plugins` 等 permission identifier，实现未增补 | 查看 `src-tauri/capabilities/default.json.host` 无 plugin 条目；仓库所有 host 命令同样未列入（Tauri v2 应用自身命令不受 ACL 约束） | 规格：增补标识；实际：走应用命令 ACL 豁免（进度表 F1 已预先声明该决策），IPC 实测可达 | 备注（既定决策） |
| DEV-F1-03 | 备注 | 规则 2 错误文案为「需要更新版本的 DataZen >= x.y.z」，规格字面为「需要 DataZen ≥ x.y.z」 | apiVersion≠2 时观察错误信息 | 语义一致（告知宿主版本过旧并给出版本号），措辞差异 | 备注 |
| DEV-F1-04 | 备注 | 集成测试落点为 `src/plugins/integration_tests.rs`（非技术方案 §7 规划的 `tests/plugins_*.rs`） | 见「新增测试文件」说明 | 因 `mod plugins` 私有且禁止放宽可见性而调整；如需回归规划路径，须由开发将模块改为 pub | 备注 |

#### 规格复核结论（逐条对照 §2.2/§2.3/§2.5/§2.6）

符合项：校验规则 1–7 全部落实（含目录名==id、semver、backend 拒绝、pages→entry 必填、路径穿越/隐藏组件双重防护、扩展名白名单、50MB/2000 文件限额、纯主题免权限）；安装 staging→校验→原子改名→备份/回滚链路完整且有测试背书；storage 原子写（同目录 tmp+rename）+全局互斥+1MB 限额；read_plugin_file 四重约束（enabled 门禁/条目名校验/隐藏组件拒绝/canonicalize 包含性）；IPC 9 命令齐全且统一 CommandError+CmdExt 日志约定；`plugins:changed` 事件在 install/remove/set_enabled 后 emit（代码审查确认）；AppState.plugins 字段、finish_app_state 启动扫描、lib.rs 注册 9 命令全部就位。偏差项见 DEV-F1-01…03。

## 回归测试

- [ ] 全量回归（cargo test -p datazen --lib + npx vitest run）
- [ ] 文档更新（架构文档 docs/architecture/backend/plugins.md、AGENTS.md 精简增补）
- [ ] 合并 main
