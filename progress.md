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
