# Data Transfer 用户指南

> 产品定义见 [data-transfer-prd.zh-CN.md](./data-transfer-prd.zh-CN.md)。  
> UI 设计存档见 [data-transfer-ui-redesign.zh-CN.md](./data-transfer-ui-redesign.zh-CN.md)（6 步向导 · 已实施）。
> 与 **Data Synchronization**（同族 + 结构一致 + 相同 PK 的行级 Diff）和 **Schema Diff**（结构对齐）是独立产品。

## 入口

- **Tools → Data Transfer…**（macOS 系统菜单 / Windows 菜单栏）
- 连接树右键 → **Data Transfer…**（数据库 / 表节点；**当前版本不会自动预填 Source**，需手动选择）

首次打开窗口时会弹出 **当前版本能力限制** 说明；可勾选「不再显示此提示」，未勾选则下次打开仍会弹出。

## 何时使用

| 场景 | 推荐工具 |
|------|----------|
| 同方言、结构一致、有相同 PK，需要增量对齐 | Data Sync |
| 仅对比结构差异 | Schema Diff |
| 跨方言（如 MySQL → PostgreSQL）、结构不一致、需整表迁移或建表 | **Data Transfer** |
| SQL ↔ Redis 等跨类别 | 不支持 |

配对路径会在 **Endpoints** 步显示：

- **direct** — 同方言族，直接 SQL 路径
- **ir** — 跨方言，经 IR 适配
- **unsupported** — 不可传输（目标连接会在下拉中禁用）

## 向导步骤（6 步）

1. **Endpoints** — 选择源/目标连接与 catalog（database）。目标连接为 `read_only` 时会显示警告，Execute 会被阻断。
2. **Setup** — 传输模式 + 写入选项（原 Mode / Options 合并）：
   - **传输模式**：Structure only / Data only / Structure + data
   - **Write mode**、batch size、遇错即停
   - 破坏性 write mode 须勾选 **「I understand this may destroy target data」** 才能进入下一步
3. **Objects** — Inspect 后列出源表，勾选要传输的表；显示源表 → 目标表与状态（`MATCHED` / `CREATE_NEW` / `INCOMPATIBLE` 等）。
4. **Mapping** — 表级与列级映射（见下节）。
5. **Preview** — DDL 与写入计划预览（**DDL 可编辑** `ddlOverride`）；底栏 **Execute transfer** 直接执行（无独立 Execute 页）；执行中可 Cancel。
6. **Result** — 每表成功/失败与插入行数汇总。

## 能力限制（弹窗内容摘要）

- 不迁移视图、函数、触发器、存储过程
- 不迁移外键与二级索引（仅 PRIMARY KEY）
- 跨方言可能丢失时区或调整列默认值
- 仅基表（不含视图、物化视图）
- 无表级断点续传；Cancel 仅停止当前 job

## Mapping 步

左栏为已启用表列表，右栏为 **Column Mapping Editor**：

- **目标表名** — 文本输入；失焦后重新 inspect 该表映射。
- **Create new table** — 结构模式或 `CREATE_NEW` 状态下可勾选，在目标库新建表。
- **Auto-match by name** — 按列名自动匹配源/目标列。
- **Clear unmapped** — 清除未映射的目标列选择。
- **列映射表** — 每行：源列 → 目标列（下拉或文本）→ Skip；跨方言新建表时可编辑 **Target type**（`targetNativeType`）。
- 未映射的目标列会以警告提示。

> 注意：UI **尚未提供 schema 级选择**；Objects 步也暂无搜索/批量筛选（见 backlog）。

## Setup 步：Write mode

| 模式 | 说明 |
|------|------|
| Insert (append) | 追加插入，不删目标数据 |
| Truncate + insert | 清空目标表后插入（破坏性） |
| Drop + create + insert | 删表重建后插入（破坏性） |

破坏性模式在 Setup 步须勾选确认才能 Next；Execute 前会弹出二次确认 Modal，列出将受影响的目标表。

其他选项：**Batch size**（默认 500）、**Stop on first error**。

## Preview 步

- 每张需建表的对象展示 **可编辑 DDL**，可复制或 override。
- **Write plans** — 源表 → 目标表、write mode、预估行数。
- **Warnings / blockReason** — 若 `canExecute === false`，Execute 按钮禁用。
- 底栏 **Execute transfer** 开始执行；执行中显示 Cancel。

## 执行能力矩阵

| 场景 | 状态 |
|------|------|
| 同方言族 + `data` / `structure+data` + `insert` + 目标表已存在 | ✅ 批量 INSERT |
| 跨方言（IR）+ 两端均有 sync adapter + `data` / `structure+data` | ✅ IR 字面量批量 INSERT |
| `truncate+insert` 执行（同族） | ✅ TRUNCATE + INSERT |
| `drop+create` 执行（需 `confirmedDestructive` + IR adapter） | ✅ DROP + CREATE (IR) + INSERT |
| 目标新建表 CREATE（`structure` / `structure+data`，IR adapter） | ✅ `table_to_ir` → `build_create_table_ddl` → execute |
| Preview（IR adapter 可用时） | ✅ 真实 CREATE DDL |
| 执行中 Cancel（job id） | ✅ |
| 目标 read_only | ❌ Execute blocked |
| 同连接同库同表自覆盖 | ❌ blocked |
| 跨类别（如 SQL ↔ Redis） | ❌ pairing unsupported |

## 与 Data Sync 的 UI 差异

Data Sync 为**单页工作台**；Data Transfer 为 **6 步向导**（视觉 token 已与 Sync 对齐），映射能力更强。仍缺：端点 swap、schema 下拉、Objects 搜索、执行进度面板、右键预填 Source 等（见 PRD backlog）。

## 常见问题

**Q: Preview 通过后 Execute 仍失败？**  
检查目标是否只读、破坏性模式是否已勾选确认、列映射是否完整（尤其跨方言 `targetNativeType`）。

**Q: 某表状态为 INCOMPATIBLE？**  
在 Mapping 步查看 `incompatibleReason`；通常需调整目标表、改映射或排除该表。

**Q: 与 Sync 选哪个？**  
结构 + PK 完全一致 → Sync；否则 → Transfer 或 Schema Diff。

## 相关代码

- 后端：`src-tauri/src/data_transfer/` + `src-tauri/src/commands/data_transfer/`
- IR / DDL：`src-tauri/src/transfer/`（与 Sync 执行引擎分离）
- 前端：`src/windows/data-transfer/`（`DataTransferWindow.tsx`、`TransferLimitationsDialog.tsx`、`TransferMappingStep.tsx`）
