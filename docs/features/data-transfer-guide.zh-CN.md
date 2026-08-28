# Data Transfer 用户指南

> 产品定义见 [data-transfer-prd.zh-CN.md](./data-transfer-prd.zh-CN.md)。  
> 与 **Data Synchronization**（同族 + 结构一致 + 相同 PK 的行级 Diff）和 **Schema Diff**（结构对齐）是独立产品。

## 入口

- **Tools → Data Transfer…**（macOS 系统菜单 / Windows 菜单栏）
- 连接树右键 → **Data Transfer…**（数据库 / 表节点；**当前版本不会自动预填 Source**，需手动选择）

## 何时使用

| 场景 | 推荐工具 |
|------|----------|
| 同方言、结构一致、有相同 PK，需要增量对齐 | Data Sync |
| 仅对比结构差异 | Schema Diff |
| 跨方言（如 MySQL → PostgreSQL）、结构不一致、需整表迁移或建表 | **Data Transfer** |
| SQL ↔ Redis 等跨类别 | 不支持 |

配对路径会在 **Source / Target** 步显示：

- **direct** — 同方言族，直接 SQL 路径
- **ir** — 跨方言，经 IR 适配
- **unsupported** — 不可传输（目标连接会在下拉中禁用）

## 向导步骤（8 步）

1. **Source / Target** — 选择源/目标连接与 catalog（database）。目标连接为 `read_only` 时会显示警告，后续 Execute 会被阻断。
2. **Mode** — 传输范围：
   - **Structure only** — 仅建表/结构
   - **Data only** — 仅数据（目标表须已存在或可映射）
   - **Structure + data** — 结构 + 数据
3. **Objects** — Inspect 后列出源表，勾选要传输的表；显示源表 → 目标表与状态（`MATCHED` / `CREATE_NEW` / `INCOMPATIBLE` 等）。
4. **Mapping** — 表级与列级映射（见下节）。
5. **Options** — 写入模式、batch size、遇错即停。
6. **Preview** — DDL 与写入计划预览；**DDL 可在编辑器中修改**（`ddlOverride`）。
7. **Execute** — 确认后执行；执行中可 Cancel（job id）。
8. **Result** — 每表成功/失败与插入行数汇总。

## Mapping 步（当前 UI 能力）

左栏为已启用表列表，右栏为 **Column Mapping Editor**：

- **目标表名** — 文本输入；失焦后重新 inspect 该表映射。
- **Create new table** — 结构模式或 `CREATE_NEW` 状态下可勾选，在目标库新建表。
- **Auto-match by name** — 按列名自动匹配源/目标列。
- **Clear unmapped** — 清除未映射的目标列选择。
- **列映射表** — 每行：源列 → 目标列（下拉或文本）→ Skip；跨方言新建表时可编辑 **Target type**（`targetNativeType`）。
- 未映射的目标列会以警告提示。

> 注意：UI **尚未提供 schema 级选择**（PG/MySQL 多 schema 场景目前依赖连接默认 schema）；Objects 步也暂无搜索/批量筛选。

## Options 步

**Write mode：**

| 模式 | 说明 |
|------|------|
| Insert (append) | 追加插入，不删目标数据 |
| Truncate + insert | 清空目标表后插入（破坏性） |
| Drop + create + insert | 删表重建后插入（破坏性） |

破坏性模式需勾选 **「I understand this may destroy target data」** 才能进入 Preview。

其他选项：

- **Batch size** — 默认 500
- **Stop on first error** — 首错即停

## Preview 步

- 每张需建表的对象展示 **可编辑 DDL**（SqlEditor），可复制或 override。
- **Write plans** — 源表 → 目标表、write mode、预估行数。
- **Warnings / blockReason** — 若 `canExecute === false`，Next 至 Execute 会被禁用。

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

Data Sync 采用**单页工作台**（顶栏端点 + 表列表 + 详情/预览 + 执行进度面板）。  
Data Transfer 仍为**线性 8 步向导**，专业映射能力更强，但：

- 无 Source ↔ Target 一键交换
- 无 schema 下拉、表搜索/筛选
- 无执行进度条（仅 Execute 步 spinner）
- 右键入口暂未预填源连接

上述体验改进已列入产品 backlog，不影响当前核心传输能力。

## 常见问题

**Q: Preview 通过后 Execute 仍失败？**  
检查目标是否只读、破坏性模式是否已勾选确认、列映射是否完整（尤其跨方言 `targetNativeType`）。

**Q: 某表状态为 INCOMPATIBLE？**  
在 Mapping 步查看 `incompatibleReason`（后端返回）；通常需调整目标表、改映射或排除该表。

**Q: 与 Sync 选哪个？**  
结构 + PK 完全一致 → Sync；否则 → Transfer 或 Schema Diff。

## 相关代码

- 后端：`src-tauri/src/data_transfer/` + `src-tauri/src/commands/data_transfer/`
- IR / DDL：`src-tauri/src/transfer/`（与 Sync 执行引擎分离）
- 前端：`src/windows/data-transfer/`（`DataTransferWindow.tsx`、`TransferMappingStep.tsx`、`ColumnMappingEditor.tsx`）
