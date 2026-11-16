## Current Task
- **任务**: A1 收口与全局视觉/交互对齐验证（plan-native-5）— A2–A6 已合流主轨，待统一门禁、契约内收尾与逐页视觉一致性校验
- **负责人**: Coder Agent
- **状态**: L1–L7 已完成；A1 进行中（首页与导航器 Recent/All/过滤已落地，pinned/用户分组受契约限制）。A2 连接表单对话框化、A3 查询面板对齐、A4 表浏览对齐、A5 结果工作区对齐、A6 设置导航分组已合流主轨。第五阶段计划见 [plan-native-5.md](plan-native-5.md)。

## Completed Work (recent)
- [x] A1 rail/弹窗对齐增量（`0aa7fa445`）：侧栏按 React 顺序重排并将 Settings/折叠按钮置于底部组；图标改为 lucide 语义；连接表单与导入改为 root modal layer（scrim + Escape，真浮层）。（见 [testing/A1-connections-home.md](testing/A1-connections-home.md)）
- [x] A2 连接表单对话框化（`1e383c421`, `7e06bf48e`，合流主轨；见 [testing/A2-connection-form-dialog.md](testing/A2-connection-form-dialog.md)）
- [x] A3 查询面板对齐（`3630c95f7`, `020a62b22`，合流主轨；见 [testing/A3-query-panel.md](testing/A3-query-panel.md)）
- [x] A4 表浏览对齐（`e041fc463`, `5ae657365`, `f8879f36c`，合流主轨；见 [testing/A4-table-browse.md](testing/A4-table-browse.md)）
- [x] A5 结果工作区对齐（`b7b25b518`, `ef38e1069`，合流主轨；见 [testing/A5-result-workspace.md](testing/A5-result-workspace.md)）
- [x] A6 设置导航分组（`27c5f2292`, `95b4a4c30`，合流主轨；见 [testing/A6-settings-nav.md](testing/A6-settings-nav.md)）

## N5 组装配方（调查结论，fc160ec 时点）

`RuntimeConnectionUseCases::new(profiles, policy, backend)` 内部自建全部管道（SessionRegistry over ObservedBackend、ExecutionGateway + BoundAuthorizer + 内存幂等、会话目录、令牌环、结果 sink）——宿主只需三个部件：

1. **profiles: `Arc<dyn ProfileRepository>`**（`connection::domain::profile`，7 个方法：list/get/create/compare_and_set/disable/advance_credential_revision）——由文件连接存储投影；`ProfileRecord` 带版本语义，CAS 冲突回 `PortError::CasConflict`。
2. **policy: `Arc<dyn PolicyService>`**（`execution::application::policy`：authorize/verify_delegation/authorize_target/isolation_key/subscribe_version_changes）——单租户宿主的允许策略 + 版本流。
3. **backend: `Arc<dyn SessionBackend>`**（`execution::runtime::registry::backend`：open/open_for_command/execute/cancel/finalize_handles/close/observability_snapshot）——走驱动的 ResourceProvider 契约（sqlite 已有 `SqliteResourceProvider`，经 factory 的 `resource_provider()` 取），资源语义参照桌面 `platform/session_backend{,.rs,/*}`（1480 行，含池化/隔离/预算——单会话宿主可裁）与 server `execution/driver_exec.rs`（500 行，含流式/守卫）。

提交路径（N6）额外需要：`ConnectionFacts`、`TableColumnSource`（schema 目录命令投影）、`ArtifactChunkReader`（runtime 固有 `read_artifact_chunk`，结果经 sink + `FileArtifactRepository` 落盘——桌面在 `platform/adapter.rs:480` 一带的装配是参照）。查询（N7）同理走 runtime 会话。

关键事实：桌面测试注释确认"Test drivers are registered via register_test_driver, not inventory catalog"——静态 registry（N3 的 `with_factories` 形态）在宿主测试与真机行为一致，这个决定已被 N3 验证。

## 约束
React 页面和组件保留。不删除、不改成空壳、不把现有入口摘掉。Rune 是并行的另一套界面。

## Completed Work
- [x] R0 Rune crate 能编译
- [x] R1 SQL Editor POC
- [x] R2 DataGrid POC
- [x] R3 UI 端口和内存表页
- [x] S1–S6、B1–B8、Q、O、P、D、W、H、C1–C2 全部完成（提交与测试报告见 [tasks.md](tasks.md)）
- [x] C3 收尾测量（数字见下节）
- [x] C4 未迁移区域提示（WAPPS/Onboarding 告知 React 页面还在，见 [testing/C4-unmigrated-notice.md](testing/C4-unmigrated-notice.md)）
- [x] A4 表浏览对齐：行号 gutter、按表列宽持久化与回填、FK 面包屑、状态横幅、待提交条（`e041fc463`, `5ae657365`；测试报告 `testing/A4-table-browse.md`）

## Verification
- 主轨集成（A2–A6 合流，含冲突解决）：`CARGO_TARGET_DIR="$PWD/target" cargo test -p datazen-ui --offline --lib -- --test-threads=1` → **176 passed, 0 failed**；`CARGO_TARGET_DIR="$PWD/target" cargo check -p datazen-rune-host --offline` → exit 0（既有 warnings）。
- A1 rail/弹窗对齐增量（`0aa7fa445`）：`CARGO_TARGET_DIR="$PWD/target" cargo test -p datazen-ui --offline --lib -- --test-threads=1` → **183 passed, 0 failed**；`cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline` → exit 0（既有 warnings）。
- 分支/轨道门禁（合流前）：A2 163 passed；A4 164 passed（含回归修复）；A3 169 passed（含 part_10）；A5 171 passed（含 data_grid find 命中测试）；A6 165 passed。
- 历史基线（C3 收尾）：`cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1`：datazen-ui 113 passed、datazen-ui-api 54 passed。

## 收尾测量（C3）

2 万行和 200 列在 Rune 网格上的实测数字，量自产品面（表标签页的表格视图），量法是 C2 建立的无障碍树读法——树里一行节点就是一行挂载，一格节点就是一格建成：

- **2 万行的页挂载 21 行**：首帧静止（视口 1440×900、sheet 高 380pt、行高 28pt）时，挂载窗口为列表头部的 21 行，与列表长度无关。数字由 `twenty_thousand_rows_mount_a_window_and_two_hundred_columns_build_one_window_of_cells` 断言钉死（报告见 [testing/C3-mount-counts.md](testing/C3-mount-counts.md)）。
- **200 列的页每行建 12 格**：列窗口 COL_WINDOW=12，静止时冻结首列落在窗口内共 12 列，表头同样建 12 个；横向滚动后冻结列在窗口外另建一格，每行 13 格（既有单元测试的上界 ≤ COL_WINDOW+1）。
- 量法不改产品代码：规模页经既有 `load_page` 装入——2 万条空记录，格值走网格的 `r{r}c{c}` 回退，页只占约 480 KB 而非 400 万格。
