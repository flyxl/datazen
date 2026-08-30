# v0.1.x Desktop 实施轨道计划

详细设计以 [desktop-v0.1x-implementation.zh-CN.md](../desktop-v0.1x-implementation.zh-CN.md) 为准。本文件只记录本轮并行编排。

## 轨道划分

### v01x-query-cancel

实现查询执行状态和取消能力感知：

- 复用 `DatabaseDriverFactory::supports_cancel_query()`，由 `DriverRegistry` 暴露 capability。
- `get_connection_info` 返回 capability，前端连接状态保存 capability。
- 新增 `QueryExecutionViewModel`，区分 `running`、`cancel_requested`、`cancelled`、`failed` 和 capability unknown/unsupported。
- 取消 command 失败不能把 query 标记成 Cancelled；不支持取消的 driver 不调用 cancel stub。
- 只修改本轨明确文件；`QueryPanel` UI 集成由协调者在编码和独立测试通过后接入。

## 固定流程

```text
编码子代理 → commit → 全新测试子代理 → 通过/登记 bug
                                      ↓
                             协调者审核并合并
```

每个状态流转必须更新 hub；完整 E2E 留到 R 阶段。
