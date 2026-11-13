# TN S6 侧栏三入口 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`3b78739` `feat(rune): trim the sidebar to connections, workspace and settings (issue-rune S6)`（`3b78739e66326a020eb8e654ada9475800864135`）
- `git show --stat 3b78739` 两处文件，合计 +166 / −80：`crates/datazen-ui/src/lib.rs`（+240/−80 中的主体）、`crates/datazen-ui/src/data_grid.rs`（+6）。没有 `packages/frontend`、`packages/web`、`Cargo.toml`、`Cargo.lock`。React 未改
- 实现：`Section` 枚举收成 `Connections` / `Workspace` / `Settings` 三个变体（`lib.rs` 16–41 行），`Section::ALL` 只列这三项，`key()` / `label()` 相应收窄，`description()` / `next_step()` 连同 `datazen-preview-card` 占位卡整体删除。`page_body`（182–188 行）按节分发：连接页走 `connection_list`，设置页走 `settings_page`（`settings-empty` 空态卡），工作区走 `workspace_body`（193–201 行）——查询标签在选时画 SQL 编辑器加结果区，否则网格有已加载页时画网格，再否则 `workspace-empty` 空态。`DataGrid::has_page`（`data_grid.rs` 69–73 行）以 `source.is_some()` 判定，2 万行 × 200 列的 POC 演示面不再当作产品页出现。`sync_grid_to_selected_tab`（117–128 行）无条件把 `active` 拨到 `Workspace`：任何标签事件都落在工作区。标签条（`tab-strip`）只在 `active == Workspace` 时渲染，其余节画高 0 的 `tab-strip-absent`。`app_key` / `app_poll` / `app_ime`（478–500 行）以 `active == Workspace` 为闸，查询标签进编辑器，有页的网格进网格，空态下按键、轮询、IME 全部返回 false
- 探针在 `/tmp/s6_sidebar_probes.rs`，用 `#[path]` 挂进 `datazen-ui` 的测试模块。12 条，不是既有单测的拷贝。跑完已删探针。`crates/datazen-ui/src/lib.rs` 已还原，sha256 与挂探针之前相同（`s6_before.sha256` 三条全 OK：`621313…` / `34bf87…` / `1535a2…`）。`datazen-ui-api/src/lib.rs` 未动。没有改产品源码，没有改 `progress.md`，没有改既有报告，没有把任务标完成
- 门禁在挂探针之前、干净产品树上跑

## 门禁

环境：`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS="-C debuginfo=0"`。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 28 passed（S5 时 24，本提交净增 4：`the_sidebar_lists_only_connections_workspace_and_settings`、`a_workspace_without_a_tab_or_a_loaded_page_is_not_the_demo_grid`、`a_query_tab_stays_in_the_workspace_and_off_the_connections_page`、`the_settings_page_is_an_empty_state_not_the_old_preview_card`），1.46s；`datazen-ui-api` 3 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

编译期仍是既有告警：`rune` 的 6 条 `private_interfaces`；非测试构建里 `DataGrid::origin_col` / `is_editing` 和 `SqlEditor` 的 `doc` / `selection` / `main_head` / `extra_carets` 的 `dead_code`。本提交新增的 `has_page` 被 `workspace_body` / `app_key` / `app_poll` 用上，不在告警里。

## 探针

`cargo test -p datazen-ui --offline --lib s6_p -- --test-threads=1 --nocapture`。12 passed，0 failed。实际值来自该运行的命中路径与绘制。点选要求不透明度大于 0.5；直接改状态后 `settle_all` 再 `frame` 才重画。工作区页头下方：页内边距 24、页头高 48、间距 16、标签条高 32，所以工作区正文从 y=136 起；设置页没有标签条（`tab-strip-absent` 高 0），正文从 y=104 起。

### P1 三个入口自上而下，旧键在每一节都找不到

- 目的：侧栏只剩连接、工作区、设置，顺序稳定；四个旧占位键不是换了地方，而是不存在。
- 输入：`frame(0.0)`。记录三个 `nav-*` 的 `presented` 矩形。对 `nav-workspace` 中心旁一点做 `element_at`。然后依次点三个入口，每个 `frame(1.0)` 后再找 `nav-sql-editor`、`nav-data-browser`、`nav-schema`、`nav-workflows`。
- 预期：三个入口都在，y 递增；命中是 `nav-workspace` 自己；三个节里四个旧键全部 `None`。
- 实际：矩形 `conns=[16, 87.25, 192, 36]`、`work=[16, 127.25, 192, 36]`、`settings=[16, 167.25, 192, 36]`，不透明度都是 1，87.25 < 127.25 < 167.25。命中 `datazen-root/datazen-sidebar/#2/nav-workspace`。三轮节切换后四个旧键都没有。
- 判定：通过。

### P2 空工作区是空态卡，不是 2 万行演示网格

- 目的：没有标签也没有已加载页时，工作区不再挂出 POC 演示面。
- 输入：`frame(0.0)`，点 `nav-workspace`，`frame(0.0)` 再 `frame(1.0)`。读 `workspace-empty` 的矩形，数 0..100 里 `grid-row-*` 的挂载数。
- 预期：`workspace-empty` 呈现且不透明度 1；`data-grid` 键不存在；挂载数为 0。
- 实际：矩形 `[248, 136, 1392, 220]` 不透明度 1。`data-grid` 没有。挂载数 0。
- 判定：通过。

### P3 编辑器跟着工作区走，文档不丢

- 目的：查询标签的编辑器只在工作区；去连接页再回来，文档原样。
- 输入：接上 `app_key`。`frame(0.0)` 点 `open-query`，`frame(1.0)`。`sql.focus()` 后送文本 `S6`。点 `nav-connections`，`frame(0.0)`。再点 `nav-workspace`，`frame(0.0)`。
- 预期：工作区里 `sql-editor-field` 呈现；连接页上 `sql-editor-field`、`query-results` 都没有，`connection-local` 在；回到工作区编辑器在，文档仍以 `S6` 开头。
- 实际：与预期一致。文档前后都是 `S6-- customer lookup…` 开头（`SqlEditor::new` 的种子文档在 `S6` 之后）。
- 判定：通过。S4 时代 `page_body` 对 `query_selected` 的越节优先分支已经收敛进工作区。

### P4 标签条只属于工作区

- 目的：连接页和设置页不画标签条，也不关标签。
- 输入：点 `open-query`，`frame(1.0)`。依次点 `nav-connections`、`nav-settings`、`nav-workspace`，每步 `frame(0.0)` 后找 `tab-strip` 与 `tab-strip-absent`。
- 预期：工作区里 `tab-strip` 在；连接页与设置页 `tab-strip` 没有、`tab-strip-absent` 在；回工作区 `tab-strip` 回来，标签仍有 1 张。
- 实际：与预期一致。标签全程 1 张（`(1, Query, Query)`）。
- 判定：通过。

### P5 连接直达工作区，网格可点，连接不开标签

- 目的：S5 的点选路径不断——点连接行直接落在工作区，customers 页进网格，标签集合仍为空。
- 输入：`frame(0.0)`，记录 `connection-local` 命中，点它，`frame(1.0)`。读 `tab-strip` 矩形，点 `cell-0-1`，找 `tree-table-customers`。
- 预期：`active` 是 `Workspace`；`tab-strip` 在且标签 0 张；`has_page` 为真；点格后焦点 `(0, 1)`；树上有 customers。
- 实际：命中 `datazen-root/datazen-page/connection-list/connection-local`。`tab-strip` 矩形 `[248, 88, 1392, 32]` 不透明度 1，标签 0 张。`has_page` 真，焦点 `(0, 1)`，`tree-table-customers` 在。
- 判定：通过。

### P6 设置页是稳定空态

- 目的：设置入口有真实页面，不是旧占位卡；重复进入不变样。
- 输入：点 `nav-settings`，`frame(0.0)`，读 `settings-empty` 矩形。去工作区再回设置页，再读。
- 预期：`settings-empty` 呈现；`datazen-preview-card`、`connection-local`、`workspace-empty` 都没有；第二次进入相同。
- 实际：首次矩形 `[248, 104, 1392, 220]`，不透明度 0（进入帧淡入，`presented` 不看透明度）；第二次进入同矩形。`datazen-preview-card` 全程没有。设置页正文比工作区高 32 点（没有标签条）。
- 判定：通过。不透明度 0 只在进入那一帧，与 S5 断开按钮的淡入行为一致，不是缺陷。

### P7 断开后节留在工作区，旧格子还在，空态不出现

- 目的：S5 的合同不被 S6 破坏——断开清会话和标签，网格不可编辑；`active` 不被拨动（S5 遗留行为保持）。
- 输入：连接，`open_table_tab("customers")`，`frame(1.0)`，点 `disconnect`，`frame(1.0)`。
- 预期：会话 `None`，标签空，`tree-table-customers` 没了，`editable` false；`active` 仍是 `Workspace`；已加载页还在，网格照画，`workspace-empty` 不出现。
- 实际：与预期一致。`workspace-empty` 没有，`cell-0-1` 仍可找到。
- 判定：通过。

### P8 无会话直接开表标签：进工作区，空态兜底

- 目的：S5 遗留的 API 路径（断开后 `open_table_tab` 仍插入标签）在 S6 的新行为——`sync_grid_to_selected_tab` 无条件把 `active` 拨到工作区，表打不开时正文落到空态而不是演示网格。
- 输入：未连接直接 `open_table_tab("customers")`（得 id 1），`settle_all` 后 `frame(1.0)`。
- 预期：`active` 是 `Workspace`；`tab-1` 呈现；`workspace-empty` 呈现；`data-grid` 没有；会话仍 `None`。
- 实际：`tab-1` 矩形 `[248, 90, 77, 28]` 不透明度 1；`workspace-empty` 矩形 `[248, 136, 1392, 220]` 不透明度 1；`data-grid` 没有；会话 `None`。
- 判定：通过。相对 S5 报告 P10 的“节留在原处”，这是 S6 的有意变化：标签事件把用户带进工作区。

### P9 空态下按键全部落空

- 目的：没有查询标签也没有网格页时，`app_key` 不把按键喂给编辑器或网格。
- 输入：接上 `app_key`。记录种子文档。点 `nav-workspace`，`frame(1.0)`。依次送 Enter、Down、Right、Text("Q")，每个 `frame(0.0)`。
- 预期：网格焦点不动 `(0, 0)`，不进编辑，SQL 文档与种子一致。
- 实际：与预期一致。种子文档 `-- customer lookup…` 全程未变（`SqlEditor::new` 自带种子文本，不是空串）。
- 判定：通过。

### P10 编辑器输入只在查询标签的工作区里生效

- 目的：键与 IME 表面都收敛进工作区查询标签。`app_ime` 钩子是 rune 的 `pub(crate)`，测试运行时接不上，与既有单测 `chinese_ime_is_not_part_of_the_document_until_commit` 的做法一致：键路径走 `app_key`（可接线），IME 走 `sql-caret`（`El::ime_cursor` 表面）的存在性与模型路径，`app_ime` 的闸与 `app_key` 是同一表达式（代码核对）。
- 输入：接上 `app_key`。点 `open-query`，`frame(1.0)`，`sql.focus()`，`settle_all` 后 `frame(0.0)`。断言 `sql-caret` 呈现。送 Text("S6")。`preedit("设")` 后 `commit("设置")`。点 `nav-connections`，`frame(0.0)`，断言 `sql-editor-field` 与 `sql-caret` 都不在，再送 Text("X")。
- 预期：工作区里 `sql-caret` 呈现；键入后文档以 `S6` 开头；组合与提交后以 `S6设置` 开头；连接页上两个表面都没有，X 不进文档。
- 实际：`sql-caret` 呈现。文档依次变成 `S6…` 与 `S6设置…` 开头。连接页 `find("sql-editor-field")` 与 `find("sql-caret")` 都是 `None`，X 之后文档仍是 `S6设置-- customer lookup…` 开头，未再增长。
- 判定：通过。

### P11 关掉最后一张查询标签回到空态

- 目的：全部标签关完，工作区落回空态；标签本体与结果区一起走。
- 输入：点 `open-query`，`frame(1.0)`，断言 `query-tab` 在。点 `tab-close-1`，`frame(1.0)`。
- 预期：标签空；`query-tab` 没了；`workspace-empty` 呈现；`sql-editor-field` 没有。
- 实际：与预期一致。
- 判定：通过。SQL 文档不清是 S5 已记录的遗留，不在本单范围。

### P12 关掉最后一张表标签，已加载的网格仍在

- 目的：关标签不清网格页；仍连接时网格照旧可编辑。
- 输入：连接，点 `tree-table-customers`，`frame(1.0)`，断言标签 1 张。点 `tab-close-1`，`frame(1.0)`。
- 预期：标签空；`active` 仍是 `Workspace`；`workspace-empty` 不出现；`cell-0-1` 在；`editable` 真。
- 实际：与预期一致。
- 判定：通过。

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。侧栏自上而下只有 Connections（`[16, 87.25, 192, 36]`）、Workspace（`[16, 127.25, 192, 36]`）、Settings（`[16, 167.25, 192, 36]`）三个入口，四个旧键（`nav-sql-editor`、`nav-data-browser`、`nav-schema`、`nav-workflows`）在三个节里都找不到，`datazen-preview-card` 与 `description()` / `next_step()` 占位模式整体删除。空工作区是 `workspace-empty` 空态卡，0 行演示网格挂载。连接直达工作区且不开标签（S5 合同保持），表标签与网格的对应不变。查询标签的编辑器与结果区只在工作区绘制，连接页和设置页不再出现越节的编辑器（S4 行为收敛）。标签条只在 `active == Workspace` 时渲染。断开后节留在工作区、旧格照画、不可编辑（S5 遗留行为原样保持）。空态下按键、IME 表面、轮询全部收敛。产品源码未改（探针前后 sha256 一致）。

遗留（不构成拒绝）：

- `sync_grid_to_selected_tab` 现在无条件把 `active` 拨到 `Workspace`（`lib.rs` 117–128 行）。无会话时直接调 `open_table_tab`（S5 报告 P10 的 API 路径）会进工作区看到空态，树是空的点不出这条路径，界面行为不变。
- `workspace-empty` 的文案引导用户去对象树或新建查询；断开后的工作区因为旧页还在显示网格而非空态（S5 已记录“不擦格子”），文案与该状态不冲突。
- `app_ime` 钩子无法在测试运行时接线（rune `pub(crate)`），IME 的平台分发只能靠 `sql-caret` 表面存在性与 `app_key` 同闸的代码核对覆盖；真机 IME 旅程留给 H 任务。
- `settings-empty` 与 `workspace-empty` 是文案不同的同一张卡片样式，设置页真实内容留给 P1 起。
