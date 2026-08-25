# ID 术语统一重构 — 进度管理文件

> 分支：`feature/db-session-id-rename`（worktree：`.worktrees/db-session-id-rename`）
> 约定：**`connectionId` = 配置连接 id（持久化，原 `configId`）**；**`dbSessionId` = 运行时数据库会话 id（内存态，原 `connectionId`）**
> 流程：每个工作项 = 编码 agent 开发（含单测）→ commit → 全新测试 agent 测试（输出 E2E 用例与结果、覆盖率 ≥80%，只测不修）→ commit → bug 按 `验证不通过 → 待验证 → 已修复` 闭环。

## 一、功能工作项

| # | 工作项 | 范围 | 状态 | 完成时间 | 备注 |
|---|--------|------|------|----------|------|
| W1 | 后端核心与服务层术语落地 | `services/connection_manager.rs` 内部命名（`config_id_map`→`session_owner_map` 等）、错误信息区分两种 id；IPC 契约暂不变 | ✅ **已完成（测试通过，0 缺陷）** | 2026-08-24 | 26 文件 +482/-278；核心模块行覆盖 86.94%~96.74%（≥80% 达标）；185 个 IPC 命令签名零变化；报告见 `test-reports/W1-test-report.md` |
| W2 | IPC 契约切换（前后端原子批） | Tauri 命令参数改名 + `src/commands/*` 封装同步 + 全部前后端调用点 + **MCP 参数直接改名 `connection_id`（不留 `config_id` 别名）** + **SQLite 列与持久化字段直接改名**（历史库按既有迁移模式做一次性列重命名） | ✅ **已完成**（复测通过，BUG-001/002/003 已修复） | 2026-08-24 | 开发 b962b4cc → 测试不通过 ce2ef15f → 修复 d20aa93a → 复测通过；⚠️ e2e:minimal 环境受阻转收尾强制项 |
| W3 | 前端状态/类型/组件改名 | `types/index.ts`、stores、`connectionViews/types.ts`、组件 props、跨窗口事件 payload、windowManager、extensionBridge 显式目标 | ✅ **已完成**（复测通过，BUG-004/005 已修复） | 2026-08-24 | 开发 80df9968 → 测试通过附 P2×2（31b92f29）→ 修复 a70f5c19 → 复测通过；八链路语义审计无装反；新发现 BUG-006 转独立闭环 |
| W4 | 外部契约与文档对齐 | 新建 CHANGELOG.md（6 条破坏性变更+迁移指引）、活文档清扫 9 文件、MCP 资源输出字段收口 | ✅ **已完成**（测试通过；P3 遗留转 BUG-007 随 W5 修） | 2026-08-24 | 实际 12 文件（含进度文件）；CHANGELOG 六条 6/6 与代码相符；示例抽查 6/6 一致；报告见 `test-reports/W4-test-report.md` |
| W5 | 文档与守护 | `docs/architecture/naming.md`、AGENTS.md 精简更新、lint/grep 守护规则 | 未开始 | - | |

收尾：回归测试（**含强制补跑 `pnpm e2e:minimal`**——W2 复测因沙箱 EPERM 环境受阻，报告 R5；合并 main 前必须在无沙箱限制环境通过）→ main 合入 feature 并复验 → 文档更新 → 合并 main。

## 二、Bug 跟踪

状态流转：`验证不通过`（测试发现）→ `待验证`（编码 agent 已修复）→ `已修复`（测试 agent 验证通过）

| Bug ID | 发现于 | 描述 | 重现步骤 | 状态 | 关联工作项 |
|--------|--------|------|----------|------|------------|
| BUG-001 | W2 独立测试 T2 | **13 个命令语义装反**：ai 域 7 个 + backup 域 6 个命令，参数名 `connection_id`（配置语义）实际按运行时会话语义使用（直调 `get_session`、无 resolve_session 双模）；当前靠前端把 dbSessionId 装在 connectionId 键里碰巧可用，W3 改造时必断 | 详见 `test-reports/W2-test-report.md` §T2 缺陷 D1（含 13 个命令清单） | 已修复 | W2 |
| BUG-002 | W2 独立测试 T3 | `commands/connection.rs` 行覆盖率 **65.37% < 80%** 验收线（其余核心模块达标：history_db 91.14%、mcp/server 88.11%、driver_command 81.95%、query 80.62%） | `cargo llvm-cov -p datazen --lib --summary-only` 复测 | 已修复 | W2 |
| BUG-003 | W2 独立测试 T2/G1 | history_db 迁移链缺**存量 v3 库（config_id 列）起点**的固化回归测试（走读与三起点 SQL 等价执行均 PASS，但无自动化测试保护） | 报告 §T2 缺口 G1 | 已修复 | W2 |
| OBS-001 | W2 独立测试 T1 | store 并发用例负载型偶发失败（隔离重跑 5/5 过；代码 vs main 零改动；main 基线同现象）——既有环境波动，不计缺陷 | 隔离单跑该用例即可复现通过 | 观察 | - |
| BUG-004 | W3 独立测试 T5/D2 | **P2 测试层**：`ConnectionNavigatorTree.test.tsx` 仍传旧 prop `activeConfigId`（组件已改名 `activeConnectionId`）；因 tsconfig exclude 测试文件致 tsc 不报、用例仍绿，但选中态覆盖被静默削弱 | 打开该测试文件查看传参；对照组件 props 定义 | 已修复 | W3 |
| BUG-005 | W3 独立测试 T5/D3 | **P2 e2e 层**：`e2e/specs/data-sync-real.ts` 本地 SyncTask 接口/载荷残留 `sourceConfigId/targetConfigId` 且缺 `sourceDbSessionId/targetDbSessionId`，后端强类型反序列化必挂——GUI E2E 实跑时 SYNC-BATCH-004 失败 | 实跑 data-sync-real SYNC-BATCH-004；或比对本地接口与后端 `commands/sync/types.rs` 字段 | 已修复 | W3 |
| OBS-002 | W3 独立测试 T5 | clearCaches/dbObjectsMap 键空间错位——main 上同构存在（继承性观察，非本次引入），登记待后续工作项评估 | 见 W3 测试报告 §T5 | 观察 | - |
| OBS-003 | W3 独立测试 T3 | vitest coverage 门禁 exit 1（10 条阈值 ERROR）在 main 基线同样存在——既有问题非本次引入 | main 上跑 `npx vitest run --coverage` 对照 | 观察 | - |
| BUG-006 | W3 复测补充审计 R4/D5 | **P1 功能性断裂（W2 清尾遗漏）**：ExportTablesRequest 后端字段已是 `db_session_id`（export.rs L83，resolve_session 双模 L563），前端 `commands/file.ts` 仍以 `connectionId` 键传会话 id、`lib/batchExportJob.ts` 以 `{connectionId: dbSessionId}` 构造并带误导注释——serde 必报 missing field，**多表批量导出当前 HEAD 必挂**；两侧既有测试因 mock/原生构造均无法拦截 | 实跑多表批量导出；或比对 file.ts 接口键与 export.rs 结构体 | 已修复 | W2 |
| OBS-004 | W3 复测补充审计 R4/OBS-3 | WorkflowChatPanel 将配置 id 塞入 `AiInput.dbSessionId`→ai_chat，后端 get_session 严格查找 + if-let-Ok 静默降级兜底：不硬错但 schema 增强静默失效（main 上行为等价）。改进建议：FE 取活动会话 id 或后端改 resolve_session | 见 W3 测试报告 R4 节 | 观察 | - |
| OBS-005 | W3 复测 T1/P3 | `e2e/specs/data-transfer-window.ts(211)` 悬空标识符 `tgtDbSessionId`（L208 实为 tgtConn），TS2304——W3 提交 80df9968 引入的一行笔误，e2e 错误总数 68 vs main 基线 67 的唯一增量 | e2e tsconfig 报错列表 | 观察（随 BUG-006 一行修掉） | W3 |
| BUG-007 | W4 独立测试 T5/DEFECT-1 | **P3 既有遗留**：`docs/TODO-screenshots.md:35` 描述 toggleDb 参数为 configId+connectionId+dbName，实际签名为 (connectionId, dbSessionId, dbName)——该文件不在归档排除范围、未被 W4 触碰，属活文档残留 | 打开该文件比对实际函数签名 | 验证不通过 | W5 一并修 |

## 三、测试记录

### E2E / 单元测试用例与结果

| 轮次 | 工作项 | 用例摘要 | 结果 | 覆盖率 | 测试 agent |
|------|--------|----------|------|--------|------------|
| D1（开发自测） | W1 | 不变式：驱逐自动重连保持 db_session_id；resolve_session 双模解析；错误文案区分两种 id | `cargo test -p datazen --lib`：1115 通过 / 2 失败（既有 sandbox 环境问题，干净 HEAD 复现，非本次引入） | 待独立测试 agent 评估 | 编码 agent ad05a2ae |
| T1（独立测试） | W1 | 10 条 E2E 视角用例（6 条已在 Rust 单元/IPC 等价层实际执行通过）；独立复核 lib 测试 1115 过 / 2 失败（既有环境问题，基线实机比对确认） | **通过**；llvm-cov 行覆盖：connection_manager 86.94%、db_tools 96.74%、query_executor 89.40%（核心模块 ≥80% 达标，TOTAL 77.03%）；缺陷 0 | 全新测试 agent 1c7f5916 |
| D2（开发自测） | W2 | 九域 IPC 契约切换；history_db v4 迁移环；MCP 旧键拒绝守护测试 | cargo lib：1115 过 / 2 既有失败；vitest **239 文件 / 1886 用例全绿**；build 分步全过（pnpm 包装器受 sandbox node-gyp 缓存 EPERM 限制，逐条等价执行成功） | 待独立测试 agent 评估 | 编码 agent 977851e6 |
| T2（独立测试） | W2 | 12 条 E2E 视角用例全部给出状态；Rust 层定向 29 单测全过 + 迁移三起点 SQL 等价执行 PASS；语义方向审计发现 BUG-001 | **不通过**：BUG-001 语义装反 ×13、BUG-002 connection.rs 覆盖率 65.37%<80%、BUG-003 缺 v3 迁移回归测试；其余全绿（lib 1114 过/vitest 1886 绿/drivers 84 绿/tsc+vite ✓）；复测条件=①D1 修复+守护测试 ②覆盖率≥80% ③e2e:minimal | 全新测试 agent dc7ba786，报告 `test-reports/W2-test-report.md` |
| F1（修复轮自测） | W2 | BUG-001：13 命令全链改名 db_session_id + 5 条防装反守护测试，程序化扫描装反残留=0；BUG-002：connection.rs 新增 4 测，覆盖 65.37%→82.42%；BUG-003：v3 有数据起点 + v2 空表起点迁移锚点测试 | cargo lib **1126 过** / 2 既有失败；vitest 1886 绿；tsc 零错 + vite ✓；llvm-cov connection.rs **82.42% ≥80%** | 编码 agent 977851e6 |
| T2R（复测） | W2 | 独立重扫装反=0；含 connection_id 命令仅剩 7 条且全为配置语义无误改；前后端键一致性抽查 4 项过；迁移锚点实测通过 | **通过**（BUG-001/002/003 → 已修复）；lib 1125 过 / vitest 1886 绿 / drivers 84 绿 / tsc+vite ✓；⚠️ 附带条件：e2e:minimal 因沙箱 EPERM 环境受阻（报告 R5），**转为收尾回归强制项：合并 main 前必须在无沙箱限制环境补跑通过** | 复测 agent dc7ba786，报告已追加「复测轮」章节 |
| D3（开发自测） | W3 | 五域前端改名：store 核心 / lib 层（含 D4 插件桥直改）/ 组件 props 链 / redis UI 适配 / e2e 清尾 | host vitest **1886 绿**；drivers vitest **84 绿**；tsc 零错 + vite ✓；configId 残留 1 处（schemaDiff.ts 历史注释，合理保留）；Rust 零改动 | 待独立测试 agent 评估 | 编码 agent 07f37e84（中止于汇报前，门禁由编排方代跑） |
| T3（独立测试） | W3 | 12 条用例（10 条 vitest/mock 层实际执行：定向 15 文件 174 用例 + redis 8 用例）；八链路语义审计全 ✅ 无装反；变体扫描发现 BUG-004/005 | **通过**（附 2 项 P2 清尾缺陷 → BUG-004/005 闭环中）；覆盖率：activeConnectionStore 91.83、extensionBridge 99.32、windowManager 92.85 达标，panelStore 69.91（与 main 基线一致，继承性不足）；TOTAL lines 81.84 | 全新测试 agent 03c401c9，报告 `test-reports/W3-test-report.md` |
| F2（修复轮自测） | W3 | BUG-004：4 处旧 prop 改名 + 新增选中态用例（反向注入实验证明缺陷状态下必失败）；BUG-005：复核并收尾 data-sync-real.ts（会话变量改名 ~60 处、legacy 载荷错位清零），e2e ConfigId 变体=0 | host vitest **1887 绿**（含新用例）；drivers 84 绿；host tsc 零错；e2e tsc 68 ≤ 基线（data-sync-real.ts 0 错） | 编码 agent 07f37e84 |
| T3R（复测） | W3 | BUG-004：全仓扫描 0 命中 + 反向注入实验证实用例判别力真实，文件 11 用例全过；BUG-005：SyncTask 15/15 字段对照一致、值语义各就各位、变体扫描=0、data-sync-real 零类型错误；补充审计发现 BUG-006（P1）+ OBS-004/005 | **通过**（BUG-004/005 → 已修复）；回归门禁：host vitest 1887 绿 / drivers 84 绿 / host tsc 零错 | 复测 agent 03c401c9，报告已追加复测轮 + R4 节 |
| F3（修复轮自测） | W3/W2 | BUG-006：file.ts/batchExportJob.ts 键改 dbSessionId + 3 条 IPC 契约守护测试（键集合双向断言/反向断言/静态锚点）；OBS-005 一行修复 | host vitest **1890 绿**（净增 4）；host tsc 零错；e2e tsc 72→71 | 编码 agent 977851e6 |
| T4R（复测） | W3/W2 | 修复正确性核对（构造点唯一、注释清理）；守护测试反向注入实验（改回旧键→5 用例失败含 2 条契约守护→恢复后 diff 为空）；OBS-005 修复后 e2e tsc 67=main 基线 | **通过**（BUG-006 → 已修复）；host vitest 1890 绿 / drivers 84 绿 / host tsc 零错 | 复测 agent 03c401c9，报告已追加最终复测节 |
| D5（开发自测） | W4 | CHANGELOG 6 条破坏性变更；活文档 29 处 token 替换+示例代码重写；MCP 资源定向加固测试（输出含 connectionId 不含 configId） | lib 1126 过 / vitest 1890 绿 / SEO 脚本触碰文件全过；grep 3 处全为合法历史演进说明 | 待独立测试 agent 评估 | 编码 agent 4119a5df |
| T5（独立测试） | W4 | CHANGELOG 六条逐一对照代码现实 6/6 相符；文档示例抽查 6/6 一致；zh/en 与 site 双语平行；10 条清单 9 过 1 警示；mcp/server.rs 行覆盖 87.60% ≥80% | **通过**；lib 1126 过 / vitest 1890 绿；发现 BUG-007（P3 既有遗留）+ OBS×4 | 全新测试 agent 9f275212，报告 `test-reports/W4-test-report.md` |

## 四、提交记录

| Commit | 说明 |
|--------|------|
| c76118f9 | 进度文件初始化 |
| f0aa9882 | W1 开发里程碑：后端核心/服务层改名 + 单测 + 进度更新 |
| 09b9d5cc | W1 测试里程碑：独立测试报告（通过，0 缺陷，覆盖率达标）+ 进度更新 |
| b962b4cc | W2 开发里程碑：九域 IPC 契约切换 + history_db v4 迁移 + MCP 无别名直改 + 驱动 UI 单测补跑（84/84）+ 进度更新（含 D3 附带变更） |
| ce2ef15f | W2 测试里程碑：独立测试**不通过**，登记 BUG-001/002/003（验证不通过）+ 测试报告 + 进度更新 |
| d20aa93a | W2 修复里程碑：BUG-001/002/003 修复（31 文件 +685/-209，净增 11 条 Rust 测试），bug 状态 → **待验证** |
| 87ceed86 | W2 复测里程碑：复测**通过**，BUG-001/002/003 → 已修复，W2 → 已完成；e2e:minimal 转收尾强制项 + 进度更新 |
| 80df9968 | W3 开发里程碑：前端五域改名（含 D4 插件桥直改）+ 门禁代跑确认 + 进度更新 |
| 31b92f29 | W3 测试里程碑：独立测试**通过**（八链路语义审计无装反），登记 BUG-004/005（P2 清尾，验证不通过）+ OBS-002/003 + 测试报告 + 进度更新 |
| a70f5c19 | W3 修复里程碑：BUG-004/005 修复（选中态区分性用例 + data-sync-real 载荷对齐后端契约），bug 状态 → **待验证** |
| 4568546c | W3 复测里程碑：复测**通过**，BUG-004/005 → 已修复，W3 → 已完成；新登记 BUG-006（P1）+ OBS-004/005 + 进度更新 |
| 897ce98a | BUG-006 修复里程碑：ExportTablesRequest 键对齐 + 3 条守护测试 + OBS-005，bug 状态 → **待验证** |
| 5bd0623b | BUG-006 复测里程碑：复测**通过**，BUG-006 → 已修复 + 进度更新 |
| d3f67525 | W4 开发里程碑：CHANGELOG + 活文档对齐 + MCP 资源收口 + 进度更新 |
| （本次） | W4 测试里程碑：复测**通过**（CHANGELOG 6/6 相符、示例 6/6 一致）；登记 BUG-007（P3 既有遗留，随 W5 修）+ 进度更新 |

## 五、决策记录

| # | 日期 | 决策 | 来源 |
|---|------|------|------|
| D1 | 2026-08-24 | W2 范围修订：① MCP 工具参数直接改名 `connection_id`，**不**保留 `config_id` 兼容别名（接受外部契约破坏性变更，W4 文档中记录 CHANGELOG）；② SQLite 列名与持久化/配置文件字段名直接改为新术语，**不**做双轨兼容；`query_history`/`favorite_queries` 沿用 history_db 既有的一次性列重命名迁移模式处理存量库。原「保留别名 + 保留旧列名」方案作废。 | 用户指示 |
| D2 | 2026-08-24 | 合并冲突策略：main ↔ feature 双向合并遇冲突时，**两边修改都要保留**——把 main 侧的新逻辑/修复按本分支新术语（connectionId/dbSessionId）适配后融入，同时不丢失本分支的改名成果；不做"二选一"式解决。无法字面并存处（如同名标识符）以语义融合方式落地，并在进度文件登记具体取舍。收尾顺序：回归通过 → main 合入 feature 并复验编译/测试 → 文档更新 → feature 合回 main。 | 用户指示 |
| D3 | 2026-08-24 | W2 附带变更两项：① `packages/drivers/redis/ui` 16 文件将 invoke 参数键提前改为 `dbSessionId`（经用户许可的直接改名，宿主 ConnectionViewProps 契约保持至 W3）；② 修复既有环境缺陷 `src/test/setup.ts`（jest-dom 副作用导入在本环境注册为空匹配器集，干净 HEAD 上即导致 285 用例 "Invalid Chai property" 失败），改为显式 `expect.extend`，全量转绿。 | 编码 agent 报告 |
| D4 | 2026-08-24 | W3 范围界定：插件可见桥接协议键随 W3 直接改名 `configId`→`connectionId`（extensionBridge / plugin-sdk / packages/extensions 示例包 / fixtures 同步），**不留兼容别名**——沿用 D1「外部契约不做双轨兼容」原则；破坏性变更由 W4/W5 文档阶段记入 CHANGELOG。宿主向 execute_driver_command 传参改为 `dbSessionId`（真实会话 id 或经 resolve_session 双模的配置 id）。 | 编排方依据 D1 原则推导 |
