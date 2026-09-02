# DataZen 架构设计（九）：大数据量结果集的前端性能设计

> 数据库工具的性能瓶颈常常不在 SQL，而在“把结果搬进 UI”。DataZen 将限制、流式传输、虚拟滚动和延迟展示分层处理，让查询可以尽快显示首批数据，又不让 WebView 被完整结果集拖垮。

## 先定义结果边界

设置中的 `limit_select_results` 和 `query_result_limit` 决定是否对 SELECT 结果设置上限。结果上限是产品语义，不能用网络批次大小推断；批次只是传输粒度。

后端在 `query_stream` Runtime 中读取设置，在输入没有显式 limit 时才注入限制。导出等需要完整结果的路径可以关闭 `applyResultLimit`，但必须由调用方明确传入。

## 流式传输首批数据

一次性返回 `MultiQueryResult` 适合小结果或 EXPLAIN。大查询使用 Tauri Channel，事件按 `executionStarted`、`statementStart`、`rows`、`statementEnd`、`done` 传递。

后端按批次从 Driver 读取行，前端收到一批就可以渲染一批。用户不必等待最后一行到达才能判断查询是否成功，内存峰值也不会等于完整结果集大小。

## DataTable 虚拟滚动

虚拟滚动只为可视区域创建行和单元格 DOM。滚动窗口移动时复用渲染槽位，行数据仍保存在受控的批次结构中。表格不能把每一行都变成 React 组件，否则即使数据库只返回几万行，布局和事件处理也会先耗尽浏览器资源。

单元格渲染还要考虑 JSON、二进制、长文本和 NULL。复杂值默认摘要展示，详情在用户点击后延迟打开；DataTable 的 `--dt-*` 颜色 token 与 StructureView 等组件共用，避免为性能优化牺牲一致性。

## 分页、Offset 与 Keyset

表浏览器的分页能力由 Driver 元数据决定。支持 offset 的数据库可以使用页码和 offset；不支持或大 offset 成本高的数据库则应使用 keyset 或 Driver 自己的分页策略。

前端只传 `page`、`pageSize`、filters 和 sorts，不拼接数据库方言。Driver 根据 `supports_offset()` 和类型规则生成安全查询，并返回总数是否可用。

## 筛选排序与编辑

筛选、排序可能在服务端执行，也可能对当前已加载批次做本地交互。两者必须在 UI 上区分：服务端筛选改变查询结果集，本地筛选只改变当前窗口。行编辑则先记录 pending changes，提交时通过参数化命令执行，不把用户输入直接拼进 SQL。

## 背压与取消

当渲染速度低于数据库读取速度时，前端需要限制待处理批次，必要时请求暂停或取消。`executionId` 与 `cancel_query` 提供了取消边界；Rust 会验证所有者，Driver 再负责真正停止查询。

取消、失败和窗口关闭都必须释放事件订阅、清理执行状态并让表格回到可重试状态。否则下一次查询可能继承旧的 loading 或结果批次。

## Schema 缓存的配合

DataTable 渲染列需要类型和可空性。SchemaCache 提供共享元数据，避免每次分页都查询系统表。缓存只优化元数据路径，不能把已变化的表结构永久当成真相；写入或结构变更后应失效或刷新相关缓存。

## 指标与测试

值得观测的指标包括首个 `rows` 到达时间、批次大小、总行数、渲染帧率、内存峰值和取消延迟。测试要覆盖空结果、超长字段、多语句、截断标记、分页边界和失败后重试。

## 结语

大结果性能不是某一个组件的技巧，而是数据库上限、Driver 分页、Channel 流式传输、Store 批次管理和虚拟表格共同决定的系统属性。下一篇我们把同一套 Command 能力组合成可复用的 YAML Workflow。

相关资料：[Schema 缓存](../architecture/backend/cache.md) · [DataTable](../../src/components/DataTable/) · [查询流类型](../../src/types/index.ts)
