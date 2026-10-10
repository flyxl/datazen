# progress.md — 轨 A-11：`close_handle` 幂等关闭缺口

缺陷编号：A-11（独立验收发现）

## 缺陷

`packages/driver-api/src/resource.rs:635-639` 的 trait 文档承诺：

> a handle the provider does not currently hold open is
> [`ResourceError::InvalidResourceState`], not `Ok`. The one legitimate
> repeat is a handle the resource handle already marks closed
> ([`ResourceHandle::is_closed`]), which is reported as success because
> the close already happened and re-doing it is not information.

即 `close_handle` 收到一个**资源句柄本身已标记 closed** 的引用时，应返回 `Ok(())`。

实测：9 个 `impl ResourceProvider` 里只有 postgres 实现了这个短路
（`packages/drivers/postgres/src/resource/provider/contract.rs:403-405`）。
postgres 把它排在 `belongs_to` 检查**之后**："closed + 外国引用" 先被
`InvalidResourceState` 拒绝。宿主 `LeaseFacade::release`
（`packages/runtime/src/resource/lease_manager.rs:431-455`）对 `Err` 是 fail-closed
（保持登记不释放），所以没有数据安全隐患；但这是**已发布 trait 文档与实现不一致**。

## 选择：补实现，不改文档

理由见最终报告（父代理汇总）。

## 落点

按 `AGENTS.md` 驱动测试落点表：Rust 单元测试写同文件 `#[cfg(test)]` 或
`<crate>/tests/`。

## 变更清单

（进行中，随改动追加）

## 门禁结论

（进行中，逐条追加命令 / 退出码 / 结论行）

## 待裁定项

（无则填无）
