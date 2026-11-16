# A2 连接表单对话框化验证报告

## 范围

- 分支：`feature/a2-conn-dialog`（基于 `feat/rune-native-ui-migration`）
- 任务：A2 连接表单对话框化（分组标准字段 + SSH 隧道族，对齐 NewConnectionDialog；plan-native-5）。
- 被测提交：`1e383c421`（feat）。
- 改动面：`crates/datazen-ui/src/state_def.rs`、`crates/datazen-ui/src/connections_io_state.rs`、`crates/datazen-ui/src/views/connections.rs`、`crates/datazen-ui/src/tests/structure_er.rs`。不改 `packages/frontend`、`packages/web`、内核公开契约与 `SharedSettings`。

## 结论

通过。连接表单由 Connections 页内联常驻改为对话框：页头提供 `connection-new`（“New connection”）入口打开 `connection-dialog`，关闭或保存成功后不再内联展示。对话框按组渲染 Basic（名称、引擎）与 Connection（地址、数据库、用户名、密码）标准字段，并包含 SSH 隧道族 UI 区块（启用切换与主机/端口/用户名输入）用于与 React `NewConnectionDialog`/`SshTunnelFields` 的结构对齐。保存继续走既有 `save_connection_form` → `ConnectionDraft`/`DesktopAdapter::save_connection`（失败回退 `MemoryWorkspace::save_connection`）路径；必填/校验行为保持与实现一致。

## 实现面

- **对话框状态**：`AppState::connection_dialog_open` 控制显隐；`open_connection_dialog`/`close_connection_dialog` 管理打开、关闭与表单/错误清理。
- **分组渲染**：`connection-section-basic`、`connection-section-connection`、`connection-section-ssh` 及对应标题/字段键；保留稳定键 `connection-form`、`connection-form-fields`、各输入键、`connection-save`、`connection-form-error`。
- **SSH 隧道（UI 对齐，非持久化）**：`connection_ssh_enabled`、`connection_ssh_host`、`connection_ssh_port`、`connection_ssh_username` 仅为对话框 UI 状态；区块明确标注 “UI only, not saved”。`ConnectionDraft` 无隧道字段，保存不读取/不写入隧道配置，不引入契约或持久化变更。
- **兼容**：Connections 页 hero/卡片/最近查询/导入区块保留；连接卡片 `connection-{id}` 与打开行为不变；既有桌面适配器消息与导入流程不变。

## 增量：新建连接窗口布局 React 对齐（本次提交）

- **布局**：`views/modals.rs` 将连接表单对话框重构为 React `NewConnectionDialog` 风格的左右分栏——左侧数据库类型选择栏（基于 `available_engines()` 列表，点击写入 `connection_engine`）、右侧为标题栏 + 可滚动表单内容 + 底部操作区（测试连接占位、取消、保存由表单操作区承载）。
- **表单**：`views/connections.rs` 将 `connection-form-fields` 重排为两列网格与分区：基础（名称、数据库类型）、连接字段（主机/地址、数据库、用户名、密码）、高级设置区块头、隧道/SSH 区块（启用切换与 SSH 主机/端口/用户名），并保留错误行与 `connection-form-actions`。
- **键与行为**：保留 `connection-dialog*`、`connection-form*`、`connection-section-*`、各输入/切换键与 `save_connection_form`/关闭逻辑；`dialog-cancel` 可由对话框底部触发关闭。
- **门禁**：`cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` → **183 + 56 passed**；`cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline` → exit 0。

## 测试

| 测试 | 位置 | 覆盖 |
|---|---|---|
| `a_connection_form_save_needs_an_address`（适配） | `crates/datazen-ui/src/tests/structure_er.rs` | 打开对话框、空地址保存被拒并显示错误、填写有效字段后保存成功并关闭对话框 |
| `connection_dialog_renders_grouped_sections_and_ssh_ui`（新增） | `crates/datazen-ui/src/tests/structure_er.rs` | New connection 入口、对话框与 Basic/Connection/SSH 分组及关键字段/切换渲染、关闭后不内联 |
| 既有连接相关测试 | `crates/datazen-ui/src/tests/structure_er.rs`、`io_tools.rs`、`workspace_tabs.rs`、`host_jobs.rs` | 表单保存白名单/宿主路径、导入、连接页消息与导航等回归（`--lib connection` 11 passed） |

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR2 | `CARGO_TARGET_DIR="$PWD/target" cargo test -p datazen-ui --offline --lib -- --test-threads=1` | **163 passed / 0 failed** |
| GR4 | `CARGO_TARGET_DIR="$PWD/target" cargo check -p datazen-rune-host --offline` | 通过，exit 0（既有 warnings） |
| GR1 | 沿用（未改 ui-api） | — |
| GR3 | 沿用（未改宿主测试面） | — |

## 契约与限制说明

- **SSH 隧道**：仅提供 UI 结构/控件对齐，不持久化、不进入 `ConnectionDraft`，保存与连接契约不变（符合 plan-native-5 A2 约束）。
- **表单形态**：对话框为页内面板式呈现（Rune 视图树），未引入独立窗口；键位与保存语义对齐既有测试与端口。
