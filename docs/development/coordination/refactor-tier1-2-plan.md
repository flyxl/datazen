# Tier 1 & Tier 2 核心源码模块化重构 — 实施计划

> **集成分支**：`feat/refactor-tier1-2`（基于 `main`）  
> **协调总览**：`docs/development/coordination/hub.md`  
> **Playbook**：`docs/development/subagent-dev-playbook.md`  
> **状态**：Wave 1 三轨并行启动

## 0. 目标与分轨原则

本计划聚焦解决代码库中历史遗留的 3 个超大单体“上帝文件”（1,300 ~ 2,900 行），将其拆解为高内聚、易阅读、单一职责的清晰子模块，外部 API 与公共 Trait 保持 100% 兼容。

三轨位于互不冲突的独立 Crate 与目录，完全无文件重叠，安排在 **Wave 1 并行推进**：

| Track ID | 目标 Crate / 路径 | 核心目标 | 当前行数 | 预期拆分结构 |
|---|---|---|---|---|
| **refactor-pg** | `packages/drivers/postgres/` | 拆解 `postgres.rs` 为驱动薄入口、连接池、执行取消、类型解码、元数据 DDL、单测 | 2,867 行 | 6~7 个 200~500 行子模块 |
| **refactor-mysql** | `packages/drivers/mysql/` | 拆解 `mysql.rs` 为驱动薄入口、连接池、执行取消、文本类型解码、例程 DDL、单测 | 2,756 行 | 6~7 个 200~500 行子模块 |
| **refactor-sqldump** | `packages/driver-api/` | 拆解 `sql_dump.rs` 为 `sql_dump/` 目录（生成、还原会话、解析器、单测） | 1,359 行 | 4 个 200~450 行子模块 |

## 1. 约束与纪律

1. **Facade 兼容**：所有模块对外公共导出（函数名、结构体、Trait 实现签名）一律保持原有路径与可见性不变。
2. **驱动测试守则**：驱动所有测试必须保留在驱动 crate 内（`cargo test -p datazen-driver-<id> --lib`），严禁移动到 Host。
3. **独立测试代理**：每轨编码完成后，由全新的独立测试代理复验，只测不修。
