# TE E6 离线验证扫尾 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`a623e8a1f feat(rune): the remote drivers' refusals prove they were called (issue-rune E6)`
- 改动面：`crates/datazen-rune-host/src/live.rs`（1 个新测试：三家远程驱动的连接不可达路径断言）

## 实现要点

- **驱动真被调用**：对 127.0.0.1 的 5432/3306/6379（测试前实测三端口全部 ConnectionRefused，沙箱无任何监听）各建一条真实引擎连接，`OpenConnection` 作业走 worker 的完整解析-连接路径——断言失败句子非空、绝不含 "no driver"（证明经注册表解析到了驱动本体）、且句子谈的是连接本身（connect/refus）。链接≠空接线的最后一块离线证据。
- **池退避与测试窗口**（实测教训）：sqlx 池对拒绝连接按 `acquire_timeout`（= 配置的 `connection_timeout`，产品默认 30 秒）退避重试——首版测试用 3 秒 poll 窗口被池退避活活拖过超时。修正：夹具用 `sqlite_config` 作底改引擎/地址并钉 `connection_timeout: 2`，产品默认不动；三家全部在窗口内诚实作答（GR3 全套 5.75 秒）。
- **redis 形态差异**：redis 驱动不走 sqlx 池（单连接尝试），拒绝即答——同一断言对三种连接实现成立。

## 测试

| 测试 | 位置 | 断言 |
|---|---|---|
| `remote_drivers_answer_honest_refusals_from_a_closed_port` | live.rs | postgresql/mysql/redis 三家：开作业 → 连接失败句子；非 "no driver"；句子谈连接；产品默认超时不变 |

既有测试零回归。

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（55 + 146 passed） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| GR3 | `cargo test -p datazen-rune-host --offline --lib` | 通过，exit 0（62 passed，新增 1） |
| GR4 | `cargo check -p datazen-rune-host --offline` | 通过，exit 0 |
| GR5 | `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-driver-postgres -p datazen-driver-mysql -p datazen-driver-sqlite -p datazen-driver-redis --offline --tests` | 通过，exit 0（69 套件 1266 passed；redis 需真服务器的活套件诚实跳过——输出明示无 redis-server，与立项记录一致；无需降级 `--lib`） |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

环境备注：GR2/GR4 各有一次失败均为磁盘写满的构建噪声（沙箱 9.9G 根分区，测试二进制膨胀），清理陈旧构建哈希目录后复验通过；真机端到端冒烟（连真 postgres/mysql/redis 服务器的完整浏览/编辑/查询）为环境差距待验——沙箱无服务器二进制且无 root，如实记录，不写进任何"已支持"声明。

## 结论与遗留

**放行。** 离线可验的全部验完：工厂准入（E4）、表单落地与封存（E5）、连接不可达的诚实句子（E6）；驱动 crate 自身测试套 CI 同形全绿（GR5）。遗留：真机端到端冒烟待有服务器环境时补（E7 的矩阵按验证程度分级措辞）；postgres 非默认 schema 的 schema.table 寻址若真机验证暴露问题另立任务修。
