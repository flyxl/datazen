# TN S3 表标签与网格 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 提交：`63d75d2` `feat(rune): show the selected table tab in the grid (issue-rune S3)`（`63d75d291e73a003fad20966dfcf3203a760c6fb`）
- 设计：`docs/development/rune-migration/plan.md`「工作区壳」任务 S3（表标签里放现有 DataGrid；点开的表和网格是同一张表）
- 源码：`crates/datazen-ui-api/src/lib.rs` 的 `MemoryWorkspace::new` 增加 `orders`（`id`/`name` 一行 `order-0`，约 83–91 行）。`crates/datazen-ui/src/lib.rs` 的 `open_table_tab`（116–119）、`select_tab`（126–139）、`show_table`（141–150）在表标签打开或选中时 `open_table` + `DataGrid::load_page`，并切到 Data Browser。`close_tab`（152–154）只关标签。查询标签不调用 `show_table`
- `git show --stat 63d75d2` 只有上述两个 Rust 文件（+69 / −3）。`packages/frontend`、`packages/web`、`Cargo.toml`、`Cargo.lock` 不在该提交里，工作区也没有这些路径的改动。React 未改
- 探针在 `/tmp/s3_table_probes.rs`，用 `#[path]` 挂进 `datazen-ui` 的测试模块。跑完已删，`crates/datazen-ui/src/lib.rs` 已还原。产品源码未改。不是既有 `the_open_table_tab_is_the_table_in_the_grid` 的重跑。门禁在挂探针之前、干净产品树上跑

## 门禁

环境：`CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RAYON_NUM_THREADS=1 RUSTFLAGS="-C debuginfo=0"`。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 20 passed；`datazen-ui-api` 3 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web`，未启动前端测试 |

编译期有既有 `rune` `private_interfaces` 告警。`cargo check` 里 `open_query_tab` 仍是 `dead_code`（`crates/datazen-ui/src/lib.rs` 122 行），查询标签入口留给 S4，不记为 S3 缺陷。另有既有 `DataGrid::origin_col` / `is_editing` 与 `SqlEditor` 若干方法在非测试构建里的 `dead_code`，与本提交无关。

## 探针

12 条新输入。`cargo test -p datazen-ui --offline --lib s3_p -- --test-threads=1`。10 passed，2 failed。失败的两条是产品合同失败（P9、P10），不是夹具编译错误。

内存工作区在 `connect("local")` 之后会话表名是 `["customers", "orders"]`。连接点击会先把 `customers` 装进网格，但不打开标签。对象树行在下一帧才挂上，`frame(0.0)` 时不透明度为 0。

### P1 零帧点不中 orders，正时间之后网格是 orders

- 目的：验收路径是连上 local、等树行淡入后再点 `tree-table-orders`。网格模型、画出来的格子和表头都是 orders，不是连接时预装的 customers。
- 输入：首帧点 `connection-local`，再 `frame(0.0)` 点 `tree-table-orders` 的中心。然后 `frame(1.0)` 再点一次。
- 预期：零帧时行已挂上但呈现不透明度为 0，点击命中后面的侧栏，不开标签，网格仍是 `customer-0`、40×8。淡入后点击命中 `tree-table-orders`，恰好一个表标签 `orders`，节是 Data Browser。`(0,0)` 为 `1`，`(0,1)` 为 `order-0`，1 行 2 列，没有 `grid-row-1` / `cell-0-2`。活着的表头只有 `id`、`name`。
- 实际：零帧不透明度正好 0，命中 `datazen-sidebar/#5`，标签数 0，网格仍是 customers。1 秒后不透明度 1，命中 `tree-table-orders`。模型与状态栏文本 `1×2 · window at column 0 · …` 一致，格子文本 `order-0`。切换当帧 `head-2` 的 `city` 还在，但是 `exiting=true`（上一张表的退出淡出）；再 `frame(1.0)` 后该节点消失。活表头没有第三列。
- 判定：通过。零帧点不中是 Rune 默认进入淡入，与 S2 记录的一样。退出中的旧表头不是另一张表的数据。

### P2 接着打开 customers，网格换成 customers

- 目的：两张表的行列不能串。customers 不是第二份 orders。
- 输入：P1 的正时间路径打开 orders 之后，点已经静止的 `tree-table-customers`。
- 预期：新的表标签，标题 `customers`，id 与 orders 不同。40 行 8 列。`(0,0)=0`，`(0,1)=customer-0`，`(0,2)=Hangzhou`，`(0,7)=CN`，`(1,1)=customer-1`。画出的格子和 `city`/`country` 表头与此一致。orders 标签还在。
- 实际：与预期一致。状态栏 `40×8`。两个标签 `(1, Table, orders)`、`(2, Table, customers)`，选中 2。
- 判定：通过。

### P3 再选 orders 标签，重新装入 orders

- 目的：选中已打开的 orders 标签必须重新装入 orders，不能留着 customers。来回切几次也不能串列。
- 输入：orders、customers 两个表标签都在，网格当前是 customers。`frame(1.0)` 后点 `tab-{orders}`。再用 `select_tab` 在两张表之间来回切，最后停在 orders。
- 预期：命中 orders 标签。选中 id 是 orders。`(0,1)=order-0`，不是 `customer-0`。1×2。`(0,2)` 落到网格占位 `r0c2`（orders 没有第三列）。画出的格子是 `order-0`。活表头没有 `city`。没有 `grid-row-1`。来回之后仍是 orders 的 1×2。节保持 Data Browser。
- 实际：标签不透明度 1，命中 `tab-strip/tab-1` 里的文本子节点，路径含 `tab-1`。模型、绘制、来回切换都与预期一致。
- 判定：通过。

### P4 查询标签不替换网格

- 目的：打开或选中查询标签不得装入另一张表，也不得清掉当前网格（含未提交的编辑会话）。之后再选表标签仍按表装入。
- 输入：打开 orders。在 `(0,1)` 粘贴 `PASTED` 并 `begin_edit` 再插入 `-WIP`（不提交）。`open_query_tab("draft")`，等标签淡入后点查询标签。再从树打开 customers。再选中查询标签。最后选中 orders。
- 预期：查询标签种类是 Query。打开和点击查询标签之后，格子仍是 `PASTED`，仍在编辑，编辑器文本仍是 `PASTED-WIP`，仍是 1×2，节仍是 Data Browser。打开 customers 后网格变为 `customer-0` 且编辑会话结束。再选查询标签时网格停在 customers，不退回 orders，也不清空。再选 orders 时装入 `order-0`（粘贴没有进端口，所以不会留下 `PASTED`）。
- 实际：与预期一致。查询标签选中时编辑器缓冲区仍是 `PASTED-WIP`。选回 orders 后是 `order-0`、1×2。
- 判定：通过。

### P5 未知表名打开标签，错误路径不清网格

- 目的：读 `show_table`。失败时必须返回 false，不能把网格换成错误句或空页。`open_table_tab` 对未知名字仍要按 S1 打开标签。
- 输入：未连接时 `show_table("orders")` 和 `open_table_tab("orders")`。连接并打开 customers 后，在 `(0,1)` 粘贴 `KEEP`。`show_table("no-such")`。空白标题 `""`、`"   "`、`"\t\n"`。`open_table_tab("no-such")`。再选中该未知标签；选中 customers（会重载）；在 `(0,1)` 再粘贴 `KEEP2` 后再选未知标签。
- 预期：未连接时 `show_table` 为 false，不自己开标签；网格保持合成页 `r0c1`、20000×200，节仍是 Connections。`open_table_tab("orders")` 仍得到一个表标签，网格不被错误替换。已连接时未知名字和空白标题都不改 `KEEP`、行数、列数和节。界面文本里没有 `unknown` / `not connected` / `error`。再选未知标签仍留下 `KEEP2`。customers 与 orders 的端口行没有被这次失败写坏。选中真实的 customers 会重载，未暂存的粘贴消失，这是 `load_page` 清编辑，不是错误路径在清。
- 实际：与预期一致。`show_table` 的 `Err` 分支（`lib.rs` 148 行）不调用 `load_page`。未知标签选中后格子仍是粘贴值。端口里 orders 的名字仍是 `order-0`，customers 仍是 `customer-0`。
- 判定：通过。错误路径没有把网格擦成错误。另有一个连接顺序问题，见遗留，不在这条「失败不得擦除」的判定里。

### P6 已提交的暂存，切走再切回还在，两张表不串

- 目的：`commit_grid_edit` 写进端口的值，在离开这张表再回来之后必须还在。orders 的暂存不能出现在 customers。
- 输入：打开 orders。`(0,1)` 编辑并 `commit_grid_edit` 成 `order-0-staged`，`(0,0)` 同样提交成 `1-id`。打开 customers，确认仍是 `customer-0` / `0`，再把名字提交成 `customer-0-staged`。选 orders 标签，再选 customers 标签。最后 `open_table` 读两张表的端口行。
- 预期：切回 orders 时网格和绘制都是 `1-id`、`order-0-staged`，1×2。切回 customers 时是 `customer-0-staged`，id 仍是 `0`，城市仍是 `Hangzhou`，40×8。端口两边一致，名字互不相等。
- 实际：与预期一致。绘制文本是 `order-0-staged`。切回 orders 时 `editor.text` 还留着上一轮的 `customer-0-staged`，但 `is_editing` 为 false，格子显示的是 orders 的暂存值（缓冲区本身见 P7）。
- 判定：通过。已提交暂存没有丢。

### P7 未提交编辑和未暂存的格子，重载时丢掉

- 目的：记下 `load_page` 清编辑之后，哪一层还在。丢掉未提交的编辑会话是允许的。丢掉已提交暂存才是缺陷（P6 已排除）。
- 输入：在 customers 的 `(0,1)` `begin_edit` 并插入 `-WIP`，不提交，改去打开 orders。再选回 customers。然后只 `commit_edit`（不走 `commit_grid_edit`）把城市改成 `Hangzhou-local`，并在名字上 `paste_tsv("PASTE-ONLY")`。读端口确认没有暂存。选 orders 再选 customers。
- 预期：离开 customers 后编辑会话结束，orders 的 `(0,1)` 是 `order-0`，格子里没有 `-WIP`。`commit_grid_edit` 在非编辑状态下不得把残留缓冲区写进 orders。回到 customers 后名字仍是 `customer-0`，城市仍是 `Hangzhou`。端口同样没有 `PASTE-ONLY` 和 `Hangzhou-local`。
- 实际：格子和端口与预期一致。`load_page`（`data_grid.rs` 98–105 行）把 `editing` 设为 false 并 `edits.clear()`，但**不**清空 `editor.text`。切到 orders 后缓冲区仍是 `customer-0-WIP`，界面不显示它（`editing` 为 false）。此时调用 `commit_grid_edit`，orders 的名字仍是 `order-0`。只进网格编辑表、没有 `stage_cell` 的 `commit_edit` 和粘贴，在来回选标签之后都没了。再打开 customers 时缓冲区残留 `Hangzhou-local`，同样不显示。
- 判定：通过。未提交编辑会话和未暂存的网格编辑会在重载时丢掉，已记录。这是 `load_page` 清 `edits` 的结果，按合同允许。已提交暂存不走这条路径（P6）。

### P8 S1 的选中和关闭还在；关掉未选中的表不清网格

- 目的：S3 不能把 S1 的选中、关闭、空白标题和 id 不复用改坏。关掉一个没被选中的表标签时，选中不动，网格也不该被换成另一张表。
- 输入：树打开 orders、customers，再 `open_query_tab("scratch")`。对 0 和 99 做 select/close。关掉未选中的 customers。点 orders 标签。关掉未选中的查询标签。再 `open_table_tab("customers")`，并再试空白标题。
- 预期：未知 select/close 为 false，选中停在查询，网格仍是打开查询之前的 customers。关掉 customers 后选中仍是查询，customers 消失，网格仍是 `customer-0`、40 行。点 orders 后网格变为 `order-0`。关掉查询后选中和网格都停在 orders。新 customers 的 id 不复用 1、2、3。空白标题返回 `None`，不改选中，网格仍是刚打开的 customers。
- 实际：与预期一致。关掉未选中 customers 后标签是 `(1, Table, orders)`、`(3, Query, scratch)`，选中 3，格子 `customer-0`。
- 判定：通过。

### P9 关掉当前表标签后，网格必须变成新选中的那张表

- 目的：关掉选中的 customers 后，S1 会把选中交给左边的 orders。此时网格必须是 orders，不能继续显示已经关掉的 customers。
- 输入：树打开 orders（id 1）再打开 customers（id 2），网格为 `customer-0`。`frame(1.0)` 后点 `tab-close-2`。然后再对已经选中的 orders 调用一次 `select_tab`，看补点能不能修好。
- 预期：关闭键可点。选中变为 `(1, Table, orders)`，customers 不在集合里。`(0,1)=order-0`，1 行 2 列。
- 实际：关闭键不透明度 1，命中 `tab-strip/tab-close-2`。选中确实变成 orders，customers 已关闭（S1 对）。网格仍是 `customer-0`，40 行 8 列。随后 `select_tab(1)` 返回 true，网格变成 `order-0`，1×2。也就是说装表的逻辑在 `select_tab` 里，不在 `close_tab` 里；用户不补点一次的话，高亮的是 orders，数据是 customers。
- 判定：失败。见缺陷 1。

### P10 打开时用未 trim 的标题查表，标签上存的是 trim 之后的名字

- 目的：S1 的 `Tabs::open` 会 trim。S3 再用这个标题查表时，打开和再次选中必须用同一张表。带两端空白、trim 之后正好是 `orders` 的标题，不能让标签写着 orders、网格还停在 customers。
- 输入：已连接。先 `open_table_tab("orders")`、`open_table_tab("customers")` 作对照。再 `open_table_tab(" orders ")`。记下标题和格子，然后 `select_tab` 这个新 id。
- 预期：对照两次分别装入 `order-0`（1×2）和 `customer-0`（40×8）。第三次标签标题是 `orders`（S1 trim），id 不复用，网格 `(0,1)=order-0`，1 行 2 列。customers 标签还在。
- 实际：对照两次正确。`" orders "` 得到 id 3，标题 `"orders"`，三个标签里有两个标题为 orders。网格仍是 `customer-0`，40×8。`select_tab(3)` 之后才变成 `order-0`，1 行。对象树上的 `orders` / `customers` 没有两端空白，所以树点击本身不会走出这条输入；坏的是 `open_table_tab` 和 `select_tab` 用的不是同一个字符串。
- 判定：失败。见缺陷 2。精确表名的打开路径是好的。

### P11 从别的节点树，以及从表标签离开 SQL 页

- 目的：树在侧栏。在 Settings 上点 orders 必须把 Data Browser 的网格换成 orders，而不是留在 Settings 或留着 customers。已经停在 SQL Editor 时再选这个表标签，要回到网格并且仍是 orders。
- 输入：连接并等树行静止。点 Settings，再点 `tree-table-orders`。点 SQL Editor，再点该 orders 标签。从树打开 customers，点 Schema，再 `select_tab` orders。
- 预期：Settings 上的点击把节改成 Data Browser，格子 `order-0`，1×2，一个表标签。切到 SQL Editor 时模型里的 orders 还在。再选标签回到 Data Browser，仍是 orders。Schema 上选 orders 标签同样回到 Data Browser 的 orders，不是 customers。
- 实际：与预期一致。Settings 上的点击命中 `tree-table-orders`。
- 判定：通过。这是相对 S2 的行为变化：S2 时点树不改节、不改网格；S3 的 `show_table` 成功时会切到 Data Browser 并装入该表。按 S3 的完成标准，这是对的。

### P12 两个 orders 标签共用一张已暂存的表；关光标签后网格不变成错误

- 目的：同名表标签不去重。暂存写在端口的那一张 orders 上，选中哪一个 orders 标签都该看到同一个新值，customers 不被写到。全部关掉之后选中为空，网格不得被换成错误文本。
- 输入：点两次 `tree-table-orders`。在后一个标签上把名字提交成 `order-0-shared`。打开 customers。选第一个 orders，再选第二个。读端口。然后把还开着的标签按选中依次关完。
- 预期：两个不同 id，都是 Table `orders`，网格先是 `order-0`。customers 是 `customer-0`、40 行。两个 orders 标签都显示 `order-0-shared`，1×2。端口 orders 是这个新值，customers 仍是 `customer-0`，orders 列名仍是 `id`、`name`。关光之后选中 `None`，集合空。格子仍是刚才那张真实表的值，不是错误句。
- 实际：与预期一致。关到只剩一个 orders 时网格是 `order-0-shared`。全部关掉后选中 `None`，网格仍是 `order-0-shared`、1×2，没有换成别的表，也没有错误文本。
- 判定：通过。关光最后一个标签不会自动清空网格，见遗留。两个 orders 标签之间的暂存是共享的，因为端口里只有一张 orders。

## 缺陷

### 缺陷 1 关掉选中的表标签后，网格仍是被关掉的那张表

- 文件：`crates/datazen-ui/src/lib.rs` `close_tab` 152–154 行。它只调用 `Tabs::close`。装表在 `select_tab` 135–136 行和 `open_table_tab` 118 行，关闭按钮（约 252–254 行）不走 `select_tab`。
- 输入：连接 local，`frame(1.0)`，点 `tree-table-orders`，再点 `tree-table-customers`，`frame(1.0)`，点 `tab-close-2`（customers 的关闭键，不透明度 1，命中该键）。
- 期望：S1 把选中交给 orders（id 1）。网格与这个选中的表标签是同一张表：`(0,1)=order-0`，1 行 2 列。
- 实际：选中是 `(1, Table, "orders")`，customers 已不在集合里。网格仍是 `(0,1)=customer-0`，40 行 8 列。对这个已经选中的 id 再调用 `select_tab` 才会装入 `order-0`、1×2。高亮标签和网格在补点之前不是同一张表。

### 缺陷 2 打开表标签用原始标题查表，标签上保存的是 trim 后的标题

- 文件：`crates/datazen-ui/src/lib.rs` `open_table_tab` 116–119 行把调用者的原始字符串交给 `show_table`。`crates/datazen-ui-api/src/tabs.rs` `Tabs::open` 44 行先 `trim` 再保存标题。再次选中走 `select_tab` 130–136 行，用的是保存后的标题。`MemoryWorkspace::open_table`（`datazen-ui-api` `lib.rs` 118–126 行）按全字匹配，不 trim。
- 输入：已连接，网格已经是 customers（`open_table_tab("customers")` 成功，`(0,1)=customer-0`，40×8）。再 `open_table_tab(" orders ")`。
- 期望：标签标题按 S1 是 `orders`，并且因为它就是表 `orders`，网格应为 `(0,1)=order-0`，1 行 2 列。精确的 `open_table_tab("orders")` 已经证明这张表能装入。
- 实际：id 3，标题 `"orders"`，选中该标签。`show_table(" orders ")` 查不到表，走 148 行的 `Err`，网格保持 `customer-0`、40×8，没有错误文本。随后 `select_tab(3)` 用 trim 后的 `"orders"` 装入 `order-0`、1 行。打开和再次选中结果不同。树点击传的是会话里的原名，内存库的 `orders` / `customers` 没有两端空白，所以这条不是树的默认点击；它是 `open_table_tab` 自己的合同缺口。`"no-such"` 这种 trim 之后仍然未知的名字不会误装表，P5 已覆盖，那种失败不清网格是对的。

## 结论与遗留

**拒绝。**

点树打开 orders / customers、再点 orders 标签、查询标签不改网格、未知表名不清网格、已提交暂存来回都在、未暂存编辑会丢、S1 的选中和关闭规则，这些都按合同成立。GR1、GR2 通过。React 未改。

不能放行，是因为选中的表标签和网格会不是同一张表：

1. 关掉当前表标签后，选中落到另一张表，网格仍显示被关掉的表（缺陷 1）。这是树点击就能走到的路径。
2. 带两端空白的表名会打开一个标题已经 trim 成真实表名的标签，但第一次打开不装入这张表（缺陷 2）。

无其他需要改产品源码的动作。本报告没有改 `progress.md`，也没有把任务标成完成。

遗留（不单独构成上面两条以外的拒绝理由）：

- 新出现的树行在 `frame(0.0)` 时不透明度为 0，点击命中侧栏。约 0.2 秒的默认进入淡入之后才能点。既有单测和本报告的通过路径都使用正的 `frame`。
- 从表 A 切到列更少的表 B 时，多出来的表头会带 `exiting=true` 再淡出。当帧活着的表头和格子已经是 B。1 秒后旧表头节点消失。
- `load_page` 不清除 `editor.text`。未提交的 `-WIP` 不会进格子，也不会被非编辑状态下的 `commit_grid_edit` 写进端口，但缓冲区里还看得到旧字符串，直到下一次 `begin_edit`。
- 粘贴和只调用 `DataGrid::commit_edit` 的修改只活在 `edits` 里。换表或重载会清掉。要留下必须走 `commit_grid_edit` → `stage_cell`。
- 未连接时 `open_table_tab("orders")` 会留下一个选中的 orders 标签，网格保持合成页（这是失败不清网格，正确）。随后点击 `connection-local`，`open_connection`（`lib.rs` 106–114 行）总是装入 customers，选中仍是 orders。网格变成 `customer-0`、40×8，和选中的 orders 标签不一致。对象树在没有会话时没有行，默认点击顺序是先连接再点表，不会先走出这条。一旦标签能在连接之前存在，连接就不会按选中的表标签装表。
- 关掉最后一个标签后选中为 `None`，网格留着最后一次装入的页（P12 里是 `order-0-shared`）。没有选中的表标签在声称另一张表，所以不记成缺陷 1。清空是 S5 的断开，不是这次。
- `open_query_tab` 仍没有产品里的调用点，留给 S4。查询标签被选中时，Data Browser 上继续画上一张表的网格，这是「查询标签不替换网格」的结果，不是一张查询结果页。
