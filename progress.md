# P5 file-cap 轨进度台账

> 本文件是并行开发期间的进度台账，**不是交付物**。本轨验收合并时必须删除，不得存活在 `main` 上。

- 分支：`feature/p5-file-cap`
- 基线：`d6423f9d1b6a65d6e3e7a60e54513a7ec0614d67`
- 门禁提交：`06a84cea53f17ea6f0cdc11206e371b3d9e24851`
- 变更范围：仅 `packages/driver-api/src/traits.rs` + 同 crate 新增子模块；`packages/drivers/sqlserver/src/sqlserver.rs` + 同 crate 新增子模块（含其 `mod` 块）
- 未触碰：`src-tauri/**`、`src/**`、`packages/data-transfer/**`、`.env*`、其他驱动 crate

## 1. 结论摘要

| 目标 | 结果 |
| --- | --- |
| `packages/drivers/sqlserver/src/sqlserver.rs` 压到 800 行以内 | **达成**：2643 → **491** |
| `packages/driver-api/src/traits.rs` 压到 800 行以内 | **未达成**：1795 → **1020**，超出 220 行。原因见 §8 |
| 本轨新增的所有文件 ≤ 800 行 | **达成**：11 个新文件全部 ≤ 755 行 |
| 新增守卫测试 | **达成**：2 个，见 §7 |

这是一个**纯重构**轨：不改行为、不改公共 API、不改 trait 语义。变异测试对纯重构基本不适用，验收靠结构化等价证据（§4–§6）。

## 2. 拆分依据（为什么这么切）

两条已经裁定、不得再翻的前提：

1. **trait 的签名不能跨文件拆**——签名和它的 impl 必须待在一起，否则 17 个实现 crate 的可读性和 trait 语义都会坏掉。
2. **可以只搬方法体**，把实现挪进子 `mod`，trait/impl 的签名原地保留。这是本轨唯一使用的拆法。

据此：

- **sqlserver**：被搬的方法变成同名 `pub(crate)` **inherent 方法**，落在子 `mod` 的 `impl SqlServerDriver` 里。trait impl 处保留签名 + 文档注释 + 一行 UFCS 委派 `SqlServerDriver::method(self, ..)`。同名 inherent 方法让**方法体逐字节不变**（不需要把 `self.` 改写成别的），代价是每个被搬方法多一行重复签名（sqlserver 共 15 个）。调用点零改动。
- **driver-api**：`DatabaseDriver` 是 trait，它的默认方法体不能走 inherent 路线（驱动类型是 `driver-api` 的外来类型，impl 不了 inherent 方法）。被搬的默认体变成 `pub(crate) async fn name<D: DatabaseDriver + ?Sized>(driver: &D, ..)`，`self` 改名为 `driver`，调用点仍然 `self.xxx(..)` 动态派发。`?Sized` 必须写，否则 trait object 调用点编译不过。
- **整块公共项**（`execute_standard_sql_command`、`SchemaScope`、`validate_schema_target`、`KeyValueDriver`）用**门面再导出**：搬进私有 `mod`，在 `traits.rs` 里 `pub use <child>::Item;`。`lib.rs` 的 `pub use traits::*;` 因此把它们送到**完全相同的公共路径**，跨 crate 零感知。

职责边界（每个新模块 `//!` 文档都写了自己的理由，不引用任何需求编号）：

| 模块 | 边界 |
| --- | --- |
| `traits/sql_text.rs` | 字面量/标识符加引号/分页方言/`qualified_sql`/restore 切分/UPDATE、DELETE 语句拼装 |
| `traits/streaming.rs` | 流式读取默认体（`get_columns`、`query_stream_with_params`） |
| `traits/command_api.rs` | `execute_command` 默认体 + `execute_standard_sql_command` 自由函数 |
| `traits/schema_target.rs` | `SchemaScope` + `validate_schema_target` + `sql_input_with_target` |
| `traits/key_value.rs` | `KeyValueDriver` trait |
| `traits/structure_defaults_tests.rs` | 原本就独立的 `mod structure_defaults_tests`（`#[cfg(test)]`） |
| `sqlserver/session.rs` | 会话/事务/连接生命周期 |
| `sqlserver/dialect.rs` | SQL 方言与转义 |
| `sqlserver/catalog.rs` | 元数据只读路径（库、表、表结构、批量列映射） |
| `sqlserver/tests.rs` | 原本就独立的 `mod tests`（`#[cfg(test)]`） |
| `sqlserver/file_line_cap.rs` | 行数守卫测试 |

**没有采用的方案及原因**：`include!` 文本切分（破坏 `rustfmt --check`）；把整个 trait 搬进 `traits/database_driver.rs`（只是把超限文件改名）；supertrait 拆分（17 个 crate 的 API 变更）；把文档注释挪进 `include_str!` 的 md 分片（刷指标）；删空行（最好也就 ~850 行，且可读性归零）；把剩下约 40 个平凡默认体全搬（净收益 ≈ 0 甚至为负，还要多出 40 个委派 + 40 个自由函数，纯噪声）。

## 3. 拆分后文件与实测行数

```
     1020  packages/driver-api/src/traits.rs              <-- 仍超限，见 §8
      104  packages/driver-api/src/traits/command_api.rs
       23  packages/driver-api/src/traits/key_value.rs
       64  packages/driver-api/src/traits/schema_target.rs
      161  packages/driver-api/src/traits/sql_text.rs
       61  packages/driver-api/src/traits/streaming.rs
      486  packages/driver-api/src/traits/structure_defaults_tests.rs
      491  packages/drivers/sqlserver/src/sqlserver.rs    <-- 2643 -> 491，达成
      266  packages/drivers/sqlserver/src/sqlserver/catalog.rs
      574  packages/drivers/sqlserver/src/sqlserver/dialect.rs
      109  packages/drivers/sqlserver/src/sqlserver/file_line_cap.rs
      720  packages/drivers/sqlserver/src/sqlserver/session.rs
      755  packages/drivers/sqlserver/src/sqlserver/tests.rs
     4834  total
```

`git diff --stat d6423f9d1 HEAD`：13 files changed, 3386 insertions(+), 2990 deletions(-)。

**11 个新增文件全部 ≤ 755 行**，没有一个大文件是从一个超大文件里原样搬出来的（`tests.rs` 755 行是最紧的一个，仍然在线内）。

## 4. 门禁实测（全部跑在 `06a84cea5`，`CARGO_TARGET_DIR=/tmp/p5fc-target`）

首尾各采样一次 HEAD 与工作区，证明跑门禁期间没人动过树：

```
start: HEAD=d6423f9d1b6a65d6e3e7a60e54513a7ec0614d67  BRANCH=feature/p5-file-cap  git status --porcelain = (空)
end  : HEAD=06a84cea53f17ea6f0cdc11206e371b3d9e24851  BRANCH=feature/p5-file-cap  git status --porcelain = (空)
```

（首尾 HEAD 不同是预期的：首采样在动工前，尾采样在提交后。两次采样期间工作区都干净。）

| 门禁 | 命令 | 退出码 | 结论行 |
| --- | --- | --- | --- |
| codegen | `node scripts/resolve-drivers.mjs --codegen-only --drivers=all` | `CODEGEN_EXIT=0` | 通过 |
| driver-api 测试 | `cargo test -p datazen-driver-api` | `API_TEST_EXIT=0` | lib `329 passed; 0 failed`；另 3 个 target 分别 5 / 8 / `0 passed; 2 ignored` |
| sqlserver 测试 | `cargo test -p datazen-driver-sqlserver` | `SS_TEST_EXIT=0` | lib `123 passed; 0 failed`；10 个集成 target 全部 ok |
| 格式 | `rustfmt --edition 2021 --check` × 13 个自有的叶子文件 | `FMT_CHECK_ALL_EXIT=0` | 通过 |
| clippy | `cargo clippy --workspace --all-targets --keep-going --message-format=json` | `CLIPPY_EXIT=101` | 见 §6。101 是基线既有状态（工作区本来就有 3 个 error 级编译问题），不是回归 |

两个 crate 的测试编译都是 **0 warning**。

## 5. 等价性证据

### 5.1 测试名逐条对照

判定键是 `cargo test -- --list` 的完整测试名（含模块路径），去重排序后 `comm` 比对。**不是**比数量。

**`datazen-driver-api`：完全一致。**

| | 基线 `d6423f9d1` | 门禁 `06a84cea5` |
| --- | --- | --- |
| 测试名总数（去重） | 344 | 344 |
| `comm -23`（只在基线） | 0 条 | — |
| `comm -13`（只在门禁） | — | 0 条 |
| `API_NAMES_DIFF_EXIT` | — | 0 |

**`datazen-driver-sqlserver`：有 2 条差异，是故意加的守卫测试。**

| | 基线 `d6423f9d1` | 门禁 `06a84cea5` |
| --- | --- | --- |
| 测试名总数（去重） | 222 | 224 |
| `comm -23`（被删掉的测试名） | — | **0 条** |
| `comm -13`（新增的测试名） | — | 2 条 |

`SS_NAMES_DIFF_EXIT=1`，逐字差异只有一行：

```
60a61,62
> lib::sqlserver::file_line_cap::database_driver_trait_declaration_does_not_grow
> lib::sqlserver::file_line_cap::split_files_stay_within_the_line_limit
```

**没有任何一个既有测试被删掉或改名。** 单模块测试的完整路径靠"行内 `mod tests` 改成 `mod tests;` + 同名 `tests.rs`"保持不变（一个模块不能拆进两个文件，所以整块搬、不拆散）。

门禁侧按 target 的测试名计数：`lib` 123、`tests/live_write_and_ddl.rs` 22、`tests/live_schema_metadata.rs` 14、`tests/live_admin_and_sync.rs` 14、`tests/live_connection.rs` 11、`tests/sync_adapter_smoke.rs` 10、`tests/live_query_types.rs` 9、`tests/live_streaming.rs` 7、`tests/live_env_file_opt_in.rs` 7、`tests/live_pagination.rs` 5、`tests/live_session_mode_lifecycle.rs` 1、`tests/command_definitions.rs` 1。

### 5.2 方法体等价

- **driver-api**（10 个被搬的默认体）：逐个把"基线 `traits.rs` 里的方法体"和"新模块里的自由函数体"抽出来比 **token 流**（空白全部折叠，并额外归一 `.` 前后的空白）。**ALL_BODIES_TOKEN_IDENTICAL=True**。逐项：

  | 方法 | 基线行数 | 新文件行数 | 判定 |
  | --- | --- | --- | --- |
  | `format_sql_literal` | 32 | 32 | SAME |
  | `quote_ident` | 6 | 6 | SAME |
  | `pagination_syntax` | 9 | 9 | SAME |
  | `qualified_sql` | 5 | 6 | SAME |
  | `split_restore_sql` | 4 | 4 | SAME |
  | `build_update_sql` | 27 | 27 | SAME |
  | `build_delete_sql` | 16 | 16 | SAME |
  | `get_columns` | 5 | 5 | SAME |
  | `query_stream_with_params` | 24 | 24 | SAME |
  | `execute_command` | 13 | 12 | SAME |

  行数不等的两处（`qualified_sql` +1、`execute_command` -1）是 rustfmt 因缩进深度从 4 变 0 而重新折行造成的，token 流完全一致。

- **sqlserver**（43 个同名方法）：把 2643 行的原始 `sqlserver.rs` 里的方法体，和新文件里的 `pub(crate)` inherent 方法体，用**括号深度感知**的抽取器逐个比（避免闭包嵌套造成误配）。**ALL_SHARED_BODIES_IDENTICAL=True**。原始 trait impl 里的 98 个 `fn` 对上自有文件里的 44 个 `pub(crate)`/`pub` inherent `fn`，同名可配 43 个，逐个字节相同。

### 5.3 API 未变

判定键 = `(可见性类别, 种类, 归一化签名)`，可见性类别**从签名原文重新推导**（`pub(crate)` 不算 `pub`），上下文列丢弃（单文件基线抽取对顶层项的 enclosing-trait 标注不可靠）。抽取时父文件和全部子模块**必须在同一次调用里**传进去。

**`datazen-driver-api`**

| | 基线 | 门禁 |
| --- | --- | --- |
| 不同签名字对 | 138 | 150 |
| 其中 `pub` | 2 | 2 |
| 其中 private | 149 | 149 |
| 其中 `pub(crate)` | 0 | 10（就是被搬走的 10 个默认体） |
| **公共 API 集合是否一致** | — | **True** |

公共项就两个，签名逐字未变：

```rust
async fn execute_standard_sql_command<D: DatabaseDriver + ?Sized>(
    driver: &D, handle: &ConnectionHandle, command: &str, input: serde_json::Value,
) -> Result<CommandResult, DriverError>

fn validate_schema_target<D: DatabaseDriver + ?Sized>(
    driver: &D, database: &str, schema: Option<&str>, scope: SchemaScope,
) -> Result<(), DriverError>
```

"移除/变化"的 5 条（`connect`、`execute` ×2、`get_databases`、`test_connection`）**每一条都有同名对应项**，差异只是 stub 测试签名从缩进 4 变成 0 后 rustfmt 折叠成一行（少了行尾逗号），token 相同，不是 API 变更。

公共路径保持的证据（`traits.rs`）：

```rust
mod command_api;  mod key_value;  mod schema_target;  mod sql_text;  mod streaming;
#[cfg(test)] mod structure_defaults_tests;
pub use command_api::execute_standard_sql_command;
pub use key_value::KeyValueDriver;
pub use schema_target::{validate_schema_target, SchemaScope};
```

这四项在原始 `traits.rs` 里都是顶层定义（`execute_standard_sql_command`、`SchemaScope`、`validate_schema_target`、`KeyValueDriver`），现在经 `lib.rs` 的 `pub use traits::*;` 落在**同一个公共路径**上。跨 crate 使用方全部实测存在并编译通过：`validate_schema_target`/`SchemaScope` 被 victoriametrics、elasticsearch、clickhouse、vector、postgres、duckdb、mock_driver、influxdb、mysql、sqlite、mongodb、rqlite、turso、hbase、redis 使用；`execute_standard_sql_command` 被 elasticsearch、clickhouse、vector、redis、mock_driver、sqlserver、hbase、mysql、postgres、mongodb、sqlite、rqlite、turso 使用；`KeyValueDriver` 被 driver-api 的 `factory.rs`/`lib.rs`、redis 的 `types.rs`/`driver/key_value.rs`/`lib.rs`、`src-tauri/src/db/registry.rs`、`packages/drivers/http-support/tests/support/` 使用。

**`datazen-driver-sqlserver`**

| | 基线 | 门禁 |
| --- | --- | --- |
| 不同签名字对 | 117 | 140 |
| 其中 `pub` | 1 | 1（`fn new() -> Self`） |
| 其中 private | 116 | 89 |
| 其中 `pub(crate)` | 0 | 50 |
| **公共 API 集合是否一致** | — | **True** |

"移除/变化"的 35 条**全部可解释**：每一条都是原本的 private 自由函数变成了 `pub(crate)` inherent 方法，参数列表逐字不变（如 `quote_identifier`、`ssl_settings`、`async fn run_routed`、`build_config`、`effective_schema<'a>(&self, ..)`）。新增 58 条 = 50 个 inherent 方法 + rustfmt 重排后的变体 + 2 个守卫测试。

sqlserver 的 `lib.rs`（80 行，未改动）末尾仍是 `datazen_driver_api::register_driver!(&SqlServerFactory);`，`inventory` 注册不受影响。

## 6. clippy 警告集合差

`CLIPPY_EXIT=101` 是**基线既有状态**：工作区本来就有 3 个 error 级问题（`packages/drivers/redis/src/ops/exec.rs`、`packages/drivers/redis/src/ops/mod.rs`、`packages/platform-api/src/error.rs`，E0004 ×1 + `clippy::approx_constant` ×2）。所以**按集合裁定，不按退出码，也不按绝对条数**。

| 指标 | 基线 `d6423f9d1` | 门禁 `06a84cea5` |
| --- | --- | --- |
| warning 记录数 | 1051 | 1052 |
| 不同 lint code 数 | 82 | 82（**集合完全相同**） |
| error 记录数 / code | 3 / {E0004, approx_constant} | 3 / 同（**集合完全相同**） |
| error 文件集合 `comm -23` / `comm -13` | — | 两个方向都为空 |

82 个 lint code 里 **80 个条数完全一致**，只有两个不同：

- `clippy::items_after_test_module`：7 → 6（**减少**，见下）
- `clippy::type_complexity`：8 → 10（**+2，我引入的，如实记录**）

主 span 文件集合差（`comm -13` 只有基线没有门禁 = 新增文件，反之 = 消失文件）：

```
comm -13 (仅门禁):  packages/drivers/sqlserver/src/sqlserver/catalog.rs
comm -23 (仅基线):  packages/driver-api/src/traits.rs
```

**逐条解释（不做"无变化"的粉饰）：**

1. **`packages/driver-api/src/traits.rs` 从警告文件集合里消失。** 基线上它恰好只有 **1** 条主 span 警告：`clippy::items_after_test_module`（"items after a test module"，span 行 1264）——1795 行的文件把 `KeyValueDriver` 等项放在 `mod structure_defaults_tests` 之后。拆完之后测试模块到了文件末尾，这个 lint **真的消失了**。这是本轨直接治好的症状，不是掩盖。仓库其他 crate 里还有 6 条同类，与本次改动无关。

2. **`packages/drivers/sqlserver/src/sqlserver/catalog.rs` 进了集合。** 其中 4 条是**搬家**：基线 `sqlserver.rs` 的 `get_first` ×2（位于 `get_tables`、`get_all_columns` 内）随代码一起进了 `catalog.rs`，基线 `sqlserver.rs` 因此从 6 条掉到 2 条（只剩 `new_without_default` ×2，行号 34→48）。剩下 2 条 `type_complexity` 是**真的新增**，见下。

3. **`clippy::type_complexity` +2 是本次拆分引入的**，位置是 `sqlserver/catalog.rs` 的 `get_all_columns`（2 条 = lib + all-targets 两轮各一条）。机制已经查清，不是凭空冒出来的：基线上这段代码的签名在 `#[async_trait]` 展开出来的函数里，**声明 span 来自宏展开**，clippy 的 `type_complexity` 对 `span.from_expansion()` 的签名直接跳过；搬成 inherent 方法后签名是真实源码，就被打分了。同样的返回类型 `Result<HashMap<String, (Vec<ColumnSchema>, Vec<String>)>, DriverError>` 在**我没碰过的** `packages/drivers/postgres/src/postgres.rs::get_all_columns` 里就**没有**警告，正是因为它还在 trait impl 里。

   我试过用 type alias 消掉它（`pub(crate) type AllColumnsMap = ...`），结果是**只消掉一半**——`get_all_columns` 里那条 `let mut all_columns: HashMap<String, (Vec<ColumnSchema>, Vec<(i64, String)>)>` 的局部类型标注仍然触发同一个 lint，要彻底消就得给两个**别人写的**类型都起别名。**这个实验已经完整回滚**（`git checkout --`，工作区已恢复到 `06a84cea5` 且 `git status --porcelain` 为空），没有留在提交里。理由：这是一条样式建议、CI 根本不跑 clippy（`.github/workflows/ci.yml` 里没有任何 clippy 步骤，也没有 `-D warnings`）、仓库基线自带 8 处同类 `type_complexity`；为了消它去改别人拥有的类型的命名，是把设计变更夹带进纯重构轨。**结论：+2 条样式警告，如实记账，不粉饰。**

**"两个 crate 真的被分析了"不是靠"0 warnings"证明的**——靠的是主 span 的路径分布，两次运行都有：

| 路径 | 基线记录数 | 门禁记录数 |
| --- | --- | --- |
| `packages/driver-api/`（共 7 / 6 个文件） | 16 | 15 |
| ├ `packages/driver-api/src/traits.rs` | 1 | 0 |
| ├ `packages/driver-api/src/mock_driver.rs` | 2 | 2 |
| ├ `packages/driver-api/src/resource_adapter_tests.rs` | 2 | 2 |
| ├ `packages/driver-api/src/resource_no_default_bodies_tests.rs` | 5 | 5 |
| ├ `packages/driver-api/src/schema_object_commands/tests.rs` | 1 | 1 |
| ├ `packages/driver-api/src/sql_dump/tests.rs` | 4 | 4 |
| └ `packages/driver-api/tests/support/mod.rs` | 1 | 1 |
| `packages/drivers/sqlserver/`（共 7 / 8 个文件） | 35 | 37 |
| ├ `packages/drivers/sqlserver/src/sqlserver.rs` | 6 | 2 |
| ├ `packages/drivers/sqlserver/src/sqlserver/catalog.rs` | 0 | 6 |
| ├ `packages/drivers/sqlserver/src/metadata.rs` | 2 | 2 |
| ├ `packages/drivers/sqlserver/src/migration/sql.rs` | 2 | 2 |
| ├ `packages/drivers/sqlserver/src/resource_provider.rs` | 14 | 14 |
| ├ `packages/drivers/sqlserver/tests/common/mod.rs` | 9 | 9 |
| ├ `packages/drivers/sqlserver/tests/live_admin_and_sync.rs` | 1 | 1 |
| └ `packages/drivers/sqlserver/tests/live_schema_metadata.rs` | 1 | 1 |

## 7. 守卫测试与反向验证

守卫测试落在 `packages/drivers/sqlserver/src/sqlserver/file_line_cap.rs`（109 行，`#[cfg(test)] mod file_line_cap;` 声明在 `sqlserver.rs` 里，所以整个文件只在测试构建下编译，不需要改 crate 根）。

- `LINE_LIMIT = 800`。
- `split_files_stay_within_the_line_limit`：守住 `src/sqlserver.rs`、`src/sqlserver/*.rs`（**含它自己**）、`driver-api/src/traits/*.rs`，并断言枚举结果非空（防止目录改名后守卫悄悄变成空跑）。
- `database_driver_trait_declaration_does_not_grow`：`driver-api/src/traits.rs` 对着**实测值 1020** 的棘轮。`traits.rs` 现在压不到 800（§8），所以改成"可以变小、不许变大"——比 800 更严格，至少拦住任何把它重新写胖的改动。

**反向验证（在 `06a84cea5` 这棵最终树上重跑，临时灌行 → 跑守卫 → 立刻 `git checkout --` 还原）**：

| 探针 | 灌行目标 | 跑的守卫 | 退出码 | 逐字 panic |
| --- | --- | --- | --- | --- |
| A | `sqlserver/dialect.rs` 574 → 1475 | `split_files_stay_within_the_line_limit` | 101 | `packages/drivers/sqlserver/src/sqlserver/dialect.rs has 1475 lines, over the 800-line limit; split it by responsibility` |
| B | `sqlserver/session.rs` 720 → 1602 | `split_files_stay_within_the_line_limit` | 101 | `packages/drivers/sqlserver/src/sqlserver/session.rs has 1602 lines, over the 800-line limit; split it by responsibility` |
| C | `driver-api/src/traits.rs` 1020 → 1022（**只多 2 行**） | `database_driver_trait_declaration_does_not_grow` | 101 | `packages/drivers/sqlserver/../../driver-api/src/traits.rs has 1022 lines, over the 1020-line limit; split it by responsibility` |

探针 C 证明棘轮不是摆设：`traits.rs` 涨 2 行就红。三次都 `test result: FAILED. 0 passed; 1 failed; 122 filtered out`，还原后行数回到 574 / 720 / 1020，`git status --porcelain` 为空。

已知瑕疵：探针 C 的 panic 里路径显示成 `packages/drivers/sqlserver/../../driver-api/src/traits.rs`——`display()` 辅助函数按 `packages` 组件截断（目的是别把本机绝对路径打进失败信息），跨 crate 引用时前缀没顺手归一化。**只在测试失败信息里出现，不影响判定，不影响定位**，所以没有为此改代码再跑一遍门禁；记在这里而不是假装它不存在。

> 教训（记下来免得再犯）：探针脚本**不能带 `set -e`**。探针 A 就是因为脚本在"预期失败"的 `cargo test`（退出码 101）处直接中止，还原没跑到，`dialect.rs` 一度被留在 1474 行。还原后复测 574 行、`123 passed`。

## 8. `traits.rs` 为什么停在 1020（未达成项）

`AGENTS.md` 要求单文件不超过 800 行。**`traits.rs` 压不到。** 实测构成：

```
总 1020 行 = 模块前言 40 行 + pub trait DatabaseDriver 声明块（第 41–1020 行，共 980 行）
块内：302 行文档/注释 + 94 行空行 + 583 行代码 + 1 行收尾 }
全文件：314 行注释 + 100 行空行 + 2 行属性 + 83 行恰为 "    }" + 87 行恰为 "}"
```

一个 95 项的 trait，签名必须和 impl 待在一起，这是 §2 第 1 条已经裁定的前提。继续往下压只有三条路，全都判定为不可接受：删文档注释（17 个驱动 crate 和宿主读的是这份公共契约）、删空行（最好也就 ~850 行，可读性归零）、拆签名（改 API）。**所以如实记账：`traits.rs` 仍超限 220 行。** 我没有为了指标去刷它。

新增的 6 个 `traits/` 子文件全部 ≤ 486 行，所以"本轨新建文件不得超 800"这条是达成的。

## 9. 被我自己推翻 / 不确定的点

1. **任务书给的 sqlserver 基线是错的。** 书上写"基线 `120 passed; 1 failed`，`the_declared_default_port_is_exactly_what_connect_dials` panic"。在 `d6423f9d1` 上实测是 **`121 passed; 0 failed`，退出码 0**。那个失败态是人为探针造的，不是仓库状态。探针逻辑本身是对的：`default_port()` 返回 `Some(DEFAULT_PORT)` = 1433，`default_host()` 返回 `None`。**以实测为准。**
2. **"全仓扫描行数守卫"这个验收写法无法满足。** `packages/driver-api/src/` 的 `schema_migration.rs`、`reuse.rs`、`capabilities.rs`、`types.rs`、`mock_driver.rs`、`resource_adapter_tests.rs` 以及 sqlserver 的 `metadata.rs`(1302)、`sync_adapter.rs`(1009) 都超 800，且都在本轨范围外。守卫因此**只枚举本次拆分所治理的文件**，不是全仓扫描——不这样做就只能去改不该改的文件。
3. **`type_complexity` +2 是我引入的**，机制已查清（§6 第 3 点），没有消掉，因为消它需要改别人拥有的类型的命名。记在这里，不藏。
4. **`traits.rs` 没达标**（§8）。不算达成。
5. clippy 的**绝对条数**在两次运行之间不可复现（增量缓存不同），所以 §6 的裁定全部基于**集合**；1051 / 1052 这两个数只是参考，不是判据。

## 10. 遗留与待裁定（不属于本轨范围，未处理）

1. `packages/driver-api/src/traits.rs:1120` 附近有一处**失效的文档引用**（指向旧行号）。拆完之后行号又变了，需要下游文档轨一起处理。
2. `execute_standard_sql_command` 的文档首行保留了原有的 `F7:` 需求编号前缀。它是**原有文案**，本轨原样搬运、没有改写（本轨的注释纪律是不引用文档编号）。如果要清理，是文档轨的事。
3. sqlserver 的 `metadata.rs`(1302)、`sync_adapter.rs`(1009) 超 800 行，driver-api 的 6 个文件也超——都在本轨范围外，需要另开还债轨。

## 11. 复现命令

```bash
export CARGO_TARGET_DIR=/tmp/p5fc-target
node scripts/resolve-drivers.mjs --codegen-only --drivers=all
cargo test -p datazen-driver-api
cargo test -p datazen-driver-sqlserver
for f in packages/driver-api/src/traits.rs packages/driver-api/src/traits/*.rs \
         packages/drivers/sqlserver/src/sqlserver.rs packages/drivers/sqlserver/src/sqlserver/*.rs; do
  rustfmt --edition 2021 --check "$f" || echo "FMT FAIL $f"
done
cargo clippy --workspace --all-targets --keep-going --message-format=json
```

注意：验证树放在 `/tmp` 下时必须把 `node_modules` 软链回主仓，否则 `scripts/resolve-drivers.mjs` 会 `ERR_MODULE_NOT_FOUND: fflate`，codegen 退出码 1，**后面所有门禁作废**。