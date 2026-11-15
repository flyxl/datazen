# TE E4 工厂导出与链接 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`d4d59d179 feat(rune): the host links the basic SKU's four drivers, eleven factories (issue-rune E4)`
- 改动面：`crates/drivers/sqlite/src/lib.rs`（补同形 `factories()`）、`crates/drivers/postgres/src/lib.rs`（导出 postgres+questdb+cloudberry 三工厂）、`crates/drivers/mysql/src/lib.rs`（导出六家族工厂）、`crates/drivers/redis/src/lib.rs`（导出单工厂）、`crates/datazen-rune-host/Cargo.toml`（链接三家远程驱动 crate）、`crates/datazen-rune-host/src/live.rs`（`host_registry()` 链式准入 11 工厂；1 个新测试）

## 实现要点

- **导出形态对齐 sqlite 先例**：四个 crate 各导出 `pub fn factories() -> Vec<&'static dyn DatabaseDriverFactory>`——返回的就是 `register_driver!` 提交的同一批工厂（postgres 含 `POSTGRES_FACTORY` 与宏生成的 QuestDb/Cloudberry 别名；mysql 六家族 Mysql/Mariadb/Doris/Starrocks/Manticore/ObOracle；redis 单工厂；sqlite 补列表形态），注释与 sqlite 的 N3 先例同口径：静态组装（`with_factories`）优于链接期 inventory 扫描，宿主的驱动清单显式、测试二进制链接丢不掉注册。
- **宿主声明 = basic 预设**：`host_registry()` 把四个列表链成 11 个工厂 id 一次准入（sqlite、postgresql/questdb/cloudberry、mysql/mariadb/doris/starrocks/manticore/ob_oracle、redis）——与 `scripts/resolve-drivers.mjs` 的 basic 预设（仓库权威定义）逐一对应；`available_types()` 即宿主的引擎声明，E5 的连接表单白名单直接取它。
- **排序即字节序**：`available_types()` 对 seeded id 做字节序排序——`manticore` 先于 `mariadb`（'n' < 'r'），测试期望序以此为准（首跑抓到，已修正断言而非实现——排序语义属注册表既有行为）。

## 测试

| 测试 | 位置 | 断言 |
|---|---|---|
| `the_host_registry_admits_the_basic_sku_eleven_factories` | live.rs | `host_registry()` 的 `available_types()` 恰为 11 id、字节序排序、与 basic 预设一致 |

GR5 首跑（CI 同形，见下）同时覆盖：mysql 六家族工厂契约（单例驱动/provider 绑定、create() 不二次建驱动）与 postgres 别名资源工厂契约——既有驱动 crate 测试零回归。

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（55 + 145 passed） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| GR3 | `cargo test -p datazen-rune-host --offline --lib` | 通过，exit 0（59 passed，新增 1） |
| GR4 | `cargo check -p datazen-rune-host --offline` | 通过，exit 0 |
| GR5 | `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-driver-postgres -p datazen-driver-mysql -p datazen-driver-sqlite -p datazen-driver-redis --offline --tests` | 通过，exit 0（69 套件 1266 passed；redis 需真服务器的活套件按预判诚实跳过——沙箱无 redis-server，与立项环境差距记录一致；无需降级 `--lib`） |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 链接即声明：宿主二进制的引擎集 = basic SKU 11 id，静态准入无注册丢失窗口。链接体积与三平台 CI 矩阵的变化属发布域（T11 的 release 流水线按 tag 触发，下次发版自然携带）。遗留：连接表单仍写内存端口——E5 把它接到活宿主（引擎白名单即本任务的 `available_types()`）；三家远程驱动的真机端到端冒烟待有服务器环境（E6 记录）。
