# TN S4 查询标签 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`e5f29ca` `feat(rune): put the SQL editor above an empty result on a query tab (issue-rune S4)`（`e5f29ca02d10d32052e0377f3b3b9c47977341e8`）
- `git show --stat e5f29ca` 两处文件：`crates/datazen-ui/src/lib.rs`（+79 / −10）和原先未跟踪的 `docs/development/rune-migration/testing/S3-table-tab.md`。没有 `packages/frontend`、`packages/web`、`Cargo.toml`、`Cargo.lock`。按提交说明，S3 拒绝报告是顺手收进来的文档，不拿它判 S4 代码。React 未改
- 实现：侧栏 `open-query`（324–337 行）调用 `open_query_tab("Query")`（122–126 行）。选中查询时 `sync_grid_to_selected_tab`（147–156 行）只把节设成 `SqlEditor`，不调用 `show_table`。`page_body`（209–221 行）在 `query_selected`（205–207 行）时整页换成 `query_workspace`（305–322 行）：键 `query-tab`，子节点是现有 SQL 编辑器面板，子节点键 `query-results`，文本 `No results`。`app_key`（468–476 行）和 `app_ime`（482–484 行）在节是 SQL Editor 或当前是查询标签时进编辑器
- 探针在 `/tmp/s4_query_probes.rs`，用 `#[path]` 挂进 `datazen-ui` 的测试模块。12 条，不是既有单测 `a_query_tab_keeps_the_editor_above_an_empty_result` 的拷贝。跑完已删探针。`crates/datazen-ui/src/lib.rs` 已 `git checkout` 还原，sha256 `39a184a56e6351f7a3cc06ea526b626ac5fc70baee82c9663a34c058bada1345` 与挂探针之前相同。没有改产品源码，没有改 `progress.md`，没有改既有报告，没有把任务标完成
- 门禁在挂探针之前、干净产品树上跑

## 门禁

环境：`source "$HOME/.cargo/env"`，`CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RAYON_NUM_THREADS=1 RUSTFLAGS="-C debuginfo=0"`。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 23 passed（含本提交的 `a_query_tab_keeps_the_editor_above_an_empty_result`）；`datazen-ui-api` 3 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

编译期仍是既有 `rune` `private_interfaces` 告警。`cargo check` 里 `open_query_tab` 不再是 `dead_code`（按钮已调用）。非测试构建里 `DataGrid::origin_col` / `is_editing` 和 `SqlEditor` 的 `doc` / `selection` / `main_head` / `extra_carets` 仍是既有 `dead_code`，与查询标签布局无关。

## 探针

`cargo test -p datazen-ui --offline --lib s4_p -- --test-threads=1 --nocapture`。12 passed，0 failed。下面的实际值来自这次运行的模型、命中路径和绘制。直接改状态后要 `settle_all` 再 `frame`，否则运行时不重画；点选路径不需要这一步。没有产品合同失败。

内存工作区在 `connect("local")` 之后表名是 `customers`、`orders`。编辑器初始文档是 R1 样本，以 `-- customer lookup` 开头，不是空文档。结果区要的是空结果，不是空 SQL。新节点在点下去的那一帧不透明度是 0，`frame(1.0)` 后是 1。`find` 在不透明度为 0 时仍能找到键，但完全透明的节点点不中。

### P1 还没有查询标签时，没有 query-results

- 目的：结果区不能提前挂上。空壳、点进 SQL Editor 节、以及打开一张表之后，都没有 `query-results`。
- 输入：`frame(0.0)` 看初始壳。点 `nav-sql-editor`，`frame(1.0)`。再点 `nav-connections`，`frame(1.0)`（连接列表是新挂上的，要等淡入），点 `connection-local`，`frame(1.0)` 点 `tree-table-customers`，再 `frame(1.0)`。
- 预期：三步都没有 `query-tab` / `query-results`，`presented("query-results")` 也没有。SQL 节有 `sql-editor-field`，没有网格行。表标签选中后网格是 customers，40×8，`(0,1)=customer-0`，画出 `data-grid` 和 `grid-row-0`。
- 实际：初始节 Connections，标签空，`open-query` 在，结果区和查询页都不在。SQL 节点中后节是 SqlEditor，编辑器在，结果区不在，没有网格行。表打开后选中 `(1, Table, customers)`，节 DataBrowser，`c0=0`、`c1=customer-0`、`c2=Hangzhou`，40×8，`grid-row-0` 和 `data-grid` 在，`query-results` 的 find 和 presented 都没有。另一次把连接列表只 `frame(0.0)` 就点：`connection-local` 不透明度 0，命中落到 `datazen-page`，树行不会出现。这是淡入，不是结果区提前出现。
- 判定：通过。

### P2 点 New query 并等一帧，编辑器在空结果区上面

- 目的：按钮打开的就是查询页。两块都在，编辑器在上，结果区没有行。
- 输入：`frame(0.0)` 点 `open-query`。先 `frame(0.0)` 记不透明度，再 `frame(1.0)`。
- 预期：选中 `(1, Query, Query)`，节 SqlEditor。`query-tab`、`sql-editor-field`、`query-results` 都在，不透明度大于 0.9。结果区文本只有 `No results`，高度约 120。没有 `grid-row-*`、`data-grid`、`cell-0-0`。`sql-editor` 的底边不高于 `query-results` 的顶边。结果区可见高度大于 40。
- 实际：`open-query` 不透明度 1，命中 `datazen-root/datazen-sidebar/open-query`，中心 `(112.0, 375.2)`。`frame(0.0)` 时三个键都能 find，不透明度都是 0。`frame(1.0)` 后不透明度都是 1。编辑器矩形 `[248, 167, 1392, 420]`，结果区 `[248, 595, 1392, 120]`，间隔 8，和 `query-tab` 的 `gap(8)` 一致。`query-tab` 高 548（420+8+120）。结果区文本 `["No results"]`。没有网格键。节 SqlEditor。
- 判定：通过。

### P3 点进编辑器再打字，文档变了，结果区还在

- 目的：不是直接 `focus()`。未聚焦时按键不改文档。聚焦后输入、回车、退格都不丢掉文档，也不拆掉结果区。回车不能去开网格编辑。
- 输入：点 `open-query`，`frame(1.0)`。未聚焦时送 `SHOULD-NOT`。点 `sql-editor-scroll`，`frame(0.0)`。再送 `QRY`、Enter、`Z9`，然后 Backspace。
- 预期：未聚焦时文档仍以 `-- customer lookup` 开头。点中滚动区后 `focused`，主光标 `sql-caret` 在，`ime_area` 有值。文档以 `QRY\nZ9` 开头，样本尾巴还在。视图里看得到 `QRY` 和 `Z9`。结果区仍是 `No results`，`query-tab` 和 `sql-editor-field` 都在，没有网格行，网格不在编辑。退格后以 `QRY\nZ` 开头，结果区还在。
- 实际：与预期一致。点滚动区命中 `.../query-tab/sql-editor/sql-editor-scroll/sql-editor-field`。打完 `caret=6`，`ime_area=[248, 221, 2, 16]`。退格后 `caret=5`，文档 `QRY\nZ-- customer lookup...`。两段的结果区文本都是 `["No results"]`。网格焦点一直是 `(0, 0)`，`editing=false`。
- 判定：通过。

### P4 第二张查询标签不清文档

- 目的：再点一次 New query。按当前实现只有一份编辑器。第二张标签不能把第一张已经打进去的字换成另一份空文档。
- 输入：打开查询，点进编辑器，打 `FIRST-DOC`。再点 `open-query`，`frame(1.0)`。点回第一张标签，再追加 `MORE`，点到第二张。
- 预期：两张都是 Query，标题都是 `Query`，id 不复用，选中第二张。文档仍以 `FIRST-DOC` 开头，视图里也是这串字，结果区仍是 `No results`，没有网格行。点回第一张，字还在。追加后两张看到的都是 `FIRST-DOCMORE`。
- 实际：id 1 和 id 2，选中 2。文档和视图都是 `FIRST-DOC` 加样本。点回 id 1 后不变。追加后选中 id 2，文档和 `sql-line` 文本都是 `FIRST-DOCMORE-- customer lookup...`。结果区一直是 `No results`。
- 判定：通过。文档没有被第二张标签清掉。两张查询共用 `state.sql`，记在遗留里，不作为失败。

### P5 切到表标签看见那张表，切回来结果区和打过的字都在

- 目的：表标签仍装自己的网格，并藏起 `query-results`。再选中查询时结果区回来，打过的 SQL 还在。查询打开的当时也不能把已经装入的 orders 网格换成别的内容。
- 输入：点 `connection-local`，`frame(1.0)` 点 `tree-table-orders`，`frame(1.0)`。确认网格后再点 `open-query`，`frame(1.0)`。点进编辑器，打 `AFTER-ORDERS`。点 `tab-1`，`frame(1.0)`。再点 `tab-2`，`frame(1.0)`。
- 预期：orders 是 id 1，表，`(0,1)=order-0`，1×2，有 `grid-row-0`，没有结果区。查询打开后网格模型仍是这一页，视图没有 `data-grid` / `grid-row-0`，有结果区。点回 orders：节 DataBrowser，结果区 find 和 presented 都没有，`query-tab` 没有，画出的 `cell-0-1` 含 `order-0`，1×2。SQL 文档不变。再点回查询：结果区文本 `No results`，视图含 `AFTER-ORDERS`，没有网格行，模型 `(0,1)` 仍是 `order-0`。
- 实际：与预期一致。打开查询前 `c1=order-0`。查询页上网格模型仍是 1×2、`order-0`，焦点 `(0, 0)`。点 `tab-1` 不透明度 1，命中 `tab-strip/tab-1/#0`。回来后编辑器矩形仍是上面 420、结果区 y=595 高 120，文档以 `AFTER-ORDERS` 开头。
- 判定：通过。

### P6 关掉查询标签后 query-results 消失

- 目的：只剩一张查询时关掉它，结果区要离开树。旁边还有表时，关掉查询后要落到那张表并画出它的网格。两种关法都不清 SQL 文档。
- 输入：只开查询，打 `LONE`，点 `tab-close-1`。先 `frame(0.0)` 再 `frame(1.0)`。另起一趟：连接后点 `tree-table-customers`，再开查询，打 `WITH-TABLE`，点 `tab-close-2`，`frame(1.0)`。
- 预期：单独那张关掉后标签空，选中 `None`。`frame(1.0)` 后 `query-results` 的 find 和 presented 都没有，`query-tab` 没有。文档仍以 `LONE` 开头。和表一起时，关掉查询后选中 customers，种类 Table，结果区没有，`data-grid` 和 `grid-row-0` 在，`(0,1)=customer-0`，40×8，文档仍以 `WITH-TABLE` 开头。
- 实际：关闭键不透明度 1，命中 `tab-close-1/#0`。`frame(0.0)` 时 find 已经没有 `query-results`，但 presented 仍是不透明度 1（退出动画还挂着）。`frame(1.0)` 后 presented 也没有。节仍是 SqlEditor，所以不带结果区的编辑器还在，文档是 `LONE-- customer lookup...`。和第二趟：关 `tab-close-2` 后只剩 `(1, Table, customers)`，节 DataBrowser，`c1=customer-0`，40×8，画出 `grid-row-0` 起的行窗，结果区没有，文档是 `WITH-TABLE...`。
- 判定：通过。关掉唯一一张查询后节不回到 Connections，编辑器截面还在，见遗留。结果区本身在 1 秒后已经不在。

### P7 app_ime 跟查询走，不跟表走

- 目的：查询标签把输入法交给 SQL 编辑器。预编辑不进文档，提交才进，结果区留着。把节改成 DataBrowser 但查询标签还选着时，仍走编辑器。换成表标签后，即使编辑器焦点标志还在，也不再改文档。
- 输入：点开查询，`frame(1.0)`。未聚焦时 `app_ime(Commit("未聚焦"))`。点进编辑器。`Preedit("中")`，`settle_all` 后 `frame(0.0)`。`Commit("中文")` 再重画。然后把 `active` 设成 DataBrowser，再 `Commit("旁路")`。接着 `connect("local")` 且 `open_table_tab("orders")`（查询页占着正文时连接列表不在树上，这条用 API），重画。对这份文档再 `Preedit("表")` 和 `Commit("订单")`。
- 预期：未聚焦时处理结果是 false，文档不动。预编辑处理结果 true，文档不变，视图里有 `中`，结果区在。提交后文档以 `中文` 开头，样本还在，组合结束，视图有 `中文`，结果区文本仍是 `No results`。节被改成 DataBrowser 后查询仍选中，`旁路` 写进文档，视图仍是查询页，没有 `data-grid`。表标签选中后节 DataBrowser，两次输入法都是 false，组合是 None，文档停在提交表标签之前，结果区没有，网格 `(0,1)=order-0`。
- 实际：与预期一致。提交后文档 `中文-- customer lookup...`，视图第一段是 `中文`，`ime_area=[274, 203, 2, 16]`。`旁路` 之后文档 `中文旁路-- customer lookup...`，查询页还在。orders 是 id 2，表，1×2，`c1=order-0`，`query-results` 没有，文档没有 `订单`。焦点标志仍是 true，输入法仍被表标签挡住。
- 判定：通过。

### P8 查询标签上的键进编辑器，表标签上的键进网格

- 目的：`app_key` 的两条路。查询页上的方向键和回车不能推动网格或开始网格编辑。表页上的方向键要移动网格焦点，文本键不能改 SQL，回车要开始网格编辑。
- 输入：连接并点 `tree-table-customers`。点 `open-query`，点进编辑器。送 Right、Down、Enter。点回表标签。再送 Right、Down、文本 `NO-SQL`、Enter。点回查询。
- 预期：查询上网格焦点停在 `(0, 0)`，不在编辑。Enter 在文档开头插入换行，文档以 `\n` 开头，结果区在，没有网格行。表上焦点变成 `(0, 1)` 再变成 `(1, 1)`。`NO-SQL` 不进 SQL。`(0,1)` 仍是 `customer-0`。Enter 后 `is_editing`。回到查询后结果区在，没有 `grid-editor`，文档仍以换行开头。
- 实际：与预期一致。表上编辑时 `ime_area` 约 `[439.2, 248, 2, 15]`，那是网格编辑器，不是 SQL。回到查询后模型里 `editing` 仍是 true、焦点仍是 `(1, 1)`、`c1=customer-0`，但 `grid-editor` 没有挂上，结果区是 `No results`。查询没有清掉这次未结束的网格编辑会话，也没有把它画出来。
- 判定：通过。未结束的网格编辑留在模型里，见遗留。

### P9 查询标签不替换已经装入的网格

- 目的：标题恰好是 `orders` 也不能按标题去装 orders。打开和再次选中都不清已暂存的名字、未暂存的粘贴和焦点。选中真实 customers 时允许按 S3 重载。
- 输入：`connect("local")`，`open_table_tab("customers")`。`(0,1)` 提交成 `customer-0-staged`，`(0,2)` 粘贴 `CITY-TEMP`，焦点设到 `(4, 5)`。`open_query_tab("orders")`。读端口里的 orders 和 customers。重画。再 `select_tab` customers，重画。在 `(0,2)` 粘贴 `CITY-2`，再选中这条查询，重画。
- 预期：查询 id 不同于 customers，种类 Query，标题是 trim 后的 `orders`。打开当时网格仍是 `customer-0-staged`、`CITY-TEMP`，焦点 `(4, 5)`，40×8，不是 `order-0`，也不是 1×2。端口 orders 的名字仍是 `order-0`，customers 的名字是已暂存的 `customer-0-staged`，城市仍是 `Hangzhou`。重画后结果区是 `No results`，没有 `data-grid`，格子和焦点不变。选中 customers 会重载：名字仍是暂存值，城市回到 `Hangzhou`，焦点 `(0, 0)`，有 `grid-row-0`，结果区没有。再选中查询时 `CITY-2` 和焦点 `(0, 2)` 还在，结果区回来，没有网格行。
- 实际：与预期一致。查询是 id 2。重画后编辑器在上、结果区 `[248, 595, 1392, 120]`，`c2=CITY-TEMP`，焦点 `(4, 5)`。选回 customers 后 `c1=customer-0-staged`、`c2=Hangzhou`、焦点 `(0, 0)`、40×8，行窗从 `grid-row-0` 画到视口内。再选查询后 `c2=CITY-2`，焦点 `(0, 2)`，结果区在，没有网格行。
- 判定：通过。查询没有装表，也没有擦掉未暂存粘贴。回到表标签时的重载是 `show_table` → `load_page`，见遗留。

### P10 空白标题和未知 id 不造出结果区

- 目的：打不开的标题、选不中的 id、关不掉的 id 不动已有查询和文档。带空白的查询标题按 trim 打开，文档还在。未知表名打开标签，但不清网格，也不把结果区留在选中的表上。
- 输入：点开查询，打 `KEEP`。`open_query_tab` 对 `"   "`、`"\t\n"`、`""`，以及 `select_tab(0)`、`select_tab(99)`、`close_tab(0)`、`close_tab(99)`。再 `open_query_tab("  draft  ")`。然后 `connect`，在默认网格 `(0,0)` 粘贴 `STILL`，`open_table_tab("nope")`。再选中 draft。
- 预期：拒绝的调用之后仍是原来那一张查询，文档以 `KEEP` 开头，结果区在。draft 的标题是 `draft`，id 新的，节 SqlEditor，文档和视图仍有 `KEEP`，结果区文本 `No results`，没有网格行。nope 是表标签且被选中。粘贴 `STILL` 还在。重画后没有 `query-results` / `query-tab`，文档仍是 `KEEP`。再选 draft：结果区回来，`STILL` 还在。
- 实际：与预期一致。拒绝之后选中 id 1，文档 `KEEP-- customer lookup...`。draft 是 id 2。nope 是 id 3，节留在 SqlEditor（`show_table` 失败，不改节），页面是不带结果区的编辑器，`c0=STILL`，20000×200，没有 `data-grid`。选回 draft 后结果区在，`STILL` 还在，文档未变。
- 判定：通过。未知表没有擦网格。节留在 SqlEditor 所以网格没有画出来，见遗留。

### P11 查询页用的是原来的编辑器面板

- 目的：不是一块画了 SQL 的静态替身。长行仍比视口宽。Add cursor 和多光标插入仍工作，结果区留在下面。
- 输入：点 `open-query`，`frame(1.0)`。比 `sql-editor-field` 和 `sql-editor-scroll` 的宽。点 `sql-add-cursor`，再点进编辑器，打 `AB`。
- 预期：字段宽比滚动视口至少大 100。有 `sql-editor`、`sql-add-cursor`、`sql-cursor-count`，计数以 `1 cursor` 开头。点完后光标数是 2。`AB` 同时出现在文档开头和末尾，至少两处。结果区仍是 `No results`，没有网格行。计数变成 `2 cursor`。
- 实际：字段宽 2800，视口宽 1392。Add cursor 命中 `.../query-tab/sql-editor/#0/sql-add-cursor`。文档是 `AB-- customer lookup...;\nAB`，视图三段含首尾的 `AB`，`caret=273`。结果区 `[248, 595, 1392, 120]`，文本 `No results`。
- 判定：通过。

### P12 查询还选着时，侧栏改节不换掉查询正文

- 目的：查询标签占着正文时，Data Browser / Schema / SQL Editor 的导航不能把结果区换成网格或占位卡，也不能改 SQL。没有查询被选中时，SQL Editor 节只有编辑器，没有结果区，也不重载网格。
- 输入：连接，点 customers，点 `open-query`，打 `NAV`。点 `nav-data-browser`，`frame(1.0)`。点 `nav-schema`。`app_ime(Commit("键"))`。点 `nav-sql-editor`。点表标签，再点 `nav-sql-editor`，`frame(1.0)`。点回查询标签。
- 预期：Data Browser 导航后节是 DataBrowser，查询仍选中，结果区和 `query-tab` 都在，没有 `data-grid`，没有网格行，文档仍是 `NAV`，网格模型 `(0,1)=customer-0`。Schema 后没有 `datazen-preview-card`，结果区还在。`键` 写进文档。SQL 导航后结果区还在。表仍选中时再点 SQL 导航：选中的标签不变，有 `sql-editor-field`，没有 `query-results` / `query-tab` / `data-grid`，`(0,1)` 仍是 `customer-0`。点回查询：结果区在，文档以 `NAV键` 开头，格子仍是 `customer-0`。
- 实际：与预期一致。Data Browser 导航后页眉文本是 `Data Browser` 和 `DARK`，节 DataBrowser，正文仍是查询页，结果区 y=595。`键` 之后文档 `NAV键-- customer lookup...`。纯 SQL 节上编辑器在，结果区没有，网格模型 40×8、`customer-0`，但没有画网格。回到查询后结果区和 `NAV键` 都在，格子未改。
- 判定：通过。页眉可以和正文不一致，见遗留。正文在查询标签选中期间一直是查询布局。

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。点 New query 并 `frame(1.0)` 后，SQL 编辑器在上，`query-results` 在下，文本是 `No results`，没有网格行。打字、回车、退格、多光标和输入法提交都改文档，不拆结果区，预编辑不进文档。再开一张查询不清文档。切到表标签画出那张表并藏起结果区，切回来字还在。关掉查询后结果区消失。标题为 `orders` 的查询不装 orders，已装入的暂存和未暂存粘贴在查询选中期间还在。表标签仍能装网格。产品源码未改。

遗留（不构成拒绝）：

- 所有查询标签共用一个 `SqlEditor`（`lib.rs` 的 `state.sql`，`query_workspace` 305–310 行）。P4 里两张标题都是 `Query` 的标签看到同一份 `FIRST-DOCMORE`。第二张没有换成空文档，所以按本次合同不算失败。每张查询一份文档不在 S4 的完成标准里。
- 关掉最后一张查询时 `sync_grid_to_selected_tab` 对 `None` 什么都不做（147–155 行）。节留在打开查询时设下的 SqlEditor，于是不带结果区的编辑器还在（P6）。结果区已经去掉。节不会自动回到打开查询之前的 Connections。
- 查询标签选中时 `page_body` 先看标签、不看节（209–212 行）。点 Data Browser 或 Schema 会改 `active` 和页眉（P12 页眉是 `Data Browser`），正文仍是查询布局，网格不画。键和输入法因为 `query_selected` 仍进编辑器（468–470、482–484 行）。要看网格必须改选表标签。
- 选中一张打得开的表会 `show_table` → `load_page`（136–144 行）。P9 里未暂存的 `CITY-TEMP` 在选回 customers 时回到 `Hangzhou`，已暂存的 `customer-0-staged` 还在端口里，焦点回到 `(0, 0)`。这是 S3 的重载，不是查询标签清的。查询选中期间这两格都还在。
- 查询标签不结束网格上未提交的编辑会话。P8 回到查询后 `editing` 仍是 true，焦点 `(1, 1)`，但 `grid-editor` 不在查询页上。再选表标签会走 `load_page`，编辑会话会被清掉。
- 未知表 `nope` 打得开标签，`show_table` 失败，节保持 SqlEditor（P10）。网格模型里的 `STILL` 还在，页面上是不带结果区的编辑器，不是网格。真实表标签会把节设成 DataBrowser 并装入该表（P5、P9）。
- 新挂上的查询页、树行、连接列表在进入的那一帧不透明度是 0。`frame(1.0)` 后 `open-query` 的结果区不透明度是 1，点得中。不透明度 0 时 `find` 仍返回中心，但点击会穿过它。
- 编辑器打开时文档是 R1 样本，不是空字符串。S4 要求的空是结果区。样本在插入之后还在。
- `e5f29ca` 带上了上一轮 S3 拒绝报告。产品行为只看 `lib.rs`。
