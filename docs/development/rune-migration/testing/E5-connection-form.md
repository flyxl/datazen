# TE E5 连接表单接活宿主 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`d221c3ed0 feat(rune): the connection form reaches the live host (issue-rune E5)`
- 改动面：`crates/datazen-ui-api/src/lib.rs`（`ConnectionDraft` 六字段结构；`WorkspaceApi::save_connection` 签名演进为草稿；`DesktopAdapter` 增 `save_connection`/`available_engines` 默认方法；内存端口实现与测试随签）、`crates/datazen-ui/src/lib.rs`（表单加 username/password/database 三输入框；`save_connection_form` 组草稿 + 白名单检查 + 宿主先行；1 个新测试）、`crates/datazen-rune-host/src/live.rs`（`save_connection` 映射 `ConnectionConfig` 经 `ConnectionService.save` 落盘；`available_engines` 答注册表准入；slug id 去重；地址解析；2 个新测试）

## 实现要点

- **一草稿两端口**：`ConnectionDraft { name, engine, address, username, password, database }`——内存端口解构取前三（其余忽略），活宿主六字段全映射；同一形状，无第二套表单词汇。
- **封存自动继承**：活宿主经 `ConnectionService.save` 落 `connections.json`，仓储的 save 在写盘前封存密码（T10 机制原样生效）——测试断言文件含 engine/host/database 而绝不含明文密码。
- **白名单 = 注册表准入**：表单侧（UI）在派发前按 `available_engines()`（即 E4 的 11 id）拒白名单外引擎；宿主侧再拦一道（注册表是真相）。空引擎在活路径要求填写（活宿主链接的是真类型，不猜默认）；演示路径保留内存端口的 postgres 默认——测试首跑暴露"默认 postgres 不在白名单（postgresql 才在）"后定的此规。
- **id 与地址**：slug(name) + 对仓去重（与内存端口同形，跨世界可预期）；地址 `host[:port]` 解析，坏端口给句子不给 panic。

## 测试

| 测试 | 位置 | 断言 |
|---|---|---|
| `the_connection_form_saves_through_the_host_and_the_whitelist_refuses` | ui lib.rs | 六字段草稿完整到达宿主、表单清空；oracle 在派发前被拒且句子点名链接集；空引擎要求填写 |
| `a_form_connection_lands_sealed_in_the_store` | live.rs | postgres 草稿落盘：engine/host/database 在、明文密码绝不在；列表以 slug id 服务回来 |
| `a_form_connection_refuses_what_the_host_does_not_link` | live.rs | oracle/空引擎拒且句子点名；坏端口句子；白名单 = 11 id |
| `a_connection_without_an_address_cannot_be_saved`（随签） | ui-api | 内存端口草稿往返不回归：地址必填、slug/去重语义原样、凭据字段随行不改变语义 |

既有测试零回归。

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（55 + 146 passed，新增 1） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| GR3 | `cargo test -p datazen-rune-host --offline --lib` | 通过，exit 0（61 passed，新增 2） |
| GR4 | `cargo check -p datazen-rune-host --offline` | 通过，exit 0 |
| GR5 | `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-driver-postgres -p datazen-driver-mysql -p datazen-driver-sqlite -p datazen-driver-redis --offline --tests` | 通过，exit 0（69 套件 1266 passed，与 E4 时点一致——本任务未改驱动 crate） |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

环境备注：沙箱无 OS 密钥环，两个宿主新测试按 T10 测试既有模式设 `DATAZEN_NATIVE_KEYRING=file`（密钥文件在临时目录内）；GR2/GR4 首验失败均为磁盘写满的构建噪声，清理陈旧构建哈希后复验通过。

## 结论与遗留

**放行。** 远程连接经表单落地、密码封存、白名单如实；内存端口零回归。遗留：表单先提供直连最小集（SSH/HTTP 隧道、SSL 细节是独立域，`ConnectionConfig` 字段已有落点）；postgres 双层命名空间的非默认 schema 表寻址与真机端到端冒烟同属 E6 的验证边界。
