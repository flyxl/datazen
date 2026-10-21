# progress · Wave 3 轨 `insert-frontend`

> 轨：04 分册（插入）前端部分 = Step 7（store）+ Step 8（UI）。
> 分支 `feature/insert-frontend`，起点 `9052e8235`（Wave 1 + Wave 2 集成分支）。
> **本文件是开发期台账，验收合并后必须删除**（AGENTS.md「进度台账：开发期间允许，交付即销毁」）。

## 自证

| 项 | 状态 | 落点 |
| --- | --- | --- |
| Step 7 · 三态写入载体（无 `CellWrite`、无三态枚举） | ✅ 完成 | `src/lib/tableChanges.ts`（`PendingInsert` / `draftWriteColumns`） |
| Step 7 · `TableState` 三个新字段 | ✅ 完成 | `src/stores/tableData/types.ts` |
| Step 7 · `pendingWorkSignature` / `hasPendingWork` / `draftsForWire` | ✅ 完成 | `src/stores/tableData/pendingChanges.ts` |
| Step 7 · 草稿纯函数与 store slice | ✅ 完成 | `src/stores/tableData/draftInserts.ts`（新） |
| Step 7 · 草稿动作 + 预览/提交接线 | ✅ 完成 | `src/stores/tableDataStore.ts` |
| Step 7 · 插入行定位（`row_identity` → 最大主键候补 → 降级） | ✅ 完成 | `src/stores/tableData/insertLocation.ts`（新） |
| Step 8 · `VirtualBody` 草稿行渲染 | ✅ 完成 | `src/components/DataTable/VirtualBody.tsx` |
| Step 8 · `TableView` 入口 / 快捷键 / 门闸 / 预览 | ✅ 完成 | `src/windows/connection/TableView.tsx` |
| Step 8 · 只读连接隐藏「+ 行」 | ✅ 完成 | `TableView.tsx`（`isEditable && …`） |
| Step 8 · i18n（英文领域包） | ✅ 完成 | `src/locales/en/query.ts`（16 个 `tableData.*`） |
| Step 8 · `gridErrors.ts` | ⏭️ **未改动**（裁定 1：不登记无生产者的码） | — |
| E2E `e2e/specs/table-insert.ts` | ✅ 已写（**留待 R 回归**：本机无 `target/debug/datazen`） | `e2e/specs/table-insert.ts` |

## 门禁（逐字结论行，HEAD = `c97f42156`）

```
$ npx vitest run
 Test Files  649 passed (649)
      Tests  6756 passed (6756)
EXIT=0

$ pnpm typecheck
EXIT=0
error TS 计数 = 0
（`check:restore-guards` ok；`tsc --noEmit` 干净；`typecheck:scripts` / `typecheck:pack-ep` 干净）
```

`wc -l`（全部 ≤ 800）：

| 文件 | 行数 | 文件 | 行数 |
| --- | --- | --- | --- |
| `src/stores/tableDataStore.ts` | 758 | `src/windows/connection/TableView.tsx` | 723 |
| `src/components/DataTable/VirtualBody.tsx` | 518 | `src/components/DataTable/DataTable.tsx` | 579 |
| `src/stores/tableData/draftInserts.ts` | 206 | `src/stores/tableData/pendingChanges.ts` | 196 |
| `src/stores/tableData/insertLocation.ts` | 158 | `src/windows/connection/useTableDraftRows.ts` | 129 |
| `src/windows/connection/PendingPlanDialog.tsx` | 92 | `src/lib/tableChanges.ts` | 297 |

生产代码零 `any` / `@ts-ignore` / `.skip` / `.todo`；`git status --short -uall` 干净。

## 协调者裁定后的落实（第二轮）

| 裁定 | 落实 |
| --- | --- |
| 1 · `noWritableColumn` 不登记 | 契约已改（集成分支 `14c59ecc7`）。`gridErrors.ts` 仍**一行未改**；新增断言：`classifyGridError('grid.insert.noWritableColumn: …')` → `'unknown'`，且 `GRID_ERROR_CODES` 不含它 |
| 2 · `row_identity` 恒空 | 不改代码。`insertLocation` 只挂在**前端**的 `CommitPendingChangesResult` 上，wire 类型 `CommitPendingChangesResponse`（桌面 + backend-client 镜像）未加字段；候补规则要求候选主键**比提交前更大**；降级提示是 `role="status"` 的中性提示，不出现「已高亮新行」措辞 |
| 3 · `inserts` 必填 | 两处镜像都改成必填：`src/lib/tableChanges.ts` + `packages/backend-client/src/types/tableData.ts`；测试改成 `@ts-expect-error` 的 fail-closed 形态（变异验证：改回可选 → TS2578 → `tsc` EXIT=2 → 还原 EXIT=0） |
| 4 · E2E spec | 新增 `e2e/specs/table-insert.ts`（TC-INS-001~007），**留待 R 回归**，未伪造通过 |

## 第 1 轮复测后的落实

| 编号 | 处理 |
| --- | --- |
| IF-D-1（真缺陷：降级提示永不消失） | **已修**：提示在「定位到了 / 不适用」时被清掉；并且在**页面状态变化**时失效——重新取数（reload / 刷新按钮）、翻页、改页大小、改筛选（应用筛选 / 快速筛选 / 清空筛选）、改排序、回滚、以及换表（contextKey 变化）。新用例 3 条，且做过变异验证（去掉修复后 3 条全红） |
| IF-D-2 | `tableData.highlightNewRow` **已删**（全仓无消费者：高亮走 grid 既有的 `highlightedRow` 通道）。`locales.test.ts` 同步改成断言该 key **不存在**。⚠️ 事实更正：04 §8 的清单里**确实列了** `tableData.highlightNewRow`（`04-insert.md:700`），所以删 key 与 §8 有一处小分歧，需要协调者在契约里一并删掉那一行 |
| IF-D-4 | 3 条被削弱的 E2E 断言已修：`INSERT` → `INSERT INTO`（表名 `_e2e_insert_test` 里就含 "insert"，原断言自满足）；补断言插入条数 `1 inserts`；草稿顺序断言从「长度」改成「顺序本身」（`toEqual([first, second])`） |
| IF-D-3 | **接受**（协调者裁定）：「显式写 NULL」这一态在真实 UI 上不可达——契约把「Set NULL」右键菜单划给 05 分册（`tableData.insertSetNull` 词条已预落），store / wire 层面的三态完整可区分已由复测钉住 |

## 第 2 轮（协调者改判：04 §8 的 `highlightNewRow` 是**要求**，不是可选项）

`tableData.highlightNewRow` **已恢复**并**实装**：网格新增独立 prop `newRowIndex`
（默认 `null`），被标记的那一行渲染 `data-testid="new-row-badge"` 的徽标（文案
`New row`）。背景高亮那条通道与详情面板共用，只有**带文字的标记**能告诉用户
「哪一行是刚插入的」。徽标与草稿行的 `draft-row-badge`（`NEW`，未提交态）区分：
文案与 testid 都不同，样式可复用。`newRowIndex === null` 时 DOM 与今天逐字相同。
（契约侧改动：集成分支 `1a0b37cd9`。）

## 第 3 轮

| 编号 | 处理 |
| --- | --- |
| IF2-D-1 | 「重试」路径（`actions.load`）也补了 `invalidateNotice()`——之前 8 条路径有、这一条没有。新用例 `drops the notice when a failed load is retried`（**变异验证**：抽掉修复后转红） |
| IF2-D-3 | 插入条数的断言加了边界：正则 `/(?:^|\D)1 insert\(s\)(?:\D|$)/`（`"11 insert(s)"` / `"21 insert(s)"` 不满足）。**没有**用分隔符形式 `· 1 insert(s) ·`：预览描述把**原始条数**与插值后的词条并排渲染（`1 1 insert(s)`），分隔符形式要么不匹配、要么把断言钉死在那个重复渲染上。另外补了一条与文案无关的断言：**预览里恰好列出 1 条 INSERT 语句**（数 `<code>` 节点） |
| 词条模板 | `tableData.pendingInserts` 改成 `'{count} insert(s)'`（契约 `ee3409494`；仓库既有约定一律是 `thing(s)`） |
| E3 两条假钉 | ① store 层那条**改成走生产函数** `pendingChangesForWire`（原来在测试里手抄了一遍 wire 转换）；② backend-client 那条**改成走生产客户端**（`createBackendClient` → transport 调用，断言变更集**逐字**透传）+ 一条**编译期**钉（`InsertMarkedStaysOptional`：镜像里 `insertMarked` 必须保持可选）。依据：backend-client 包**不生产**载荷（转换在宿主的 `pendingChangesForWire` 里），所以在包内断言 fixture 的 JSON 证明不了任何生产性质；宿主侧的真钉由复测的 `insertThreeState.audit.test.ts` 承担 |

## 待裁定（只报不改，AGENTS.md「发现契约/实装冲突，停下来报告」）

1. **`grid.insert.noWritableColumn` 没有生产者** —— 04 §8 把它列为本册要追加的码，
   但 Wave 1/2 的后端（`packages/feature-core/src/data_browsing/`）**没有任何
   `grid.insert.noWritableColumn:` 前缀的产生点**（V-3 的 NOT NULL 校验没实现）。
   而 `src/lib/gridErrors.ts` 的文件头规则是「每个登记的码都必须有真实生产者，
   否则前端会分支到一个永远不会到达的前缀」。
   **本轨的处理**：不追加该码（`grid.insert.unknownColumn` 与
   `grid.insert.returningUnavailable` 早已在册，本轨直接消费），因此
   `gridErrors.ts` **一行未改**（「append only」规则天然满足）。
   i18n 的 `tableData.insertNoWritableColumn` 词条按 04 §8 仍然落地（未使用的
   词条无害，翻译补齐脚本会处理）。
2. **`RowChangePlan.inserts` 在 TS 镜像里是可选的**（`inserts?: PlannedStatement[]`），
   而 04 §3.6 写的是「必填数组」。这是 Wave 1/2 落地的镜像形态，本轨**未改**
   （只读消费）。后端 `#[serde(default)]` 序列化时始终输出该字段，所以线上永远是
   数组；前端按 `plan.inserts ?? []` 读。
3. **后端插入结果的 `row_identity` 恒为空**（`write_exec.rs` 里
   `row_identity: change.row_identity.clone()`，而 `canonicalize_insert` 把新行的
   `row_identity` 清空）。所以 04 §9.1 的
   `insert_identity_falls_back_to_written_primary_key_when_returning_is_none`
   在后端**没有实现**，「用户写满主键 → 精确定位」这条路径本批走不到；
   前端因此把「最大主键候补」作为唯一定位手段（并加了一条守卫：候选主键必须
   **比提交前更大**，否则宁可不定位），定位不到就降级为中性提示。
4. **草稿格的「显式写 NULL」本批不可达**：编辑器里清空 = 「不写」（删键），
   显式 NULL 的入口（`Set NULL` 右键菜单）归 05 分册；本轨只落
   `tableData.insertSetNull` 词条。

## 偏离契约的实现选择（已在提交信息里写明）

- `draftInsertCount(ts)` 放在 `src/stores/tableData/draftInserts.ts`（而不是
  04 Step 7 写的 `src/lib/tableChanges.ts`）：`ts` 的类型住在 store 层，放进
  `lib` 会造成 `lib → stores` 的反向依赖。
- `draftsForWire` / `pendingWorkSignature` / `hasPendingWork` 按任务要求放在
  `src/stores/tableData/pendingChanges.ts`；草稿的**变更**纯函数放新文件
  `draftInserts.ts`，`tableDataStore.ts` 只留 3~6 行的接线（04 §6 拆分方案）。
- 草稿行的 UI 状态（当前草稿行 / 正在编辑的草稿格）放在
  `src/windows/connection/useTableDraftRows.ts`（组件态），不塞进 `TableState`——
  `TableState` 严格只加契约规定的三个字段。
- 提交后需要一个清高亮的写入口，因此给 store 多加了一个动作
  `clearHighlightedNewRow(panelId)`（高亮 1.5s 后自动清、翻页/筛选/排序时清）。
