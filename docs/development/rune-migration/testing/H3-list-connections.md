# TN H3 列出连接接到现有 use case 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`3a441166 feat(rune): list connections through the existing use case (issue-rune H3)`
- 改动面：`crates/datazen-ui-api`（`UseCaseAdapter`：`list_connections` 接 `datazen::connection::application::ConnectionService::list_redacted`；`MemoryConnectionRepository` 内存仓储；`config_from_profile` 映射；新依赖 `datazen`(application)/`datazen-driver-api`/`async-trait`/`futures-executor`）、`crates/datazen-ui/src/lib.rs`（`AppState.desktop` 改为 `Box<dyn DesktopAdapter>`，桌面入口 `init_state` 注入 `UseCaseAdapter::with_demo_connections()`；页面渲染与交互零改动）

## 接线方式

- **现有 use case**：`crates/datazen/src/connection/application/service.rs` 的 `ConnectionService`（该模块文档自述 "Connection application use cases"）。调用 `list_redacted`（列表演示路径，凭据脱敏）。
- **仓储**：适配器自带内存仓储实现 `ConnectionRepository`（8 个方法；只读——写方法答 `BackendUnavailable`；跨组织访问按边界语义答空）。桌面宿主之后经 `from_repository(Arc<dyn ConnectionRepository>)` 换真存储，用例不动。
- **同步桥**：用例是 async，UI 事件循环是同步——用 `futures_executor::block_on` 阻塞等待（内存仓储的 future 首次 poll 即就绪，不会真的挂起）。
- **UI 不动**：适配器答列表时替换端口演示列表；失败（H4/H5 未接的方法）仍走 H2 的兜底显示。`AppState::default` 仍是 `UnimplementedAdapter`，全部既有运行时测试不受影响。

## 测试（测在适配器里）

| 测试 | 位置 | 断言 |
| --- | --- | --- |
| `the_wired_adapter_lists_the_demo_connections_through_the_use_case` | datazen-ui-api | 用例的脱敏列表映射回 UI 词汇，与端口演示连接一致（local/replica） |
| `a_wired_adapter_over_an_empty_store_lists_no_connections` | datazen-ui-api | 空仓储列出空列表，不是错误 |
| `the_wired_adapter_keeps_the_unwired_methods_at_the_fixed_error` | datazen-ui-api | H4/H5 未接的方法仍答固定错误 |
| `a_wired_adapter_replaces_the_demo_list_with_the_use_case_answer` | datazen-ui（runtime） | 注入带 warehouse 连接的适配器后，连接页显示 use case 的列表（`connection-warehouse` 出现、`connection-local` 消失、无兜底消息） |

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（datazen-ui 103 passed，datazen-ui-api 50 passed；新增 4 个测试） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 列出连接已接到现有 use case，桌面进程经适配器取数，页面代码未动。遗留：H4 接读一页表（失败时网格保持上一页）、H5 接提交脏格（失败时脏标记还在）；真机存储仓储由桌面宿主后续注入。
