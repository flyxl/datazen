# TN S3 表标签与网格 复验报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 复验提交：`4fe37e5` `fix(rune): sync the grid when a table tab closes or its title is trimmed (issue-rune S3)`（`4fe37e578ac7c64ffef4972ae269e223fe97f0d7`）
- 上一轮：`docs/development/rune-migration/testing/S3-table-tab.md` 拒绝。缺陷 1：关掉选中的 customers 后选中落到 orders，网格仍是 customers。缺陷 2：`open_table_tab(" orders ")` 标签标题是 trim 后的 `orders`，但 `show_table` 用了未 trim 的字符串，网格停在 customers
- 本提交只改 `crates/datazen-ui/src/lib.rs`（+44 / −10）。`git show --stat 4fe37e5` 没有 `packages/frontend`、`packages/web`、`Cargo.toml`、`Cargo.lock`。React 未改
- 修复：`open_table_tab`（116–120 行）和 `select_tab`（126–132 行）改为 `sync_grid_to_selected_tab`（145–154 行），用标签上已经 trim 过的标题调用 `show_table`（134–143 行）。`close_tab`（156–165 行）只在选中 id 变化时同步。选中不是表标签（查询，或已经没有选中）时 `sync_grid_to_selected_tab` 不调用 `show_table`。`show_table` 失败仍不 `load_page`
- 探针在 `/tmp/s3_table_probes_r2.rs`，用 `#[path]` 挂进 `datazen-ui` 的测试模块。12 条，不是既有单测 `closing_the_selected_table_tab_follows_the_remaining_table` 和 `a_padded_table_title_loads_the_trimmed_table` 的拷贝。P01、P02 按上一轮两条失败输入重跑，并多查了关闭后的绘制、选中来回和关掉这条 padded 标签。跑完已删探针，`crates/datazen-ui/src/lib.rs` 已 `git checkout` 还原（sha256 与跑探针之前相同）。没有改产品源码，没有改 `progress.md`，没有改上一轮报告，没有把任务标完成
- 门禁在挂探针之前、干净产品树上跑

## 门禁

环境：`source "$HOME/.cargo/env"`，`CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RAYON_NUM_THREADS=1 RUSTFLAGS="-C debuginfo=0"`。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 22 passed（含本提交新增的两条单测）；`datazen-ui-api` 3 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

编译期仍是既有 `rune` `private_interfaces` 告警。`cargo check` 里 `open_query_tab` 仍是 `dead_code`（`crates/datazen-ui/src/lib.rs` 122 行），留给 S4，不记为 S3 缺陷。非测试构建里 `DataGrid::origin_col` / `is_editing` 和 `SqlEditor` 若干方法的 `dead_code` 与本提交无关。

## 探针

`cargo test -p datazen-ui --offline --lib s3r2_p -- --test-threads=1`。12 passed，0 failed。下面的实际值来自这次运行的模型、命中路径和状态栏。没有产品合同失败。

内存工作区在 `connect("local")` 之后表名是 `customers`、`orders`。连接点击会把 `customers` 装进网格，但不打开标签。`frame(1.0)` 后树行和关闭键不透明度是 1。

### P1 关掉选中的 customers，网格变成 orders

- 目的：上一轮缺陷 1 的原输入。树打开 orders 再打开 customers，点 customers 的关闭键。选中落到 orders 时网格必须是 orders。
- 输入：`frame(0.0)` 点 `connection-local`，`frame(1.0)` 点 `tree-table-orders`，再点 `tree-table-customers`，`frame(1.0)` 点 `tab-close-2`。
- 预期：关闭键不透明度大于 0.9 且点得中。选中变为 `(1, Table, orders)`，customers 不在集合里。`(0,0)=1`，`(0,1)=order-0`，1 行 2 列，没有第三列名。节是 Data Browser。画出的 `cell-0-1` 是 `order-0`，状态栏以 `1×2` 开头。再 `frame(1.0)` 后活着的表头只有 `head-0`、`head-1`。
- 实际：树行和 `tab-close-2` 不透明度都是 1。关闭键命中路径 `datazen-page/tab-strip/tab-close-2/#0`（关闭行里的文本子节点，路径含 `tab-close-2`）。关闭后选中 `(1, Table, orders)`，只剩这一个标签。模型 `c0=1`、`c1=order-0`，1×2，列名只有 `id`、`name`。状态栏 `1×2 · window at column 0 · selection r0c0..r0c0 · 0 edits`。绘制文本 `order-0`。1 秒后活表头只有 `head-0`、`head-1`。
- 判定：通过。上一轮缺陷 1 不再复现。

### P2 带空白的 `" orders "` 打开时就装入 orders

- 目的：上一轮缺陷 2 的原输入。打开时必须用存进标签的 trim 标题查表，不能等下一次 `select_tab`。然后再选回 customers、选回这条标签、关掉它，网格都跟当前表标签走。
- 输入：`workspace.connect("local")`。`open_table_tab("customers")`，确认 40×8、`customer-0`。`open_table_tab(" orders ")`。在还没补选的时候读标题和格子。然后 `select_tab` customers，再 `select_tab` 这条新标签，最后 `close_tab` 它。
- 预期：新标签标题是 `orders` 不是 `" orders "`，id 不复用。打开当时网格就是 `(0,0)=1`、`(0,1)=order-0`，1×2，节是 Data Browser，customers 标签还在。选回 customers 是 40×8、`customer-0`。再选回这条标签仍是 orders 的 1×2。关掉它之后选中 customers，网格回到 `customer-0`、40×8。端口里 orders 的名字仍是 `order-0`。
- 实际：id 1 标题 `customers`，id 2 标题 `orders`。padded 打开后、任何补选之前，模型已经是 `c0=1`、`c1=order-0`，1×2。来回选中和关闭都与预期一致。端口 orders 名字 `order-0`。
- 判定：通过。上一轮缺陷 2 不再复现。打开和再次选中用的是同一个 trim 后的标题。

### P3 关掉没选中的表，不重载，已提交的格子还在

- 目的：`close_tab` 只在选中 id 变化时同步。关掉旁边的 orders 时，customers 上已经 `commit_grid_edit` 的值和没进端口的粘贴都不能被 `load_page` 清掉，节也不能被 `show_table` 改回 Data Browser。
- 输入：连接后 `open_table_tab("orders")` 再 `open_table_tab("customers")`。在 `(0,1)` `begin_edit`、插入 `-staged`、`commit_grid_edit`。`(0,2)` 粘贴 `CITY-LOCAL`（不暂存）。焦点改到 `(4,3)`。节改成 Schema。`close_tab(orders)`。
- 预期：选中仍是 customers，orders 标签消失。格子仍是 `customer-0-staged` 和 `CITY-LOCAL`，焦点仍是 `(4,3)`，40×8，列名 `city` 还在，节仍是 Schema。端口 customers 的名字是 `customer-0-staged`，城市仍是 `Hangzhou`。端口 orders 的名字仍是 `order-0`。
- 实际：与预期一致。关闭前后焦点、节、两格文本都没变。
- 判定：通过。未选中标签的关闭没有重载，也没有擦掉已提交暂存或未暂存粘贴。

### P4 关掉最后一张表标签，网格保持原样

- 目的：选中从有变成 `None` 时会走同步，但没有表标签，不得调用 `show_table`，网格（含未暂存粘贴和已提交暂存）留在原地。
- 输入：连接后只打开 orders。名字 `commit_grid_edit` 成 `order-0-last`，`(0,0)` 粘贴 `ID-LOCAL`，焦点 `(0,1)`，节改成 Settings。`close_tab` 这唯一的标签。然后只把节设回 Data Browser（不调用 `show_table`）再画一帧，读状态栏和格子。
- 预期：集合空，选中 `None`。模型仍是 `ID-LOCAL`、`order-0-last`，焦点 `(0,1)`，1×2，列名 `name` 还在，节在关掉的当时仍是 Settings。端口 orders 名字是 `order-0-last`，id 仍是 `1`。回到 Data Browser 后状态栏以 `1×2` 开头，画出的两格是这两个值，没有 `error`。
- 实际：关掉之后选中 `None`，节 Settings，`c0=ID-LOCAL`，`c1=order-0-last`，焦点 `(0,1)`，1×2。端口与此一致，id 仍是 `1`。切回 Data Browser 后状态栏 `1×2 · window at column 0 · selection r0c1..r0c1 · 2 edits`，绘制 `ID-LOCAL` 和 `order-0-last`。
- 判定：通过。最后一张表关掉以后网格没有被换成别的表，也没有被清空。Settings 上本来就不挂网格控件，所以状态栏要等节回到 Data Browser 才画得出来；模型在关掉的那一帧已经保持原值。

### P5 关掉表之后新选中的是查询标签，不调用 show_table

- 目的：查询标签标题即使正好是 `orders`，也不能按标题去装 orders。关闭使选中落到这条查询时，网格、焦点、节都停住。
- 输入：连接。`open_query_tab("orders")`，此时还没有表页。再 `open_table_tab("customers")`。`(0,1)` 粘贴 `KEEP-Q`，焦点 `(3,2)`，节改成 Settings。`close_tab` customers。标签顺序是查询在左、表在右，关掉表之后 S1 选中左边的查询。
- 预期：查询打开时种类是 Query、标题 `orders`，网格仍是合成页 `r0c1`。关掉 customers 后选中是这条查询，不是表。格子仍是 `KEEP-Q`，`(0,2)` 仍是 `Hangzhou`，焦点 `(3,2)`，40×8，列名 `country` 还在，节仍是 Settings，不是 `order-0`、也不是 1×2。端口 customers 名字仍是 `customer-0`，orders 仍是 `order-0`。
- 实际：查询打开时 `c1=r0c1`。关闭后选中 `(1, Query, orders)`，只剩这一条。模型仍是 `KEEP-Q`、`Hangzhou`，焦点 `(3,2)`，40×8，节 Settings。两个端口都没被写成别的值。
- 判定：通过。新选中是查询时没有装表。同名标题没有把网格换成 orders。

### P6 选中仍在 customers 和 orders 之间切换

- 目的：修复不能把 `select_tab` 的换表弄丢。未知 id 不得改选中或网格。点标签和直接 `select_tab` 同一条路。
- 输入：连接。`open_table_tab("customers")` 再 `open_table_tab("orders")`。`select_tab` 到 customers，再到 orders，再各一次。`select_tab(0)` 和 `select_tab(99)`。`frame(1.0)` 后点 `tab-1`，再 `frame(1.0)` 点 `tab-2`。
- 预期：打开 orders 后是 `order-0`、1×2。选 customers 是 `(0,0)=0`、`(0,1)=customer-0`、`(0,2)=Hangzhou`、`(0,7)=CN`、`(1,1)=customer-1`，40×8，节 Data Browser。再选 orders 是 `order-0`，`(0,2)` 为占位 `r0c2`，没有第三列名，1×2。来回之后仍是 orders。未知 select 返回 false，选中和网格不动。点 `tab-1` 命中该标签并装回 customers；点 `tab-2` 装回 orders。
- 实际：与预期一致。两次点击不透明度都是 1，命中路径分别含 `tab-1/#0` 和 `tab-2/#0`。点完 customers 是 40×8、`customer-0`；点完 orders 是 1×2、`order-0`。
- 判定：通过。

### P7 未知表名打开标签，不清掉已经装入的网格

- 目的：`Tabs::open` 允许 `nope` 和 `" nope "`。同步会去 `show_table`，失败必须留下当前页，包括已提交暂存、未暂存粘贴和焦点。空白标题按 S1 返回 `None`。再选中未知标签仍不清；选中真实 customers 会重载，这是允许的。
- 输入：未连接时 `open_table_tab("nope")`。再 `connect` 并 `open_connection("local")`（只记录，不作为本条失败条件）。然后 `open_table_tab("customers")`，名字提交成 `customer-0-kept`，`(0,2)` 粘贴 `CITY-KEEP`，焦点 `(2,4)`。`open_table_tab("nope")`，再 `open_table_tab(" nope ")`。空白 `""`、`"   "`、`"\t\n"`。`select_tab` 前一个 nope。`select_tab` customers。再在城市粘贴 `CITY-KEEP2`，`select_tab` 后一个 nope。扫界面文本。
- 预期：未连接时 nope 标签打开，标题 `nope`，网格保持 `r0c1`、20000×200，节仍是 Connections。已装入 customers 之后，两个 nope 都打开，标题是 trim 后的 `nope`，id 不复用。`customer-0-kept`、`CITY-KEEP`、焦点 `(2,4)`、40×8 都还在，节仍是 Data Browser。空白三次都是 `None`，选中不动，标签数不变。再选 nope 仍留下粘贴和焦点。选中 customers 会重载：名字仍是已暂存的 `customer-0-kept`，城市回到 `Hangzhou`，焦点 `(0,0)`。再选 nope 留下 `CITY-KEEP2`。界面文本里没有 `unknown table` / `not connected`。
- 实际：与预期一致。未连接时 `c1=r0c1`，20000×200，节 Connections，标签 `(1, Table, nope)`。四个标签里 customers 是 id 2，两个 nope 是 id 3 和 4。空白之后仍选中 id 4，`CITY-KEEP` 还在。选中 customers 后城市变回 `Hangzhou`、焦点 `(0,0)`、名字仍是 `customer-0-kept`。再选 id 4 后城市是 `CITY-KEEP2`，40×8。文本里没有错误句。
- 判定：通过。未知名字没有擦网格。`Tabs::open` 允许这些非空标题。

### P8 关掉选中的第一张表，网格跟着新的第一张

- 目的：S1 在关掉最左边的选中标签时，选中变成新的第一张，而不是左边（左边没有）。这条分支也要装入新选中的表。再关一个已经不存在的 id 不得改网格。
- 输入：连接。`open_table_tab("customers")` 再 `open_table_tab("orders")`。`select_tab` customers，确认它在下标 0。`close_tab` customers。再 `close_tab` 同一个 id。
- 预期：关闭前网格是 `customer-0`、40×8。关闭后只剩 orders，选中就是它。`(0,0)=1`，`(0,1)=order-0`，1×2，节 Data Browser。第二次 close 返回 false，选中和网格不变。
- 实际：与预期一致。关闭后标签只剩 `(2, Table, orders)`，`c0=1`、`c1=order-0`，1×2。重复关闭之后仍是这张。
- 判定：通过。上一轮只盖住了关掉右边那张；关掉第一张也会跟上。

### P9 别的空白形式，打开和来回选中都用 trim 后的表

- 目的：不是单测那一个 `" orders "`。`" customers "`、`"\torders\n"`、`"  orders  "` 都要在打开时装入 trim 后的表，选中来回也不能串。
- 输入：连接。`open_table_tab(" customers ")`，再 `open_table_tab("\torders\n")`。`select_tab` 回 customers，再回 orders。再 `open_table_tab("  orders  ")`。
- 预期：第一条标题 `customers`，`(0,1)=customer-0`，`(0,7)=CN`，40×8。第二条标题 `orders`，id 不同，`order-0`，1×2。选回 customers 是 40×8。选回 orders 是 1×2。第三条标题仍是 `orders`，又一个新 id，网格 `order-0`，集合里两个标题为 orders 的标签。
- 实际：与预期一致。id 依次是 1、2、3。第三条打开后 `c1=order-0`，1×2，orders 标题出现两次。
- 判定：通过。

### P10 点未选中标签的关闭键，已提交编辑还在

- 目的：关闭键走 `close_tab`，不是绕过它直接改 `Tabs`。用户点掉旁边的 orders 时，customers 上的已提交暂存和未暂存粘贴都还在，节不变。
- 输入：点 `connection-local`，`frame(1.0)`，点 `tree-table-orders`，点 `tree-table-customers`。名字提交成 `customer-0-ui`，城市粘贴 `CITY-UI`，焦点 `(6,1)`，节改成 Schema。`frame(1.0)` 点 `tab-close-1`。
- 预期：orders 是 id 1，customers 是 id 2 且被选中。关闭键不透明度大于 0.9。点完之后 orders 消失，选中仍是 customers。格子仍是 `customer-0-ui` 和 `CITY-UI`，焦点 `(6,1)`，40×8，节仍是 Schema。端口名字是 `customer-0-ui`，城市仍是 `Hangzhou`。
- 实际：树点击命中对应的 `tree-table-*`。关闭前焦点 `(6,1)`、节 Schema、两格已是暂存和粘贴。`tab-close-1` 不透明度 1，命中路径含 `tab-close-1/#0`。关闭后只剩 `(2, Table, customers)`，`c1=customer-0-ui`，`c2=CITY-UI`，焦点 `(6,1)`，40×8，节 Schema。端口城市仍是 `Hangzhou`。
- 判定：通过。

### P11 查询标签、空白标题、未知 id 都不替换网格

- 目的：查询标题与表同名（`customers`）时，打开和再次选中都不装表，连正在进行的编辑会话也留着。空白表标题、空白查询标题、未知 select/close 不动集合。关掉没选中的查询也不重载。
- 输入：连接并打开 customers。`(0,1)` 粘贴 `KEEP`，焦点 `(1,3)`，节改成 SQL Editor。`open_query_tab("customers")`。`select_tab` 回表（这一步允许重载）。再粘贴 `KEEP2`，`begin_edit` 并插入 `-WIP`，节改成 Workflows，`select_tab` 查询。`open_table_tab(" ")`、`open_query_tab("  ")`、`select_tab(0)`、`close_tab(0)`、`close_tab(99)`。再选中表，城市粘贴 `CITY3`，节改成 Schema，关掉查询。
- 预期：查询打开后种类 Query、标题 `customers`，`KEEP`、焦点和 SQL Editor 节都还在，40×8。选回表会重载成 `customer-0`、焦点 `(0,0)`、节 Data Browser。再选查询时 `KEEP2` 还在，仍在编辑，编辑器文本是 `KEEP2-WIP`，节仍是 Workflows。空白和未知 id 都失败，选中仍是查询，编辑会话还在。关掉查询后选中是 customers，`CITY3`、焦点 `(0,2)`、Schema、40×8 都不变。
- 实际：与预期一致。查询打开时 `c1=KEEP`、焦点 `(1,3)`、节 SqlEditor。选回表后 `c1=customer-0`、焦点 `(0,0)`。再选查询时 `editing=true`，编辑器 `KEEP2-WIP`，节 Workflows。拒绝的几次调用之后仍是这条查询、仍在编辑。关掉查询后 `c2=CITY3`，焦点 `(0,2)`，节 Schema，40×8。
- 判定：通过。查询标签没有装表。未选中查询的关闭没有重载。

### P12 选中落到未知标签，以及关掉最后一张未知标签，都不清网格

- 目的：两张 orders 共用端口里的暂存。打开 `nope` 不得清掉 orders 上的未暂存粘贴。关掉未选中的 orders 时选中停在 nope，不得因此装表。最后关掉 nope、选中变成 `None` 时，网格仍是刚才那一页。customers 端口不被写到。
- 输入：连接。打开 orders，名字提交成 `order-0-shared`。再打开一张 orders。`select_tab` 在两张 orders 之间来回。`(0,0)` 粘贴 `ID-TEMP`，焦点回到 `(0,1)`。`open_table_tab("nope")`。节改成 Settings。关掉第二张 orders，再关掉第一张，再关掉 nope。读两个端口。
- 预期：第二张 orders 打开时网格已是 `order-0-shared`，1×2，id 不同。来回选中都是这个值。打开 nope 后选中 nope，`ID-TEMP` 和 `order-0-shared` 都在，焦点 `(0,1)`。关掉两张 orders 时选中一直是 nope，节一直是 Settings，两格不变。关掉 nope 后选中 `None`，集合空，格子仍是 `ID-TEMP`、`order-0-shared`，1×2，焦点 `(0,1)`，节 Settings。端口 orders 名字是 `order-0-shared`、id 仍是 `1`。customers 名字仍是 `customer-0`，40 行。
- 实际：与预期一致。第二张 orders 打开后就是 `order-0-shared`。nope 是 id 3。两次关掉 orders 之后仍选中 nope，`ID-TEMP` 还在，节 Settings。最后选中 `None`，模型未变。端口 customers 仍是 `customer-0`、40 行。
- 判定：通过。未知选中和「一个标签都不剩」都没有把网格擦成空页或错误。

## 缺陷

无。上一轮两条在 P1、P2 的原输入上不再出现。本轮要求的其余合同（未选中关闭不重载、最后一张表留下网格、落到查询不装表、选中仍能切换、未知表名不清网格）都成立。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。点关闭键关掉当前 customers 后网格是 orders。`" orders "` 在打开的当时就装入 orders，不用再选一次。查询标签和未知表名都不替换网格。产品源码未改。

遗留（不构成拒绝）：

- 未连接时 `open_table_tab("nope")` 会留下选中的 nope 标签，网格保持合成页（失败不清网格，正确）。随后 `open_connection("local")` 仍总是装入 customers：P7 里选中还是 `(1, Table, nope)`，节变成 Data Browser，网格变成 `customer-0`、40×8。这是 `open_connection`（`lib.rs` 106–114 行）不看当前标签。上一轮已经记过，本提交的说明只覆盖关闭同步和 trim 标题，默认点击仍是先连接再点表。不用这次的两条修复来要求它。
- `load_page` 仍会在选中另一张真实表时清掉未暂存的粘贴和焦点，已暂存的值留在端口里（P6、P7）。关掉未选中标签不会走这条重载（P3、P10）。
- 关掉最后一个标签后网格留着最后一页（P4、P12）。没有选中的表标签在声称另一张表。清空留给 S5。
- `open_query_tab` 仍没有产品里的调用点。查询被选中时 Data Browser 继续画上一张表，这是「查询不替换网格」的结果。
- 新树行要用正的 `frame` 才点得中。本轮 `frame(1.0)` 时 `tree-table-orders` 和关闭键不透明度都是 1。
