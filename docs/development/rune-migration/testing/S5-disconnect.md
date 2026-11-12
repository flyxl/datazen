# TN S5 断开连接 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`0f21ead` `feat(rune): disconnect clears the session, tabs and grid editing (issue-rune S5)`（`0f21ead8a9e6486bd4da6dff00daf2fae06d2167`）
- `git show --stat 0f21ead` 三处文件，合计 +78：`crates/datazen-ui-api/src/lib.rs`（+6）、`crates/datazen-ui/src/data_grid.rs`（+16）、`crates/datazen-ui/src/lib.rs`（+56）。没有 `packages/frontend`、`packages/web`、`Cargo.toml`、`Cargo.lock`。React 未改
- 实现：`WorkspaceApi::disconnect`（`datazen-ui-api` 119–122 行）只把 `session` 和 `open` 清掉，不碰 `tables`。`AppState::disconnect`（`lib.rs` 117–121 行）再把 `tabs` 换成 `Tabs::new()`，并对网格 `set_editable(false)`。`set_editable(false)`（`data_grid.rs` 73–78 行）顺手把 `editing` 拉低。`begin_edit`（135–138 行）在 `editable == false` 时直接返回。`commit_grid_edit`（`lib.rs` 177–180 行）在不可编辑时直接返回，不会调用 `stage_cell`。断开按钮（350–364 行）只在 `session()` 为 `Some` 时画出键 `disconnect`；否则是高 0 的 `disconnect-absent`，没有文案、没有 `on_press`。侧栏在 `view` 里挂上它（417 行），不限当前节
- 探针在 `/tmp/s5_disconnect_probes.rs`，用 `#[path]` 挂进 `datazen-ui` 的测试模块。12 条，不是既有单测 `disconnect_clears_the_tree_and_blocks_editing` 的拷贝。跑完已删探针。`crates/datazen-ui/src/lib.rs` 已 `git checkout` 还原，sha256 `6261b11ff77ac8f92a2fbc2c5c8c44455a786f07aae1abc9fb866b5a5c233e55` 与挂探针之前相同。`data_grid.rs` 与 `datazen-ui-api/src/lib.rs` 的 sha256 也未变。没有改产品源码，没有改 `progress.md`，没有改既有报告，没有把任务标完成
- 门禁在挂探针之前、干净产品树上跑

## 门禁

环境：`source "$HOME/.cargo/env"`，`CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RAYON_NUM_THREADS=1 RUSTFLAGS="-C debuginfo=0"`。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 24 passed（含本提交的 `disconnect_clears_the_tree_and_blocks_editing`），1.42s；`datazen-ui-api` 3 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

编译期仍是既有 `rune` `private_interfaces` 告警。非测试构建里 `DataGrid::origin_col` / `is_editing` 和 `SqlEditor` 的 `doc` / `selection` / `main_head` / `extra_carets` 仍是既有 `dead_code`。本提交的 `disconnect`、`set_editable`、`is_editable` 已被按钮和 `commit_grid_edit` 用上，不在这份告警里。

## 探针

`cargo test -p datazen-ui --offline --lib s5_p -- --test-threads=1 --nocapture`。12 passed，0 failed（P1–P11 一次跑完，P12 同树补跑）。下面的实际值来自这两次运行的命中路径和绘制。直接改状态后要 `settle_all` 再 `frame`，否则运行时不重画；点选路径不需要这一步。点选要求不透明度大于 0.5。没有产品合同失败。

内存工作区在 `connect("local")` 之后表名是 `customers`、`orders`。`customers` 40 行 8 列，`(0,1)` 是 `customer-0`。断开按钮出现的那一帧不透明度是 0，`frame(1.0)` 后是 1。`find` 在不透明度为 0 时仍能找到键，但完全透明的节点点不中。合同不要求把网格里已经画着的格子擦掉，只要求不能再编辑。

### P1 连接之前没有断开按钮，只开查询标签也没有

- 目的：没有会话就不能出现断开控制。查询标签不是会话。
- 输入：`frame(0.0)`。再点 `open-query`，`frame(1.0)`。然后对 `disconnect-absent` 的中心做一次按下抬起。
- 预期：两步都没有键 `disconnect`。占位行高为 0。查询标签在，会话仍是 `None`。点占位行不造出会话。
- 实际：初始 `disconnect-absent` 矩形 `[16, 409.25, 192, 0]`，不透明度 1，`find("disconnect")` 是 `None`。`open-query` 不透明度 1，命中 `datazen-root/datazen-sidebar/open-query`，中心 `(112.0, 375.2)`。之后仍是查询标签，占位行高仍是 0。点 `(112.0, 409.2)` 命中侧栏本身 `datazen-root/datazen-sidebar`，不是断开按钮。会话仍是 `None`，标签仍是 1 张。
- 判定：通过。

### P2 连接并 frame(1.0) 后点断开，会话、两张表行和标签都没了

- 目的：按钮在淡入之后点得中。点下去要清会话、清标签、从树上拿掉 `customers` 和 `orders`。格子可以还留着，但必须不可编辑。
- 输入：`frame(0.0)` 点 `connection-local`。先 `frame(0.0)` 记按钮，再 `frame(1.0)`。点 `tree-table-orders`，`frame(1.0)`，点 `tree-table-customers`，`frame(1.0)`。点 `disconnect`，先 `frame(0.0)` 再 `frame(1.0)`。
- 预期：`frame(1.0)` 后按钮不透明度大于 0.9，两行树都在，没有 `disconnect-absent`。点完会话是 `None`，标签空，两行树的 `find` 都没有，`tab-1` / `tab-2` 没有，按钮自己也没有。`frame(1.0)` 后两行的 `presented` 也没有，树壳高度为 0。网格不可编辑。
- 实际：连接行不透明度 1，命中 `.../connection-list/connection-local`，中心 `(944.0, 187.0)`。断开按钮在 `frame(0.0)` 时矩形 `[16, 469.25, 192, 32]`，不透明度 0，`find` 仍给出 `(112.0, 485.25)`，但 `element_at` 命中的是侧栏，不是按钮。`frame(1.0)` 后不透明度 1。两行树不透明度都是 1。点断开命中 `.../disconnect`。会话 `None`，标签空，选中 `None`。同一帧 `find` 已经没有两行树和 `disconnect`，但 `presented("tree-table-customers")` 仍是 `[16, 341.25, 192, 28]` 不透明度 1（退出动画），树壳高仍是 60（28+4+28）。网格仍是 `customer-0`，40×8，`editable=false`，节留在 `DataBrowser`。`frame(1.0)` 后树壳高 0，两行和按钮的 `presented` 都没有，`disconnect-absent` 在。
- 判定：通过。格子没有被擦掉，合同允许。树行在 1 秒后不再绘制。

### P3 断开之后 begin_edit 和回车都不进入编辑

- 目的：连接时编辑真的开得起来。断开把正在进行的编辑关掉。之后 `begin_edit` 和 Data Browser 上的 Enter 都留在 `is_editing == false`，也不再挂 `grid-editor`。
- 输入：接上 `app_key`。连接并 `frame(1.0)`。`select(0, 1)`，`begin_edit`，`insert("-live")`。点 `disconnect`，`frame(1.0)`。再 `begin_edit`。再送 Enter，`frame(0.0)`。
- 预期：连接时 `is_editing`。断开后立刻不是，且没有 `grid-editor`。`begin_edit` 不把 `is_editing` 设回 true，也不改编辑器缓冲。Enter 之后仍不在编辑，焦点不动，没有 `grid-editor`。
- 实际：连接时缓冲是 `customer-0-live`。断开按钮命中与 P2 相同。断开后 `is_editing=false`，`editable=false`，没有 `grid-editor`。`begin_edit` 没改缓冲。Enter 之后仍是 `editing=false`，焦点 `(0, 1)`，格子 `customer-0`，没有 `grid-editor`。
- 判定：通过。

### P4 断开之后提交和 stage_cell 都不改已存的 customers 格

- 目的：编辑进行到一半就断开，不能把没提交的字刷进表。之后改编辑器缓冲再 `commit_grid_edit`，以及直接 `stage_cell`，都不能改存储。
- 输入：连接，`open_table_tab("customers")`，`settle_all` 后 `frame(0.0)`。确认 `(0,1)=customer-0`。`begin_edit`，`insert("-nope")`，缓冲应为 `customer-0-nope`。点 `disconnect`。再把缓冲改成 `hacked-from-editor` 并 `commit_grid_edit`，再 `stage_cell(0, "name", "hacked-from-stage")`，再 `begin_edit` + `commit_grid_edit`。每次用重新 `connect` + `open_table` 读回名字，读完再把端口断开。
- 预期：点断开的当时存储仍是 `customer-0`。`stage_cell` 失败。之后读回仍是 `customer-0`。
- 实际：`stage_cell` 返回 `Err("no open table")`。三次读回都是 `customer-0`。
- 判定：通过。

### P5 再连接恢复树和编辑

- 目的：断开后从界面再连上 `local`。树回到两张表，按钮回来，`begin_edit` 重新有效，`commit_grid_edit` 能把新字写入端口。
- 输入：连接，点 `tree-table-customers`，`frame(1.0)`，点 `disconnect`，`frame(1.0)`。确认树上没有 customers、网格不可编辑、`begin_edit` 无效。点 `nav-connections`，`frame(1.0)`，再点 `connection-local`，`frame(1.0)`。然后 `select(0, 1)`，`begin_edit`，`insert("-again")`，`commit_grid_edit`。
- 预期：断开后节仍是 Data Browser，页面上没有 `connection-local`。走过导航再连上之后，会话是 `local`，表名含 `customers` 和 `orders`，两行树都在，断开按钮在，`editable`，格子是 `customer-0`。提交后端口里的名字是 `customer-0-again`，不再处于编辑。
- 实际：断开后节是 `DataBrowser`，`connection-local` 的 `find` 为空。导航命中 `.../nav-connections`，中心 `(112.0, 105.2)`。再连接后表列表是 `["customers", "orders"]`，`editable=true`。`begin_edit` 进入编辑，提交后 `open_table("customers").rows[0][1]` 是 `customer-0-again`。
- 判定：通过。断开没有把节拨回 Connections，见遗留。再连接这条点选路径是通的。

### P6 断开不删内存里的 customers 行，重开仍是 customer-0

- 目的：存储不被删。未改过的表在断开之后整页还在，包括 orders。
- 输入：连接后读 customers 和 orders。点 `disconnect`，`frame(1.0)`。断开时 `open_table("customers")`。再 `connect("local")` 重开两张表。
- 预期：断开前 40 行，`(0,1)=customer-0`，`(39,1)=customer-39`，`(0,2)=Hangzhou`，orders 是 `order-0`，8 列。断开时打开表失败，错误是 `not connected`。重开后行数、上述格子、邮箱列和 orders 都不变。
- 实际：与预期一致。`open_table` 在断开时是 `Err("not connected")`。重开 `(0,1)=customer-0`，`(39,1)=customer-39`，`(7,6)=c7@example.com`，8 列，orders 是 `[["1", "order-0"]]`。
- 判定：通过。

### P7 已经打开的查询标签也被清掉

- 目的：不只有表标签。表加查询一起打开时，断开把两张都拿掉，查询页和结果区跟着走。
- 输入：连接，点 customers，`frame(1.0)`，点 `open-query`，`frame(1.0)`。点进 `sql-editor-scroll`，`focus`，送文本 `SELECT S5`。点 `disconnect`。先 `frame(0.0)` 再 `frame(1.0)`。
- 预期：点之前两张标签，当前是 Query，`query-tab` 和 `query-results` 都在，文档以 `SELECT S5` 开头。点完标签空，选中 `None`，`find` 没有查询页、结果区、`tab-1`、`tab-2` 和树行，会话是 `None`。`frame(1.0)` 后结果区和查询页不再 `presented`。文档不被写坏。
- 实际：查询按钮在连上之后中心是 `(112.0, 435.2)`（树占了高度）。文档以 `SELECT S5` 开头。点断开的那一帧 `find` 已经没有结果区，但 `presented("query-results")` 仍是 `[248, 595, 1392, 120]`，不透明度 1。`frame(1.0)` 后 `query-results`、`query-tab`、`tab-1` 的 `presented` 都没有，也没有 `grid-editor`。文档仍以 `SELECT S5` 开头，和断开前同一个字符串。
- 判定：通过。SQL 文档没有被清掉，见遗留。结果区本身在 1 秒后已经不在。

### P8 连按两次断开是空操作

- 目的：不恐慌。第二次仍然是断开状态，标签不会被第二次调用救回来，存储也不被动。
- 输入：从未连接时直接 `disconnect()` 两次，`settle_all` 后 `frame(0.0)`。再连接，点开 orders 和查询。对断开按钮连续两次按下抬起，中间不 `frame`。然后再 `disconnect()` 一次并 `frame(1.0)`。读回 customers。
- 预期：两次调用之后会话是 `None`，标签空，网格不可编辑，没有断开按钮。同一帧连点两次之后仍然如此，标签不会复活。再调用一次也不恐慌。读回是 `customer-0`。
- 实际：与预期一致。连点用的是上一帧的命中（按钮中心 `(112.0, 485.2)`）。读回 `customer-0`。
- 判定：通过。从未连接时直接调用也会把默认的可编辑标志拉低，见遗留。按钮在没有会话时不出现，界面点不到这条路径。

### P9 断开前没暂存的粘贴不会被刷进表

- 目的：粘贴只写网格覆盖层。断开和之后的 `commit_grid_edit` / `stage_cell` 不能把这层写进端口。`begin_edit` 仍然无效。
- 输入：连接，打开 customers 标签。`select(0, 1)`，`paste_tsv("PASTED")`。确认端口仍是 `customer-0`。点 `disconnect`。`commit_grid_edit`，`stage_cell(0, "name", "PASTED")`，`begin_edit`。
- 预期：粘贴当时网格格子变成 `PASTED`，端口仍是 `customer-0`。断开后 `stage_cell` 失败，读回仍是 `customer-0`，`is_editing` 为 false。
- 实际：与预期一致。`stage_cell` 是 `Err("no open table")`。断开并提交之后网格格子仍是 `PASTED`，`editable=false`。端口是 `customer-0`。
- 判定：通过。覆盖层还在，合同不要求擦格子。它没有进存储。

### P10 断开之后失败的重开不能把编辑打开

- 目的：清掉的标签 id 选不中、关不掉。`show_table` 失败时不能把 `editable` 设回 true。未知连接 id 不能变出会话，也就不能变出按钮。
- 输入：连接，打开 customers 标签，`frame(1.0)`，点断开，`frame(1.0)`。对旧 id 调用 `close_tab` 和 `select_tab`。`show_table("customers")`，`begin_edit`，`open_table`。再 `open_table_tab("customers")`，`begin_edit`，`commit_grid_edit`。然后 `open_connection("missing")`，`settle_all` 后 `frame(0.0)`。
- 预期：旧 id 的关闭和选中都是 false。`show_table` 是 false，网格仍不可编辑，`begin_edit` 无效，`open_table` 错误是 `not connected`。即便端口调用又打开了一张表标签，编辑仍关着，存储仍是 `customer-0`。`missing` 之后会话是 `None`，没有断开按钮。
- 实际：与预期一致。`open_table_tab` 在断开之后仍返回 `Some(1)`，标签是 `(1, Table, customers)`。`editable` 仍是 false，`begin_edit` 无效，读回 `customer-0`。`missing` 没有按钮。
- 判定：通过。断开之后直接调用 `open_table_tab` 仍会插入一张表标签，见遗留。树上已经没有能点的行，编辑没有被重新打开。

### P11 已经提交的修改留着，没提交的第二次修改不留

- 目的：断开不回滚已经 `commit_grid_edit` 的值，也不把断开前塞进编辑器、还没提交的字写进去。再连上之后还能再改一次，第二次断开仍留着新值，行数仍是 40。
- 输入：接上 `app_key`。连接，点 customers，`frame(1.0)`。Enter 进入编辑，`frame(0.0)`，`insert("-kept")`，`commit_grid_edit`。再 `begin_edit`，`insert("-lost")`，点 `disconnect`，`frame(1.0)`，`commit_grid_edit`。点 `nav-connections`，`frame(1.0)`，再连接。确认存储。`begin_edit`，`insert("-two")`，提交，再点断开，并再调一次 `disconnect()`。
- 预期：第一次提交后端口是 `customer-0-kept`。第二次编辑的缓冲是 `customer-0-kept-lost`。断开并 `commit_grid_edit` 之后存储停在 `customer-0-kept`，不在编辑。再连接后 `editable`，页仍是这个名字、40 行。再提交并断开后存储是 `customer-0-kept-two`。
- 实际：与预期一致。Enter 当时 `grid-editor` 在。两次断开之后读回 `customer-0-kept-two`。
- 判定：通过。

### P12 重建之后点旧树行不会把标签开回来；断开之后的粘贴不进存储

- 目的：断开后的下一帧，原来的 customers 行就算还在退场，也不能再当命中目标。退场结束后按下旧坐标不开标签。断开之后的 `paste_tsv` 可以改网格覆盖层，但不能改端口里的城市，也不能进入编辑。
- 输入：连接并 `frame(1.0)`，记下 `tree-table-customers` 的中心，点 `disconnect`。同一帧只做 `element_at`，不按下。`frame(0.0)` 后再看 `presented` 和 `element_at`，然后按下抬起，`frame(0.0)`。再 `frame(1.0)`。然后 `select(0, 2)`，`paste_tsv("AFTER")`，`begin_edit`，`commit_grid_edit`，`stage_cell(0, "city", "AFTER")`。重新连接并 `open_table`。
- 预期：`frame(0.0)` 之后的按下不产生标签，会话仍是 `None`，网格不可编辑。`frame(1.0)` 后 customers 行不再 `presented`。粘贴之后不在编辑，`stage_cell` 失败。端口 `(0,1)` 仍是 `customer-0`，`(0,2)` 仍是 `Hangzhou`。
- 实际：点断开之前，同一帧的旧中心命中仍是 `.../object-tree/tree-table-customers`（这一帧没有按下）。`frame(0.0)` 后行仍 `presented` 为 `[16, 341.25, 192, 28]` 不透明度 1，但命中变成 `.../object-tree`，不再是那一行。按下后标签仍空。`frame(1.0)` 后 `presented` 没有这一行。粘贴后网格 `(0,2)` 是 `AFTER`，`editing=false`，`stage_cell` 是 `Err("no open table")`。端口是 `customer-0` / `Hangzhou`。
- 判定：通过。断开之后 `paste_tsv` 仍改网格覆盖层，见遗留。它不进存储，也开不了编辑。

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。没有会话时没有 `disconnect`（只有高 0 的占位），只开查询标签也没有。连接并 `frame(1.0)` 后按钮不透明度为 1，点它清掉会话、全部标签和树上的 `customers` / `orders`。`begin_edit` 与 Enter 都不进入编辑，`commit_grid_edit` 和 `stage_cell` 不改存储（错误分别是直接返回和 `no open table`）。再点 Connections 里的 `local` 能恢复树和编辑，并能再写回一格。内存表仍是 40 行，重开 `open_table` 仍返回 `customer-0`，已提交的修改也不会被断开删掉。查询标签和结果区一并清掉。连点两次断开不恐慌。产品源码未改。

遗留（不构成拒绝）：

- 断开不改 `active`（`lib.rs` 117–121 行）。P5 之后节留在 Data Browser，连接列表不在页面上，要点 `nav-connections` 再 `frame(1.0)` 才能点到 `connection-local`。再连接本身是成功的。
- 网格页不被 `load_page` 清掉。P2 断开后仍是 40×8 的 `customer-0`，节还是 Data Browser，所以旧格子还画着。合同写明不要求擦格子。
- `paste_tsv`（`data_grid.rs` 169–187 行）和滚动区的 `on_paste`（321 行）不看 `editable`。P9 里断开前粘上的 `PASTED` 留在网格模型里。P12 里断开之后再粘 `AFTER`，网格 `(0,2)` 变成 `AFTER`，但 `stage_cell` 失败，端口城市仍是 `Hangzhou`，`begin_edit` 仍是 false。暂存路径是关的。
- 新出现的断开按钮在进入那一帧不透明度是 0（P2 矩形已经是 32 高）。`element_at` 落到侧栏，点不中。`frame(1.0)` 后不透明度是 1，点得中。树行和 `query-results` 在断开的那一帧仍以不透明度 1 `presented`，`find` 已经跳过退出中的节点；再过 1 秒，树壳高度从 60 收到 0，结果区也不再呈现。同一帧里旧树行的命中路径还在（P12 只读了 `element_at`，没有在那一帧按下）。下一帧按下命中的是空的 `object-tree`，标签没有回来。
- `open_table_tab`（123–127 行）不检查会话。P10 在树已经空了之后直接调用，仍得到 id 1 的 customers 表标签。`show_table` 失败，不把 `editable` 设回 true，存储不变。侧栏上已经没有 `tree-table-customers` 可以点出这条路径。`Tabs::new()` 把 id 从 1 重新计，这是换了一个新的标签容器，不是同一容器里复用 id。
- 查询标签清掉之后 SQL 文档还在。P7 断开前后都是以 `SELECT S5` 开头的同一份字符串。合同要清的是标签和结果区。
- 从未连接时没有按钮。直接调用 `AppState::disconnect` 仍会把网格从默认的可编辑改成不可编辑（P8）。界面走不到。再点连接会经 `open_connection`（109–114 行）把 `editable` 设回 true。
