# F9 QA 报告：i18n + Host E2E + Diff Workspace 壳

| 项 | 值 |
|---|---|
| 切片 | F9 |
| 工作目录 | `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat` |
| 分支 | `feat/data-sync-navicat` |
| 被测代码 | `src/windows/data-sync/DataSyncWindow.tsx`、`src/windows/data-sync/mappingView.ts`、`src/windows/data-sync/__tests__/DataSyncWindow.test.tsx`、`src/windows/data-sync/__tests__/mappingView.test.ts`、`src/locales/locales.test.ts`（Data Sync keys / `mappingSummary` 插值）、`e2e/specs/data-sync-window.ts`（DSW-001~005 + DSW-MAP-001）、`e2e/specs/data-sync-real.ts` 的 `SYNC-INSPECT-001` |
| 规格 | PRD `docs/data-synchronization-prd.zh-CN.md` V1.2；Compare → `inspect_data_sync` 映射列表；Apply 禁用；覆盖拷贝横幅；10 语言 `sync.mapping*` / `sync.applyUnavailable` |
| 测试角色 | **全新独立验收会话**；未修改任何产品代码；未 commit；只写本报告 + `f9-coverage.txt` |
| 日期 | 2026-08-13 |
| **总评** | **PASS**（单测 33/33；覆盖率 Lines ≥80%；E2E 全部 **BLOCKED**；产品缺陷 0） |

---

## 1. 测试环境

| 工具 | 版本 |
|---|---|
| OS | macOS darwin 24.6.0 (arm64) |
| rustc | 1.90.0 (1159e78c4 2025-09-14) |
| cargo | 1.90.0 (840b83a10 2025-07-30) |
| Node | v22.20.0 |
| vitest | 4.1.10 |
| crate | `datazen` 0.0.9（`src-tauri`） |
| 桌面应用 | **未运行** |
| webdriver 二进制 | **不存在** |

工作目录（所有命令均在此执行）：

```text
/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat
```

覆盖率原始摘要：`docs/progress/f9-coverage.txt`。

### 1.1 桌面应用 / E2E 前置探测

computer-use MCP：

- `list_running_apps`：访达 / Microsoft Edge / Sublime Text / 微信 / iTerm2 / Cursor / 终端。**无 DataZen。**
- `list_windows`：仅 Cursor（`Cursor Agents`）。**无 DataZen 窗。**

本机探测：

- `ps`：无 `datazen` 应用进程（仅 Cursor extension-host 工作区名含 datazen）
- `target/debug/datazen`：**不存在**
- `target/debug/bundle/macos/DataZen.app/Contents/MacOS/datazen`：**不存在**
- `dist/index.html`：**不存在**
- `127.0.0.1:4445`：**未监听**
- `e2e/.env`：**不存在**（仅有 `.env.example`）

尝试执行（按任务要求）：

```bash
pnpm e2e:skip-build -- --spec e2e/specs/data-sync-window.ts
```

结果：**exit 1**。

```text
[e2e-runner] Skipping build (--skip-build). Binary MUST come from a prior Tauri webdriver build.
[e2e-runner] WARNING: e2e/setup-e2e-env.sh failed. DB specs may fail; UI-only specs can still run.
[e2e-runner] E2E binary not found: .../data-sync-navicat/target/debug/datazen
psql: FATAL: role "postgres" does not exist
```

**结论：Host E2E 与 `data-sync-real` IPC 冒烟均 BLOCKED，不假装 PASS。**

---

## 2. 范围 / 非范围

### 2.1 本切片范围（对照规格）

- 窗口展示 `sync.overwriteRetiredBanner`（`data-testid="data-sync-overwrite-retired"`）
- 未选两端点 Compare → `sync.selectBoth`（`data-testid="data-sync-error"`）；不调用 inspect 成功路径
- idle：`data-testid="data-sync-compare"` 可见；body 含 `sync.selectPrompt`；Apply 此时不出现
- Compare 成功后至少一行 `data-testid="data-sync-mapping-row"`；MATCHED 可勾选
- 页脚 Apply：`data-testid="data-sync-start-disabled"` 且 **disabled**；title 为 `sync.applyUnavailable`
- 10 语言 host key：`sync.mapping*` / overwrite / `applyUnavailable` 均非 raw key；`mappingSummary` 插值
- Host E2E：`e2e/specs/data-sync-window.ts` DSW-001~005 + DSW-MAP-001
- 真实 IPC：`e2e/specs/data-sync-real.ts` `SYNC-INSPECT-001`（两 PG → 含 `sync_batch_a` 且 MATCHED）

| ID | 层 | 意图 | 已有自动化 |
|---|---|---|---|
| UT-DSW-01 | TS DataSyncWindow | 横幅 + idle 引导；Apply 不出现 | `shows the overwrite-retired banner and idle prompt` |
| UT-DSW-02 | TS | 空 Compare → `sync.selectBoth`；不调 inspect | `prompts to select both endpoints when Compare is clicked empty` |
| UT-DSW-03 | TS | 同族两端 Compare → mapping row；Apply disabled | `inspects same-family connections and keeps Apply disabled` |
| UT-DSW-04 | TS | 异构 Target 标 unsupported | `marks heterogeneous targets as unsupported` |
| UT-DSW-05 | TS | connect / inspect 失败对话框 | `shows inspect errors and connect failures` |
| UT-DSW-06 | TS | MATCHED 勾选 + Select All / Deselect All；footer selected | `toggles MATCHED rows and select-all / deselect-all` |
| UT-MV-01~03 | TS mappingView | label / 改名显示 / 摘要分桶 | mappingView 3 cases |
| UT-I18N-DS | TS locales | 10 语 Data Sync keys 非 raw；`mappingSummary` 插值 | `resolves Data Sync workspace keys` + `interpolates sync.mappingSummary` |
| UT-PAIR | TS syncPairing | V1 同族 / 异构 / sqlite 非 V1 | 8 cases |
| E2E-F9-01~08 | E2E | 开窗 / Compare / mapping / Apply / inspect IPC / 10 语 / 主页入口 | **BLOCKED**（无应用 / 无 webdriver 二进制） |

### 2.2 非范围

- 不修代码、不改产品逻辑、不 commit
- 不跑 `pnpm tauri build --debug --features webdriver`（无既有二进制则 BLOCKED，不新建构建）
- 行 Diff / ChangeSet / SQL Preview / `execute_data_sync` 接线（后续切片）
- 驱动方言 / Redis / Kiwi 专属 UI
- Rust `inspect.rs` 覆盖率（属 F8；本切片门槛是 TS 两文件）

F8 遗留缺陷在本切片代码审查中的状态（不重新编号为 F9-BUG）：

- **F8-BUG-01**（Select All 清空已选）：`selectAll` 现绑定 `mappingResults` 中 MATCHED。UT-DSW-06 PASS → **已修复**。
- **F8-BUG-02**（页脚 total=0）：footer 现用 `mappingResults.length`。UT-DSW-06 断言 `data-sync-selected` 含 `"selected":1` → **已修复**。

---

## 3. 单元测试

### 3.1 命令与结果

```bash
cd /Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat

npx vitest run src/windows/data-sync/__tests__/DataSyncWindow.test.tsx \
  src/windows/data-sync/__tests__/mappingView.test.ts \
  src/locales/locales.test.ts \
  src/lib/__tests__/syncPairing.test.ts
```

| 命令 | 通过 | 失败 | 忽略 | 结果 |
|---|---:|---:|---:|---|
| vitest 4 files（verbose 复核） | **33** | 0 | 0 | **PASS**（1.39s） |

分文件：

| 文件 | 通过 | 失败 |
|---|---:|---:|
| `DataSyncWindow.test.tsx` | 6 | 0 |
| `mappingView.test.ts` | 3 | 0 |
| `locales.test.ts` | 16 | 0 |
| `syncPairing.test.ts` | 8 | 0 |

失败详情：无。

### 3.2 已落地单测清单

| ID | 测试名 | 覆盖规格 | 结果 |
|---|---|---|---|
| UT-DSW-01 | `shows the overwrite-retired banner and idle prompt` | `data-sync-overwrite-retired` 文案 `sync.overwriteRetiredBanner`；`sync.selectPrompt`；Compare 可见；Apply testid 不存在 | PASS |
| UT-DSW-02 | `prompts to select both endpoints when Compare is clicked empty` | `data-sync-error` 含 `sync.selectBoth`；`inspectDataSync` 未调用 | PASS |
| UT-DSW-03 | `inspects same-family connections and keeps Apply disabled` | 两 PG → inspect；2 行 `data-sync-mapping-row`；MATCHED / UNMAPPED_SOURCE；`data-sync-start-disabled` disabled + title `sync.applyUnavailable` | PASS |
| UT-DSW-04 | `marks heterogeneous targets as unsupported` | MySQL target 选项含 `sync.unsupportedHint` | PASS |
| UT-DSW-05 | `shows inspect errors and connect failures` | connect 失败 → `sync.connectFailed`；inspect reject → 错误文案 | PASS |
| UT-DSW-06 | `toggles MATCHED rows and select-all / deselect-all` | 取消 MATCHED → Select All 恢复 1；Deselect All → 0；INCOMPATIBLE 展示 reason；`data-sync-path` = `sync.pathDirect` | PASS |
| UT-MV-01 | `labels every mapping status` | 五 status → 对应 `sync.mapping*` | PASS |
| UT-MV-02 | `displays renamed mappings` | `customers → clients`；同名；仅 target | PASS |
| UT-MV-03 | `summarizes mapping buckets` | 1 matched + 1 incompatible + 2 unmapped | PASS |
| UT-I18N-DS-01 | `resolves Data Sync workspace keys for every locale` | 10 语 × 13 key（含 6×`sync.mapping*`、`overwriteRetiredBanner`、`applyUnavailable`、`selectBoth`、`windowTitle`）非空且 ≠ raw key | PASS |
| UT-I18N-DS-02 | `interpolates sync.mappingSummary placeholders in every locale` | `{matched}/{incompatible}/{unmapped}` 替换为 3/1/2，无残留 `{` | PASS |
| UT-I18N-parity | `keeps host key parity with en across all locales` | 10 locale 与 en 奇偶一致 | PASS |
| UT-PAIR-01~08 | syncPairing V1 | PG 同族 direct；mysql/mariadb 同族；跨方言 IR 不可选；sqlite/kv 非 V1 | PASS |

### 3.3 静态对照（不改代码）

`handleCompare`：未选两端 → `sync.selectBoth` 后 return；成功路径只调 `syncCommands.inspectDataSync`；MATCHED 用 `displayTableName` 自动勾选；非 MATCHED checkbox `disabled`。页脚 Apply **硬编码 `disabled`** + `title={t('sync.applyUnavailable')}` + `data-testid="data-sync-start-disabled"` + `onClick={() => undefined}`。顶栏横幅 `data-testid="data-sync-overwrite-retired"`。idle 不渲染 Apply。

10 语言（en / zh-CN / zh-TW / de / es / fr / ja / ko / pt-BR / ru）均有且非 raw：

- `sync.mappingMatched` / `UnmappedSource` / `UnmappedTarget` / `Disabled` / `Incompatible` / `mappingSummary`
- `sync.overwriteRetiredBanner` / `sync.applyUnavailable`

`mappingSummary` 均含 `{matched}` `{incompatible}` `{unmapped}`，与 `summarizeMappings` 对齐；UT-I18N-DS-02 已断言插值。

---

## 4. 覆盖率

```bash
npx vitest run --coverage \
  --coverage.include='src/windows/data-sync/DataSyncWindow.tsx' \
  --coverage.include='src/windows/data-sync/mappingView.ts' \
  src/windows/data-sync/__tests__/DataSyncWindow.test.tsx \
  src/windows/data-sync/__tests__/mappingView.test.ts
```

| 文件 | Stmts | Branch | Funcs | **Lines（门槛）** |
|---|---|---|---|---|
| `DataSyncWindow.tsx` | 90.67%（107/118） | 87.14%（61/70） | 93.75%（30/32） | **92.15%（94/102）** |
| `mappingView.ts` | 92.85%（13/14） | 93.33%（14/15） | 100%（6/6） | **92.85%（13/14）** |
| **合计（include）** | **90.9%（120/132）** | 88.23%（75/85） | 94.73%（36/38） | **92.24%（107/116）** |

**门槛：上述 include 的 Lines ≥ 80% → 两文件均满足 → PASS。**

未覆盖行见 `f9-coverage.txt`。缺口（不改本切片结论）：`cannotSame` 同连接 Compare、get_connections 失败、非法 mapping status default 臂、Apply 死 `onClick`。

---

## 5. E2E 用例表

现状：**全部 BLOCKED**。原因叠加：

1. computer-use 确认无 DataZen 窗口 / 进程
2. 无 Tauri webdriver debug 二进制（`pnpm e2e:skip-build` 在 `assertBinaryReady` 失败）
3. 无 `dist/index.html`
4. 无 `e2e/.env`；`setup-e2e-env.sh` 因 `role "postgres" does not exist` 失败（额外挡住 E2E-F9-06 夹具）

未启动应用，未执行任何 WDIO 断言。**不把单元测试结果记为 E2E PASS。**

| ID | 步骤 | 期望 | 实际 |
|---|---|---|---|
| E2E-F9-01 | 打开 `window.html?window=data-sync` 或主页 Data Sync | 标题 `sync.windowTitle`；`data-testid="data-sync-overwrite-retired"` 可见且文案为当前语言 `sync.overwriteRetiredBanner` | **BLOCKED**（无应用 / 无 webdriver 二进制）。对应 spec：`DSW-001`。UT-DSW-01 覆盖同断言（组件层） |
| E2E-F9-02 | 看 Compare / 引导 | `data-testid="data-sync-compare"` 可见；body 含 `sync.selectPrompt`；Apply 按钮此时不出现 | **BLOCKED**。对应 spec：`DSW-002`。UT-DSW-01 覆盖 |
| E2E-F9-03 | 不选 Source/Target 点 Compare | `data-testid="data-sync-error"` 含 `sync.selectBoth`；不调用 inspect 成功路径 | **BLOCKED**。对应 spec：`DSW-003`。UT-DSW-02 覆盖 |
| E2E-F9-04 | 选同族两端（两 PG 或两 MySQL）点 Compare | 至少一行 `data-testid="data-sync-mapping-row"`；MATCHED 可勾选 | **BLOCKED**。对应 spec：`DSW-MAP-001`（且该 spec 在无 PG 选项时 **静默 return，不 fail** — 有应用后仍须盯假绿）。UT-DSW-03 覆盖 |
| E2E-F9-05 | Compare 成功后看页脚 | `data-testid="data-sync-start-disabled"` **disabled**；title 为 `sync.applyUnavailable` | **BLOCKED**。对应 spec：`DSW-MAP-001` 后半。UT-DSW-03 覆盖 |
| E2E-F9-06 | IPC `inspect_data_sync` 两 PG（`data-sync-real.ts` SYNC-INSPECT-001） | 含 `sync_batch_a` 且 status MATCHED | **BLOCKED**（无 webdriver 二进制 + 无 `e2e/.env` + `role "postgres" does not exist`，夹具不可用） |
| E2E-F9-07 | 10 语言 host key | `sync.mapping*` / overwrite / applyUnavailable 均非 raw key | **BLOCKED**（无应用内切语言）。**单元层已覆盖**：UT-I18N-DS-01 对 10 locale × 上述 key 断言 `text !== key` 且非空 → 该规格在 UT 为 PASS，不记 E2E PASS |
| E2E-F9-08 | 主页 `action.dataSync` 打开窗 | 新窗口出现横幅 + Compare | **BLOCKED**。对应 spec：`DSW-005` |

### 5.1 失败则重现步骤

E2E 未跑到断言，无 FAIL 重现。解除 BLOCKED 的前置：

1. 在本 worktree 执行 `pnpm tauri build --debug --features webdriver`（或等价 `scripts/e2e-tauri-build.mjs`），得到 `target/debug/datazen` 或 macOS `.app` bundle
2. 复制 `e2e/.env.example` → `e2e/.env`，填入本机 PG/MySQL（当前 Homebrew PG 无 `postgres` 角色）
3. `pnpm e2e:skip-build -- --spec e2e/specs/data-sync-window.ts`
4. 夹具可用后再跑 `e2e/specs/data-sync-real.ts` 中 `SYNC-INSPECT-001`（及 overwrite refuse 相关 SYNC-REAL-002 / SYNC-BATCH-001，非本切片门槛但同文件）

---

## 6. 缺陷列表

无。本切片 P0（横幅、空 Compare `selectBoth`、同族 Compare 见 mapping、Apply 禁用、10 语 key）在单元测试与静态审查中与规格一致。F8-BUG-01 / F8-BUG-02 已在当前 `DataSyncWindow.tsx` 修复，不重复开单。

**不记缺陷**（测试/后续切片）：

1. `DSW-MAP-001` 在找不到 PostgreSQL/MySQL 选项或 0 行 mapping 时 `return` 而非 fail — E2E 假绿风险，属 spec 质量，待有应用后收紧。
2. `handleCompare` 的 `sync.cannotSame` 被 UI 禁同一 connection 挡住，无组件单测击中；后端仍有自同步 Validation（F8）。
3. Apply `onClick={() => undefined}` 死代码；按钮 disabled，用户点不到。等 Execute 接线。
4. `syncState === 'syncing' | 'done'` 仍存在，本切片无 Start Sync 路径。

---

## 7. 总评

| 门槛 | 结果 |
|---|---|
| `npx vitest run` 指定 4 files | **33 passed / 0 failed** |
| `DataSyncWindow.tsx` Lines ≥80% | **92.15%**（94/102）；Stmts **90.67%**（107/118） |
| `mappingView.ts` Lines ≥80% | **92.85%**（13/14）；Stmts **92.85%**（13/14） |
| include 合计 Lines / Stmts | **92.24%** / **90.9%** |
| 10 语言 `sync.mapping*` / overwrite / applyUnavailable | UT-I18N-DS-01 **PASS**（非 raw key） |
| 与 P0 规格冲突的产品缺陷 | **无** |
| E2E E2E-F9-01~08 | 全部 **BLOCKED**（无桌面应用、无 webdriver 二进制；E2E-F9-06 另缺 PG 夹具） |
| 记入缺陷 | **0** |

**总评：PASS**

F9 Diff Workspace 壳满足：Compare 走 `inspect_data_sync`；映射列表按 status 渲染；Apply 禁用；覆盖拷贝横幅保留；10 语言 mapping / applyUnavailable / overwrite keys 由 locales 单测钉死；两文件行覆盖均 ≥80%。E2E 因本机无 DataZen / 无 webdriver debug 二进制全部 BLOCKED，按任务约定**不单独导致 FAIL**。有应用后优先跑 DSW-001/002/003/005/MAP-001 与 SYNC-INSPECT-001，并收紧 MAP-001 的静默 skip。
