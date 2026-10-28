# 前端形态拆分实施工作日志（form-split worklog）

> 本文件随每个 form-split PR 同步提交，记录实际执行顺序、决策与验证结果，
> 避免工作进度丢失。条目按时间倒序（最新在上）。基线：`2a08e55`（F4a 合入后）。

## 2026-10-29（会话二）— F4c 簇合并收口：223 → **215**（计划终态命中）
> 本段落 rebased 在并行会话的 F5（59dea6941）与 R6 event 焚烧（d712c47ff）之后：
> 两个会话的 F4c 重核结论一致（零死命令）；本提交把交接说明中「合并簇为
> 持续整合项」落实为实际合并，机械计数达到计划 §8 终态 ~215。


### 簇核定（对照计划 §5.4 项6 与 §8 轨迹）

- **死命令重核**：对 223 命令面跑字面引用 + transport.call 双轨审计，
  **零死命令残留**——main 侧 `6c883f2c0`（6 冗余命令）、`ad1ff0102`
  （query history 合并 + 网关薄壳 ×2）、`ce8c37998`（get_er_data +
  get_system_ui_language）已提前消化计划中的死命令清单。计划 §8 的
  267→255→~232 为旧基线估算，机械计数为准。
- **schema plan 族 6→2**：`prepare_schema_unified_plan` 是合并目标且已在线
  （SchemaDiffWindow 双路径：对象走 unified、纯表走 table）。四个 per-kind
  命令（view / routine_trigger / sequence / type）经核定**零产品调用、零
  e2e 调用**——只有 `schemaDiff.ts` 里的死包装方法与其单测在引用。跨方言
  纯表场景 unified 内部已委托 table impl（`unified_plan.rs:144`），但同方言
  纯表走的是 `build_unified_schema_diff_plan_with_source_scope` 独立代码
  路径，与 table builder 非行为等价——**table 命令保留**，不做 6→1 的
  最后一步（风险登记：无法本地编译验证 Rust 行为等价性，遵循「机械计数
  为准 + 不做无法验证的行为切换」）。
- **dir-open 4→2**：`open_log_dir` / `open_workflows_dir` /
  `open_context_dir` 合并为 `open_known_dir(kind: log|workflows|context)`；
  路径保持服务端解析（安全属性不变），`open_path` 保留（webdriver 门控 +
  app-data 校验路径）。
- **file pickers 3→1**：`pick_connections_import_file` /
  `pick_app_data_import_file` / `pick_connection_import_path_with_dialog`
  合并为 `commands::dialog::pick_import_file(kind, mode?, source?)`——放
  dialog 网关（R-1.3 集中化），filter 表留在 feature 模块
  （`connections_open_filters` / `import_file_filters` 升 pub(crate)）。
  `pick_data_transfer_sql_file` 是 save+token 语义（路径不出宿主），
  不属 open-picker 族，保留。
- **dialogs 8→6 / job 读侧 13→5 / AI 一次性 8→2**：经基线对照，这些簇
  在 main 侧 IPC 收敛 PR 中已按不同形状吸收（file_jobs 命令族删除、
  query_history 合并等）；当前面 215 已达终态，不再为凑旧估算数字做
  无验证依据的深部 Rust 手术。

### 改动清单

- Rust：删 `object_plan_commands.rs`（整文件）；`schema_diff.rs` /
  `object_scope.rs`（死 helper `fetch_schema_view` / `list_schema_views` 一并
  删除）；`config.rs`（open_known_dir 合并体）；`dialog.rs`（pick_import_file
  合并体）；`connection_import/ipc.rs`、`app_archive.rs`（旧 picker 删除）；
  `app_menu.rs`（菜单 OpenLogDir 走 open_known_dir("log")）；`run.rs` 注册
  223→215。
- TS：`schemaDiff.ts` 删 4 死方法 + 4 死测试；`connection.ts` / `backup.ts`
  切 `pick_import_file`；`hostServices.ts` 切 `open_known_dir`；
  `pathIpcWiring.test.ts` / `importExportCommands.test.ts` 断言更新；
  e2e `path-ipc-hardening.ts` / `workflow-window.ts` 源码断言重定向到
  hostServices 区（原断言指向 F2 已迁移走的 settings.ts 旧位置——顺手
  修复了这两处陈旧断言）。
- 闸门：R6 台账移除 `src/commands/settings.ts`（F4a 已清零，过期项）。

### 验证

- typecheck（tsrs root/scripts/pack-ep）OK；commands+platform+backend-client
  **337/337** 绿；schemaDiff 单测 17/17；边界闸门 0 blocking / 41 advisory；
  ci-docs 一致性 OK；注册命令机械计数 **215**（计划 §8 终态目标命中）。
- Rust 侧本机无 cargo（工具链下载受限）：静态核对全部删除项引用面 +
  契约测试源码断言双向核对；宿主 crate 依赖远程 CI 验证（开 PR 触发，
  同 F4a 策略）。

---

## 2026-10-29（续） — R6 台账 event 类焚烧（d712c47ff）+ 会话终态

- `@tauri-apps/api/event` 类违规全部迁移到 F2 EventChannel 端口（裸载荷，无 Tauri 信封）：
  WorkspaceView（wapps:open-page 深链）、sqlFileExecution（restore-progress）、
  QueryPanel（menu:add-favorite）、ai.ts（三条流监听）、file.ts（onExportProgress）、
  BackupWindow（backup-progress）。
- surfaceBgCache 尝试后**回退**：`set_surface_background` 是宿主文件持久化命令而非涂色，
  需专门端口（行为保持优先于台账数字）。
- 台账 41 → 34 advisory；测试从 event mock 改为平台契约 mock（裸载荷断言）。
- 验证：typecheck 绿；闸门 0 blocking / 34 advisory；受影响 125 测试绿。

### 会话终态与续作交接（F6 前）

- **已完成**：F1–F5 全部合入并推送（至 3d66bf002）+ 本批 event 焚烧（d712c47ff）。
- **F6 前置未满足**：`src/` 内 `@tauri-apps` 未归零——剩余约 33 文件全部为
  core-invoke 深迁移类（commands/*.ts 的 legacy 命令族 + BackupWindow/fetchRelationDdl/
  wappBridge/onboarding 等）。每文件需端口/门面设计（kernel 面方法入 MethodMap、
  宿主命令入 HostSettingsPort/新端口、驱动命令走 driverCommands 桥），非机械替换。
- **F6 本体**（workspace 协议 + 目录迁移 + R5 闸门）依计划为单个大 PR，
  须在上述归零后执行；worktree checkout `pnpm install` 实测为 §7.1 强制风险项。
- 续作路径：①烧 core-invoke 台账（按域分批）→ ②F6 restructure
  （`pnpm-workspace.yaml` packages 字段 + apps/desktop + packages/frontend + alias 退役）。

---

---

## 2026-10-29 — F4b + F4c 重核 + F5 合入（74d2eef → 59dea69）

### F4b（74d2eefe8）
- 命名归零完成：`get_table_data_at_target`→`get_table_data`、`read_artifact_chunk`→`read_artifact`、
  `platform_list/create/update/disable_profile`→`list/create/update/disable_connection`；
  **`commandOverrides` 表删除**（toCommandName 纯约定，kernel 轨与命令名 1:1，裁决 #2 第 4 步达成）。
- Connection 命令 wire 对齐 kernel 面（flattened ProfileDraft + kernel SubmissionToken、
  expectedRevision）——预留命令真正可从 connectionClient 调用（激活）；消费方切换依计划 §7
  「不设硬同步门槛」归 D 阶段。
- 导出双流程示范（§3.3）：`TablesExporterPort` 共享签名 `exportTables(request)`；桌面实现走
  save-dialog token 流式写（进度上 EventChannel），`createBlobTablesExporter` 为 web 半边
  （分页读入 → CSV → Blob 下载，仅浏览器 API，R6 天然合规）。
- R8 退役：driver-sdk 事件桥 `bindHostEventListener/listenHostEvent`；装配绑到 EventChannel；
  redis PubSubPanel 改走桥（不再 import @tauri-apps/api/event）。
- capabilities 预备：`settingsSectionsForBackend(ReadonlySet<SettingsSection>)`；
  SettingsContent 读 `capabilities?.settingsSections`——**src/ 内 `backendId ===` 代码级归零**。

### F4c 重核（并入 F5 提交说明）
- 经验式死命令扫描（注册名 × 全树字符串引用）：**0 条真死命令**——main #142/#143 已执行删除计划；
  合并簇为持续整合项（计划 §8 台账随 PR 更新）。F4 退出标准全项达成。

### F5（59dea6941）
- `src/platform/capabilities.ts`：DESKTOP/WEB_CAPABILITIES 静态表（sections/updater/versionSource）；
  桌面装配绑定。UpdateSection 消费 `capabilities.updater`、版本读消费 `versionSource`。
- 每域契约表 `domainContractTables.test.ts`（32 例：方法名 × 请求封套 × 响应直通 + 缺口两分类）。
- 端口契约 `platformContracts.test.ts`（11 例：§7.4 未绑定显式错误 + 绑定后行为；HostSettings
  惰性默认例外）。
- web stub `src/platform/web/`：全 throw transport（CapabilityUnsupported，绝不静默空答）+
  浏览器原生 PlatformServices 降级 + 惰性宿主端口 + Blob 导出半边 + WEB_CAPABILITIES——
  **编译期同形持续验证**。
- playbook：`docs/development/platform-extension-playbook.md`（决策树 + F3 复刻配方 + 测试标准）。
- 验证：typecheck 绿；闸门 0 blocking / 41 advisory（R8 清零）；418 测试绿。

---

---

## 2026-10-29（会话一）— 环境基线与 F4b 并行会话发现

### 会话环境基线（续作前核对）

- 检出 `docs/frontend-form-split-plan` @ `2a08e5507`，工作区干净。
- 工具链：node v24.22.1；pnpm 12.16.1（全局安装）；**cargo 本机不可用**
  （rustup 官方 CDN 与清华镜像均超时，直连组件下载 ~16KB/s 不可行）。
- `pnpm install --frozen-lockfile` 完成；`scripts/typecheck.mjs --engine=tsrs` OK；
  `vitest run packages/backend-client` 194/194 绿；边界闸门 0 blocking / 42 advisory。
- IPC 注册命令实测：**223**（计划轨迹数字 267→255→~232 为旧基线估算，
  main 侧 IPC 合并已提前消化部分——F4c 需按当前面重新核定死命令清单）。

### F4b 并行会话发现（rebase 时合入）

远程在克隆后新增 `74d2eefe8`（F4b）：命名归零完成（commandOverrides 表
删除、platform_*_profile→*_connection 等 7 项改名）、TablesExporterPort
导出双流程示范（桌面流式 + web Blob）、R8 退役（driver-sdk host event
bridge）、`settingsSectionsForBackend(ReadonlySet<SettingsSection>)` capabilities
预备形态（最后一个 backendId === 'desktop' 探测消除）。Connection 消费方
切换按计划 §7.4 无硬同步门槛裁决随 D 阶段走。

### 待办（按计划 §5.4–§5.6）

- [x] F4b：Connection 域示范迁移 + `platform_*_profile` 改名 + override 表消减（并行会话）
- [x] F4c：死命令删除 + 合并簇（本会话，223→215）
- [x] F4 项5：SettingsContent capabilities 预备、PubSubPanel 违规、导出双流程（F4b 并行会话）
- [ ] F5：DESKTOP_CAPABILITIES / 每域契约测试 / web stub / playbook
- [ ] F6：workspace 协议 + 包完整化 + 目录迁移 + R5 闸门

