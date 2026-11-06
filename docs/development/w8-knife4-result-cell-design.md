# W8 刀4 实施级设计：result-cell 二端点点亮（previewResultCellChange / commitQueryResultCell）

状态：设计档（实施随本档排刀）。前置件全部就位：#189（执行面 SSE + ephemeral
会话注册表）、刀2 #193（origin 读半边 + 活句柄借出 + SchemaCache）、刀3 #192
（变更二成对点亮 + write_exec 共享层修正）、#191（绿路径 + 共享层三处修正：
statementColumns 双认 / read_rows output 词汇表 / 默认库回退）。
11 条 501 已点亮 9 条；本刀落地后 **tableData 域 501 清零**。

## 1. 桌面参照面（`commands/result_write.rs`，383 行）

`commit_query_result_cell(state, ResultCellWrite)` 七步：

1. **binding 校验**：`runtime.get_execution(binding.execution_id)`，要求
   `view.state == Succeeded` 且 `view.runtime_binding == Some(&binding)`——
   「Result requires its original live session」；不满足 → Validation 拒。
2. **会话上下文比对**：`WriteTarget::of(session.observed_context)` 与本次
   期望上下文一致（写不许瞄准已漂移的会话）。
3. **只读预检**：连接 `read_only` → 全拒，码表 `grid.result.originUnavailable`
   同族（`read_only_error_code`）。
4. **origin/映射**：结果的 `WritableMapping`（列→源列 passthrough 投影）+
   `TableSchema` 主键 → 行标识。
5. **plan 构建**：`build_row_change_plan`（共享层，与刀3 同一份）。
6. **执行**：`execute_in_session(binding.handle, …)`——**写回到产出结果的
   同一个活会话**（原会话绑定的本质）；事务语义随会话。
7. **轮询收集**：等终态 → `chunks()` 读产物（rows / statementColumns）。

请求体：`{ binding: RuntimeResultBinding, statementIndex, rowIndex, column,
value }`——行定位是**结果集坐标**（rowIndex），不是主键值。

## 2. server 侧事实源替换（桌面 → server）

| 桌面事实 | server 事实源 |
| --- | --- |
| 进程内 runtime（`adapter.runtime()`） | `state.lease.resolve()`（刀3 同款单次 resolve 贯穿提交序列）+ `TableDataState.sessions` |
| `RuntimeResultBinding`（进程内 execution 台账） | **客户端携带** `dbSessionId` + `executionId`；server 用 `runtime.get_execution(ctx, executionId)` 复验 `runtime_binding` + `state == Succeeded`（execution 台账在共享 sink，跨请求可读——#191 read_rows 修正后产物词汇已统一） |
| `execute_in_session(handle, …)` | 同一共享用例（`RuntimeConnectionUseCases`），handle 来自会话视图；**manualHandle 显式 501 明拒**（刀3 同款裁决，serde 显式反序列化） |
| 结果集坐标行定位 | 客户端携 `rowIndex`（+ `statementIndex`）；server 从产物块重读该行 → 用主键列值构造 `rowIdentity` → 走 `build_row_change_plan`（共享层规则不变） |
| `WritableMapping`（原查询的列映射） | 刀2 origin 读半边同源重推导（`describe_result_origin` + `evaluate_result_editability` 的 writable 面 + passthrough 列）——写前 origin **重读**，不信任客户端声明的映射 |

## 3. 两条 handler（成对点亮，一个 PR）

- `previewResultCellChange`（`POST /api/v1/table-data/result-cell-change-previews`）：
  ①~⑤ 步（binding 复验 → 会话上下文比对 → 只读预检 → origin 重读 + 行定位 →
  plan 预览），输出与刀3 preview 同形状（planId / fingerprint / configRevision
  十进制字符串 / updates）。
- `commitQueryResultCell`（`POST /api/v1/table-data/result-cell-commits`）：
  ⑥⑦ 步（`execute_in_session` + 轮询收集 + rowsAffected 兼容两种产物形状），
  复用刀3 的指纹/CAS/affectedMismatch 五道闸（共享层已重检，宿主不复制）。

模块头状态表翻新为 **10✅/0⛔**；`REASON_ORIGIN` 删除；table_data_routes.rs
探针收窄（两条 URI 移出 501 断言）。

## 4. 错误映射（沿刀3 表）

Validation → 400；`Result requires its original live session` 类（binding 失配/
execution 非 Succeeded/会话不在）→ 404（防枚举口径与四读一致）或 409（会话
尚在但 binding 漂移——诚实区分：session_entry miss → 404；binding 比对失败 →
409 `contextConflict`）。`TransactionResolutionRequired` 保留为显式错误
（rollbackAndClose 语义；本刀不吞）。

## 5. 测试（`tests/result_cell_routes.rs`，新文件）

夹具与 `mutation_pair_routes.rs` 同构 + `table_data_live_routes.rs` 的
「先 resolve cell runtime 再开会话」纪律。用例：

1. t1 绿路径：开会话 → query（`execute_in_session` 产物带 binding）→
   preview（plan 形状钉）→ commit（rowsAffected + 复验计数）。
2. t2 行定位：rowIndex 越界 / 列不在 passthrough 投影 → 400。
3. t3 只读连接 400 `grid.result.*` 码。
4. t4 binding 复验：executionId 陌生 → 404；`runtime_binding` 漂移 → 409。
5. t5 manualHandle 显式 501；缺省 = owned-session 绿路径。
6. t6 提交期 CAS（revision 漂移 → stalePlan；重 preview 对照腿）。
7. t7 跨组织 404 固定文案 / 无 cookie 401。
8. t8 歧义写 affectedMismatch + 驱动零写调用。
9. t9 幂等重放全链。

MockDriver 供血沿用刀2 登记（table_schemas_by_database / parameterized_writes /
count_total 分支）+ 事务三方法真实现（刀3 的 `TransactionalMockProvider` 先例）。

## 6. 风险与开放问题

1. **rowIndex 的时点语义**：结果集行序在产物块间是否稳定（分页/截断）——
   预览时按产物重读定位，commit 以 fingerprint 复验整体一致性；行序漂移
   → fingerprint 失配 → 400 stalePlan（诚实失败）。
2. **execution 台账的 TTL**：sink 的 86400s 回收与原会话存活的交叠窗口；
   binding 失配路径（409）已覆盖会话重建场景。
3. **写执行授权基线**（刀3 开放问题的延续）：result-cell 写同受
   `(Write, Execution)/(Write, Connection)` 闸；OrgMember 授权决策仍是
   产品开放问题，随刀3 登记一并上交。
