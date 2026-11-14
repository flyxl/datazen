# TN N5 宿主执行 runtime 组装 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`65f4b49 feat(rune): the host's execution runtime, assembled from three honest parts (issue-rune N5)`
- 改动面：新 `crates/datazen-rune-host/src/backend.rs`（HostProfiles/HostPolicy/HostSessionBackend/assemble_runtime + 测试）、`crates/datazen-rune-host/src/live.rs`（LiveAdapter 持有 runtime 并暴露 `runtime()`/`with_data_dir`、`host_registry` 改 pub(crate)、测试共享助手）、宿主 crate 清单（uuid）。**UI 侧零改动**。

## 实现要点

- **三部件**（`RuntimeConnectionUseCases::new` 的全部入参，其余管道由用例自建）：
  - **HostProfiles**：文件连接存储投影成 ProfileRepository。7 个方法全实现：list/get 从存储读配置构造 `ProfileRecord`；create/compare_and_set 经 `ConnectionService::save` 落盘并推进内存 revision 计数（会话不跨进程存活，重启归一无害）；disable 在无 enabled 标志的存储上=诚实删除；advance_credential_revision 只推计数。CAS 冲突回 `PortError::CasConflict`。
  - **HostPolicy**：单租户允许策略——desktop 组织放行、越界 NotFound；委托校验 TokenInvalid（本地无委托链）；隔离键常量；版本流为空迭代器（本地策略进程内不变）。
  - **HostSessionBackend**：驱动的资源契约，单会话形态。open = describe→acquire（无界 BudgetPort——单用户无账可守）→observe→登记；execute = 平台事务命令（begin/commit/rollback）直通 provider，其余 `execute_on_resource`，输出经 SinkAdapter 发布进 runtime 的结果 sink（`read_artifact_chunk` 因此能读回驱动产物），上下文版本随观察推进；cancel 直通 provider 回执；finalize/close 沿用桌面的句柄绑定校验与 `registered_handles` 门禁。无池、无预算台账、无隔离区——单用户宿主的诚实裁剪。
- **装配**（`assemble_runtime`）：三部件 + `RuntimeConnectionUseCases::new` + sink 回装后端（未装时执行拒绝而非发布进虚空）+ `FileArtifactRepository` 落在数据目录 `result-artifacts/`（打开失败留给 sink 自己的临时回退）。LiveAdapter 持有 runtime 并暴露给 N6/N7 的作业流。
- **夹具发现（写进测试注释的事实）**：sqlite 资源**懒连接**——坏文件上会话照样打开，垃圾内容在第一次目录读取时才浮现（`SELECT 1` 不读文件头，答得出来）；`RequireNoTransaction` 关闭在事务不可观测的驱动上 fail-closed，宿主用 `RollbackAndClose`。

## 测试（全在 datazen-rune-host）

| 测试 | 位置 | 断言 |
|---|---|---|
| `the_runtime_opens_executes_reads_and_closes_over_a_real_session` | backend.rs | 基石全链路：token→开真会话（配置来自文件存储、资源走静态 registry 的 provider）→会话内执行真查询→轮询到终态 Succeeded→经 `read_artifact_chunk` 读回驱动的三行真数据（user-0..2 在产物里）→`RollbackAndClose` 关会话 |
| `a_session_over_a_bad_database_file_fails_with_a_sentence` | backend.rs | 坏文件：会话懒开成功、第一条目录读取的执行终态 Failed（钉死懒连接边界）；关闭走 RollbackAndClose |

既有 10 个测试（N2–N4）零回归；GR1 120+55 零回归（UI 未改）。

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（120 + 55 passed，本任务无 UI 改动） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| GR3 | `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1` | 通过，exit 0（12 passed；新增 2 个测试） |
| GR4 | `cargo check -p datazen-rune-host --offline --all-targets` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 提交（N6）与查询（N7）的执行地基就位，全链路经真 sqlite 验证。遗留：N6 在此之上接 `table_editing::commit_changes`（需要 ConnectionFacts/TableColumnSource/ArtifactChunkReader 三件套，其中 ArtifactChunkReader 可由 runtime 固有 `read_artifact_chunk` 投影）；N7 的查询作业直接用 `execute_in_session`。
