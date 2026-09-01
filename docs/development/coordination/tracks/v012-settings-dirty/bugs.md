# v0.1.2 Settings dirty bugs

暂无本轨新增 bug。

Host 全量回归中的 4 个 ConnectionNavigatorTree 失败属于前置 Dialog polish 的既有测试适配问题（旧测试依赖 `aria-label` 定位），不归入本轨；Settings 定向测试与 driver UI 测试均通过。
