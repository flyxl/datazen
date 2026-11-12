## Current Task
- **任务**: 阶段 1，UI 端口和第一条数据路径
- **负责人**: Coder Agent
- **状态**: `datazen-ui-api` 和内存工作区已接上 DataGrid。未跑 Tester。
- **计划**: [plan.md](plan.md)

## Completed Work
- [x] SQL Editor POC
- [x] DataGrid POC（虚拟行、列窗口、编辑、TSV）
- [x] 全量迁移计划 `docs/development/rune-migration/plan.md`
- [x] `crates/datazen-ui-api`：连接、打开表、暂存单元格
- [ ] 阶段 2 工作区壳（对象树和标签页）
- [ ] 阶段 3 接到真实 table browsing / editing

## 阶段 1
连接列表来自 `WorkspaceApi`。点 Local Postgres 后，网格装入 `customers` 这一页，不再显示占位的 `r0c0`。提交编辑会写回端口里的那一页。桌面和 HTTP 适配器还没换进去。

## Verification
`cargo test -p datazen-ui-api --lib`：1 passed。`cargo test -p datazen-ui --lib`：17 passed。
