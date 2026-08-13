# F8 QA 报告：最小 Diff Workspace（inspect + mapping 列表）

| 项 | 值 |
|---|---|
| 切片 | F8 |
| 工作目录 | `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat` |
| 被测代码 | `src-tauri/src/commands/sync/inspect.rs`、`src/windows/data-sync/mappingView.ts`、`src/windows/data-sync/DataSyncWindow.tsx`、10 语言 `sync.mapping*` |
| 规格 | 最小 Diff Workspace：Compare → `inspect_data_sync` 映射列表；Apply 禁用；覆盖拷贝横幅仍在。PRD V1.2 §6 / 方案 Phase 5 最小切片 |
| 测试角色 | 独立 QA；未修改任何产品代码；未 commit |
| 日期 | 2026-08-13 |
| **总评** | **PASS**（2 个产品缺陷：1 S3 + 1 S4；E2E 全部 BLOCKED） |

---

## 1. 测试环境

| 工具 | 版本 |
|---|---|
| OS | macOS darwin 24.6.0 (arm64) |
| rustc | 1.90.0 (1159e78c4 2025-09-14) |
| cargo | 1.90.0 (840b83a10 2025-07-30) |
| toolchain | stable-aarch64-apple-darwin |
| cargo-llvm-cov | 0.8.7 |
| Node | v22.20.0 |
| vitest | 4.1.10 |
| crate | `datazen` 0.0.9（`src-tauri`） |
| 桌面应用 | **未运行**（`ps` 无 DataZen 进程；computer-use `list_running_apps` / `list_windows` 仅 Edge / Cursor / 访达 等）。E2E **BLOCKED** |

工作目录（所有命令均在此执行）：

```text
/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat
```

覆盖率原始摘要：`docs/progress/f8-coverage.txt`。

---

## 2. 本切片测试设计（对照规格）

规格要点（本切片 = 最小可用，不是完整 Diff Workspace）：

- IPC `inspect_data_sync`：同族门闸 + 禁止同库自同步 + `get_tables` / `get_table_schema` → `classify_tables`（**不做行比较**）
- Compare 后窗口展示 mapping 列表（`data-testid="data-sync-mapping-row"`），状态文案走 `sync.mapping*`
- Apply / Start Sync **保持禁用**（`data-testid="data-sync-start-disabled"` + `sync.applyUnavailable`）
- 覆盖拷贝横幅仍在（`data-testid="data-sync-overwrite-retired"`）
- `mappingView.ts` 纯函数：label key / 显示名 / 摘要计数
- 10 语言 host 字典含全部 `sync.mapping*` 且 `mappingSummary` 占位符与 `summarizeMappings` 对齐

| ID | 层 | 意图 | 已有自动化 |
|---|---|---|---|
| UT-INS-01 | Rust `inspect_data_sync_impl` | 两连接同表 `users` → 含 MATCHED | `inspect_data_sync_returns_matched_tables` |
| UT-INS-02 | Rust | 同 connection + 同 database + 同 schema → Validation | **缺失**（L27–30 count=0） |
| UT-INS-03 | Rust | 异构 pair / 非 V1 family → incompatible | **缺失**（L25 `?` Err 未击中） |
| UT-INS-04 | Rust | 连接不存在 / get_tables 失败 | **缺失** |
| UT-INS-05 | Rust | View 被 filter 掉；schema 加载失败 → INCOMPATIBLE（classify） | **缺失**（L59/66/70/77） |
| UT-MV-01 | TS `mappingLabelKey` | 五种 status → 对应 i18n key | `labels every mapping status` |
| UT-MV-02 | TS `displayTableName` | 改名 `→`；同名；仅 target | `displays renamed mappings` |
| UT-MV-03 | TS `summarizeMappings` | matched / incompatible / unmapped 分桶 | `summarizes mapping buckets` |
| UT-MV-04 | TS | `mappingLabelKey` default / DISABLED 不计入 unmapped | **缺失**（L29 count=0） |
| UT-I18N-01 | TS locales | 10 语 host key 与 en 对齐（含 6 个 `sync.mapping*`） | `keeps host key parity with en` |
| E2E-F8-* | E2E | 开窗 → 横幅 → Compare → mapping row → Apply 禁用 | **BLOCKED**（无桌面应用；`e2e/specs/` 无 mapping 用例） |

---

## 3. 单元测试

### 3.1 命令与结果

```bash
cargo test -p datazen --lib commands::sync::tests::inspect_data_sync -- --nocapture
npx vitest run src/windows/data-sync/__tests__/mappingView.test.ts src/locales/locales.test.ts
```

| 命令 | 通过 | 失败 | 忽略 | 过滤 | 结果 |
|---|---:|---:|---:|---:|---|
| `commands::sync::tests::inspect_data_sync` | **1** | 0 | 0 | 887 | PASS（0.16s） |
| vitest（2 files） | **17** | 0 | 0 | — | PASS（3 mappingView + 14 locales） |

失败详情：无。

编译告警（不计入本切片缺陷；**未改代码**）：

- `src-tauri/src/dashboard/execute.rs` unused import `RefreshMode`（与 F5–F7 相同，无关）
- `src-tauri/src/dashboard/create.rs` unused variable `registry`（无关）
- `data_sync/execute.rs:205` `cancel_mid_run_rolls_back` 中被遮蔽的 `let mut exec`（F6 夹具，无关）

### 3.2 已落地单测清单

| ID | 测试名 | 覆盖规格 | 结果 |
|---|---|---|---|
| UT-INS-01 | `inspect_data_sync_returns_matched_tables` | `TestAppState::with_tables`；`src-ins` / `tgt-ins` 均有 `users`；结果含 `status == Matched && source_table == "users"` | PASS |
| UT-MV-01 | `labels every mapping status` | MATCHED / UNMAPPED_SOURCE / UNMAPPED_TARGET / DISABLED / INCOMPATIBLE → 六个 key 中的五个 label key | PASS |
| UT-MV-02 | `displays renamed mappings` | `customers → clients`；同名 `users`；空 source → `only_tgt` | PASS |
| UT-MV-03 | `summarizes mapping buckets` | 1 matched + 1 incompatible + 2 unmapped | PASS |
| UT-I18N-01 | locales 14 cases | 10 locale 注册；host key 与 en 奇偶一致；非英文本地化比例 | PASS |

### 3.3 IPC / 窗口静态对照（不改代码）

`inspect_data_sync` 已挂 `lib.rs` invoke；前端 `syncCommands.inspectDataSync` 调用同名 command。`TableMappingStatus` 为 `SCREAMING_SNAKE_CASE`，与 TS `'MATCHED'` 等对齐。`TableResult` 为 `camelCase`，与 `DataSyncTableResult` 对齐。

`handleCompare`：连上 Source/Target 后只调 `inspectDataSync`，`setComparisons([])`，MATCHED 行用 `displayTableName` 自动勾选，非 MATCHED checkbox `disabled`。页脚 Apply **硬编码 `disabled`** + `title={t('sync.applyUnavailable')}` + `data-testid="data-sync-start-disabled"`。顶栏横幅 `data-testid="data-sync-overwrite-retired"`。

10 语言（en / zh-CN / zh-TW / de / es / fr / ja / ko / pt-BR / ru）均有：

- `sync.mappingMatched` / `UnmappedSource` / `UnmappedTarget` / `Disabled` / `Incompatible` / `mappingSummary`
- `sync.overwriteRetiredBanner` / `sync.applyUnavailable`

`mappingSummary` 均含 `{matched}` `{incompatible}` `{unmapped}`，与 `summarizeMappings` 返回字段一致；`getTranslation` 会替换这些占位符。手工扫 10×8 key：**全部存在且非空**。

`locales.test.ts` 的 host key parity 会挡住任一语种漏 key；**没有**单独断言 `sync.mapping*` 文案或 `mappingSummary` 插值（缺口，不记缺陷）。

---

## 4. 覆盖率

### 4.1 `inspect.rs`（llvm-cov）

```bash
cargo llvm-cov -p datazen --lib --json --output-path /tmp/datazen-f8-cov/f8-coverage.json -- commands::sync::tests::inspect_data_sync
```

按路径 `/commands/sync/inspect.rs` 过滤。llvm-cov 本次 **1 passed / 0 failed**。

| 文件 | 行 covered | 行 total | 行覆盖率 | 函数 | 区域 |
|---|---:|---:|---:|---:|---|
| `inspect.rs` | 73 | 79 | **92.41%** | 100%（4/4） | 86.49%（96/111） |

**门槛：`inspect.rs` 行覆盖率 ≥ 80% → 满足（92.41%）。**

未覆盖行见 `f8-coverage.txt`：自同步拒绝、schema `Err`、第二次 family `?` Err。

### 4.2 `mappingView.ts`（vitest --coverage）

| 指标 | 覆盖 | 总数 | % |
|---|---:|---:|---:|
| **Lines（门槛）** | 13 | 14 | **92.85%** |
| Statements | 13 | 14 | 92.85% |
| Functions | 6 | 6 | 100% |
| Branches | 14 | 15 | 93.33% |

**门槛：行覆盖率 ≥ 80% → 满足（92.85%）。**

未覆盖：L29 `default` 臂。

测试缺口（建议后续补，不改本切片覆盖率结论）：

1. inspect：自同步 / 异构 / 缺连接 / View / schema 失败。
2. `mappingLabelKey` 非法 status；`summarizeMappings` 含 DISABLED / 空数组。
3. `DataSyncWindow` 无组件单测（Select All / footer total 见 §7）。

---

## 5. E2E 用例表

现状：**BLOCKED**。未新增 `e2e/specs/` 文件（禁止改产品代码）。本机无 DataZen 桌面进程；computer-use 窗口列表无应用窗。现有 `e2e/specs/data-sync-real.ts` 面向旧 compare / `sync_tables` refuse，**没有** `inspect_data_sync` / `data-sync-mapping-row` 断言，不能当 F8 验收。

| ID | 步骤 | 期望 | 实际 |
|---|---|---|---|
| E2E-F8-01 打开窗口见横幅 | 主窗口 Tools / 菜单 Data Sync → 单例 `data-sync` 窗 | 标题为 `sync.windowTitle`；顶栏 `data-testid="data-sync-overwrite-retired"` 可见，文案为当前语言的 `sync.overwriteRetiredBanner` | **BLOCKED**（无应用） |
| E2E-F8-02 Compare 后见 mapping row | 选同族两连接（如两 PG）；点 Compare | 进入 compared；至少一行 `data-testid="data-sync-mapping-row"`；MATCHED 可勾选；非 MATCHED 勾选禁用；摘要为 `sync.mappingSummary` | **BLOCKED** |
| E2E-F8-03 Apply 禁用 | 同上，Compare 成功后看页脚主按钮 | 按钮 `data-testid="data-sync-start-disabled"` **disabled**；hover/title 为 `sync.applyUnavailable`；点击无 `sync_tables` / 无 DROP | **BLOCKED** |
| E2E-F8-04 未选两端点 Compare | 不选 Source/Target 点 Compare | 提示 `sync.selectBoth`；无 mapping 列表 | **BLOCKED** |
| E2E-F8-05 同源同目标 | 选同一连接为两端 | Target 选项 disabled 或 Compare 报 `sync.cannotSame`；后端若绕过则 Validation self-sync | **BLOCKED** |
| E2E-F8-06 异构 Target | Source PG、Target MySQL | Target 标 `sync.unsupportedHint` 且不可选；不得出现 mapping | **BLOCKED** |
| E2E-F8-07 仅源有表 | Source 多一表 | 该行 UNMAPPED_SOURCE 文案；checkbox 禁用 | **BLOCKED** |
| E2E-F8-08 结构不一致 | 同名表缺 PK / 列不同 | INCOMPATIBLE + `incompatibleReason`；不可勾选 | **BLOCKED** |
| E2E-F8-09 Select All | Compare 后点全选 | **规格期望**：仅勾选 MATCHED。**代码审查**：`selectAll` 仍读空的 `comparisons`，会清空已选（见 F8-BUG-01） | **BLOCKED**（未跑；静态已确认） |
| E2E-F8-10 i18n mapping | 切 zh-CN / en 再开窗 Compare | 状态列与摘要用对应语言 `sync.mapping*`，无 raw key | **BLOCKED** |

F9 落地建议：E2E-F8-01/02/03 进 Host 契约 journey（PG 夹具即可）；断言 testid + i18n，不写方言 SQL。IPC 冒烟：`connect` 两配置 → `inspect_data_sync` → 至少一条 MATCHED。

---

## 6. 规格缺口审查（F8 范围内）

审查范围：`inspect_data_sync_impl`、`mappingView.ts`、`DataSyncWindow` 的 Compare → 映射列表 → Apply 禁用、10 语言 mapping keys。行 Diff / ChangeSet / SQL Preview / `execute_data_sync` 接线属后续切片。

| 规格 | 实现 | 结论 |
|---|---|---|
| Compare 走 inspect，不做覆盖拷贝 | `handleCompare` → `inspectDataSync`；`setComparisons([])` | 一致 |
| 映射状态 MATCHED / UNMAPPED_* / INCOMPATIBLE / DISABLED | Rust enum + TS union + i18n keys | 一致；inspect 传入 `mappings=&[]`，本 IPC **不会**产出 DISABLED（仅 auto 同名匹配） |
| 仅 MATCHED 可勾选 | `disabled = row.status !== 'MATCHED'`；自动勾选 MATCHED | 一致 |
| Apply 未接通则禁用 | 按钮恒 `disabled` + `applyUnavailable` | 一致（故意） |
| 覆盖拷贝横幅 | `overwriteRetiredBanner` + testid | 一致 |
| 同族门闸 | 前端 `resolveSyncPairing` + 后端 `require_data_sync_family`（调用两次：连库前 + classify 前） | 行为一致；第二次 `?` 在第一次已成功时不可达 Err |
| 同库自同步禁止 | UI 禁同一 connection；IPC 再比 config id + database + schema | 一致；**无单测**击中 IPC 分支 |
| 10 语言 mapping keys | 10 文件均有 6 key；locales parity 覆盖存在性 | 一致 |
| Select All / 已选计数随 mapping 更新 | `selectAll` / footer `total` 仍绑 `comparisons` | **不一致** → §7 |

**未记为缺陷**的残留：

1. `startSync` 仍 `invoke('sync_tables')`，但 Apply 禁用，用户点不到。F3 IPC 也会 refuse。属死代码，等 Execute 接线删除。
2. 表头仍是「源行数 / 目标行数 / 旧 status」列，mapping 行只画名称+新 status；布局错位，F8 最小列表可接受。
3. `get_table_schema` 失败被静默跳过，表会在 classify 里变成「schema not loaded」INCOMPATIBLE，不 panic。
4. `t(mappingLabelKey(...) as never)` 类型绕过；运行时 key 合法。
5. `require_data_sync_family` 调用两次：第一次丢弃 `Ok(family)`，第二次再取。冗余，不改变结果。
6. 无完整 Diff（行/单元格/Preview/Execute）：本切片明确「最小」。
7. `SavedTasksBanner` 未挂到窗口（文件仍在）；属旧任务 UI，非 F8 mapping 范围。

---

## 7. Bug 列表

### F8-BUG-01（S3）Select All 在 mapping 列表下会清空已选

| 字段 | 内容 |
|---|---|
| 严重等级 | S3 |
| 模块 | Data Sync 窗口 / 映射列表 |
| 证据 | `DataSyncWindow.tsx` `selectAll`：`setSelectedTables(new Set(comparisons.filter(...)))`；`handleCompare` 成功后 `setComparisons([])`，MATCHED 已写入 `selectedTables` |

Compare 成功后点「全选」会用空的 `comparisons` 覆盖 selection，**已自动勾选的 MATCHED 会被全部取消**。用户仍可逐行勾选，故非 S2。E2E-F8-09 未跑，静态 100% 可复现。

### F8-BUG-02（S4）页脚「已选择 N / M」的 M 恒为 0

| 字段 | 内容 |
|---|---|
| 严重等级 | S4 |
| 模块 | Data Sync 窗口 / 映射列表 |
| 证据 | 页脚 `t('sync.selected', { selected: selectedTables.size, total: comparisons.length })`；inspect 路径 `comparisons.length === 0` |

Compare 后会显示 `Selected 1 / 0 tables`（或当前语言等价文案），总数应来自 `mappingResults`（或至少 MATCHED 数）。不影响 Compare / 禁用 Apply。

无 S1/S2。inspect 快乐路径与 mappingView 纯函数与 i18n key 未发现与 P0 冲突的缺陷。

---

## 8. 总评

| 门槛 | 结果 |
|---|---|
| `cargo test -p datazen --lib commands::sync::tests::inspect_data_sync` | **1 passed / 0 failed** |
| `npx vitest run` mappingView + locales | **17 passed / 0 failed** |
| `inspect.rs` 行覆盖 ≥80% | **92.41%**（73/79） |
| `mappingView.ts` 行覆盖 ≥80% | **92.85%**（13/14） |
| 10 语言 `sync.mapping*` | 6 key × 10 locale 均在；parity 单测 PASS |
| 与 P0 规格冲突的缺陷（inspect / 见 mapping / Apply 禁用 / 横幅） | 无 |
| E2E | 全部 **BLOCKED**（无桌面应用；无 F8 spec） |
| 记入缺陷 | **2**（S3 Select All 清空；S4 页脚 total=0） |

**总评：PASS**

最小 Diff Workspace 满足：Compare 走 `inspect_data_sync`；映射列表按 status 渲染；Apply 禁用；覆盖拷贝横幅保留；`mappingView` 与 inspect 行覆盖均 ≥80%。Select All / 已选总数仍绑旧 `comparisons`，记 2 个缺陷，不挡住本切片 P0（开窗横幅 + Compare 见 mapping + Apply 禁用）。E2E 待有应用后跑 E2E-F8-01/02/03，并修 F8-BUG-01 后再跑 E2E-F8-09。
