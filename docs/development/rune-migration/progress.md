## Current Task
- **任务**: S1 标签模型
- **负责人**: Coder Agent
- **状态**: 未开始。计划已按一天一件重写。
- **计划**: [plan.md](plan.md)

## 约束
React 页面和组件保留。不删除、不改成空壳、不把现有入口摘掉。Rune 是并行的另一套界面。

## Completed Work
- [x] R0 Rune crate 能编译
- [x] R1 SQL Editor POC
- [x] R2 DataGrid POC
- [x] R3 UI 端口和内存表页
- [ ] S1 标签模型

## Verification
`cargo test -p datazen-ui-api --lib`：1 passed。`cargo test -p datazen-ui --lib`：17 passed。
