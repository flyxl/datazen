# Data Synchronization 使用手册

> 行级增量同步（对标 Navicat Data Synchronization）。  
> 架构说明见 [data-sync.md](../architecture/backend/data-sync.md)；产品定义见 [data-synchronization-prd.zh-CN.md。

---

## 1. 入口

- **Tools → Data Sync…**（macOS 菜单）
- 连接 / 库 / 表右键 → **Data Sync…**（预填 Source）
- 开发 / E2E：`tauri://localhost/window.html?window=data-sync`

旧版 **DROP + INSERT 覆盖拷贝** 已退役；窗口顶部有说明横幅。

---

## 2. 工作流：6 步向导

| 步骤 | 界面 | 说明 |
|------|------|------|
| **Endpoints** | 源/目标连接 + **database** 下拉 | 选择端点、数据库和 schema；同族配对路径会显示在本步 |
| **Setup** | **Options** | 配置 Insert / Update / Delete；启用 Delete 前需确认 |
| **Objects** | 表映射列表 | 先做表映射门闸（`inspect_data_sync`），可取消勾选整表 |
| **Compare** | 摘要、表筛选、**Row diff** | 对 MATCHED 表执行行比较（`compare_data_sync`），按表 / 操作类型审阅差异 |
| **Preview** | **SQL preview** | 只读预览参数化 DML（`generate_data_sync_sql`）；可复制，**V1 不支持改文本再执行** |
| **Result** | 执行结果 | 执行完成后显示成功状态与再次比较入口；目标 `read_only` 时 Execute 禁用 |

```text
Endpoints → Setup Options → Objects 映射
    → Compare 行差异 + 勾选范围
    → Preview SQL → Execute
    → Result → 期望再次 Compare 行差异为 0
```

比较或执行进行中可用 **Cancel**（`cancel_data_sync`）。

---

## 3. 硬门闸（不满足则 INCOMPATIBLE，不进 Compare）

1. **同方言族**：V1 仅 MySQL（含 MariaDB）↔ MySQL、PostgreSQL ↔ PostgreSQL。异构对（如 PG→MySQL）请用 **[Data Transfer](./data-transfer-guide.zh-CN.md)**。
2. **结构完全一致**：列名、类型等价（`types_eq`）、可空性一致。
3. **主键一致**：双方 PRIMARY KEY 列集与顺序相同；无 PK → INCOMPATIBLE。
4. **目标表须已存在**；仅基表（非视图）。
5. **禁止自同步**：同一连接 + 同一 database（+ 同一 schema）不能既作源又作目标。
6. **按 database 同步**：选连接后还须各选源/目标 **database**（`get_databases` 枚举，默认取连接配置里的库名）。

结构不一致时：先用 **[Schema Diff](./schema-diff-guide.zh-CN.md)** 对齐 DDL，或改用 Transfer。

---

## 4. Options 与 Delete 默认值

| 选项 | 默认 | 说明 |
|------|------|------|
| **Insert** | ✅ 开启 | 源有、目标无的行 → INSERT |
| **Update** | ✅ 开启 | 同 PK、非键列不同 → UPDATE |
| **Delete** | ❌ 关闭 | 目标有、源无的行 → DELETE；勾选前会弹确认，执行前再次提示 |

ChangeSet 只包含**已勾选**且 options 允许的变更。DELETE 默认不进 ChangeSet。

---

## 5. 与 Schema Diff / Data Transfer 的分工

| 产品 | 做什么 | 何时用 |
|------|--------|--------|
| **[Schema Diff](./schema-diff-guide.zh-CN.md)** | 结构对比 + 受控 DDL 部署 | 列/索引/PK 不一致，要先改结构 |
| **Data Sync（本文）** | 结构一致 + 相同 PK → 行 Diff → 审查 → DML | 两库表结构已对齐，补行级差异 |
| **[Data Transfer](./data-transfer-guide.zh-CN.md)** | 异构 / 结构不同 / 无 PK / 可建表 → 单向搬运 | 迁库、异构导入、空库灌数 |

```text
结构不同、无 PK、目标表不存在、异构库  →  Schema Diff 和/或 Data Transfer
结构一致 + 相同 PK + 同族库            →  Data Sync
```

窗口内对 INCOMPATIBLE 表可跳转 **Schema Diff**；异构对在目标下拉中会标为 unsupported 并提示 Transfer。

---

## 6. 执行与安全

- DML 走专用通道 **`execute_data_sync`**，不经 SQL 编辑器的 Safe Mode / `execute_query`。
- 目标连接标记 **read_only** 时 Execute 禁用。
- 支持 job 取消；数据库支持时在事务中执行（失败 rollback）。
- **MCP V1 不暴露** Sync 的 apply/execute 工具（高风险写操作）。

---

## 7. 排错

| 现象 | 可能原因 |
|------|----------|
| Next 灰掉 | 未选源或目标连接 / database，或配对不受支持 |
| 提示 useTransferHint | 异构或非 Sync 支持的方言对 |
| 表 INCOMPATIBLE | 结构或 PK 不一致 → Schema Diff 或 Transfer |
| Execute 不可用 | 无勾选行差异、目标 read_only、或仍在 comparing |
| Preview 为空 | 无选中变更，或 Options 过滤掉了所有操作 |
| 大表 Compare 慢 | V1 对 MATCHED 表使用全表 `SELECT` 后在 Host 内存合并；超大表请缩小范围或分批 |

---

## 8. 相关文档

- 架构：[docs/architecture/backend/data-sync.md](../architecture/backend/data-sync.md)
- E2E 覆盖：[docs/e2e-coverage.md](../development/e2e-coverage.md)
- Transfer：[docs/data-transfer-guide.md](./data-transfer-guide.zh-CN.md)
- Schema Diff：[docs/schema-diff-guide.md](./schema-diff-guide.zh-CN.md)
