# P3 / CM-70 过期幂等键与记录删除

出口门禁条目：`docs/architecture/platform/connection-management.md:1294-1297`。

> **CM-70 过期幂等键与记录删除（H/W1）**
> - 前置：签名令牌、fake clock，写入已接受且响应丢失。
> - 步骤：有效期内重发同/不同输入；过期重发；超过记录保留期删除记录，再重放令牌；伪造 issuedAt/keyVersion。
> - 断言：同 receipt、不同输入冲突；过期和删除后均不再执行；伪造拒绝；客户端不能用新键自动重试未知写入；open/context 的 owner 重启后旧令牌 SessionLost，运行时 receipt/token 不落盘。

**轨道路径**：这不是「零基础」。CM-54 的幂等账本（`gateway/idempotency.rs`，543 行）已经
建好并被 CM-56 集成契约覆盖。本轨**只补 CM-70 的断言面**，不重建幂等机制。

---

## 第一步：7 条断言 → 代码/测试机械对照表

统计口径：`grep -rin "idempot" packages/runtime/src packages/runtime/tests` 共 **294** 处
（`src/` 242，`tests/` 52）；全工作区按 crate：runtime 121、application 29、platform-api 21、
driver-api 11、backend-client 7、wapp-sdk 1。

### 全局机械事实（决定本轨工作量的关键）

| 机械查询 | 结果 | 含义 |
| --- | --- | --- |
| `grep -rn "issued_at\|issuedAt" packages/runtime` | **0** | runtime 侧没有 `issuedAt` |
| `grep -rn "key_version\|keyVersion" packages/runtime` | **0** | runtime 侧没有 `keyVersion` |
| `grep -rn "retained_until\|retainedUntil" packages/runtime` | **0** | runtime 侧没有保留期 |
| `grep -rn "signatur\|Signature" packages/runtime` | 2（`connection/types.rs:366`、`registry/actor/tests.rs:437`，均为无关的 registry 字段） | runtime 侧**没有签名设施** |
| `impl SubmissionTokenIssuer` 全工作区 | **0** | `platform-api/src/ports/token.rs:63` 是**只有 trait、没有实现**的端口 |

⇒ 签名令牌层在 runtime 侧**完全缺失**；只有 `platform-api` 的 DTO/端口形状和
`application/src/dto/accept.rs` 的 `AcceptRetention::MINIMUM_AFTER_EXPIRY_HOURS = 24` 常量。

### 逐条断言

| # | 断言（逐字拆句） | 状态 | 机械证据 |
| --- | --- | --- | --- |
| A1 | 有效期内重发**同**输入 → **同 receipt** | **已有** | `gateway/mod.rs:236-241` `IdempotencyLookup::Hit(existing) => GatewayAcceptance::replayed(&existing.execution_id, …)`；`IdempotencyLookup::Hit` 定义于 `idempotency.rs:182`；单测 `request.rs:409 a_replay_returns_the_original_receipt_and_first_seen_time` |
| A2 | 有效期内重发**不同**输入 → **冲突** | **已有** | `gateway/mod.rs:242-248` → `GatewayError::IdempotencyConflict`；`IdempotencyLookup::Conflict` 定义于 `idempotency.rs:184-187`；错误 kind `idempotencyConflict` 见 `request.rs:304-308` |
| A3 | **过期**重发 → **不再执行** | **缺** | 全局 `issued_at`/`expires_at`/`key_version` 在 runtime **0 命中**；`connection/testing/clock.rs:417 idempotency_token_expiry_is_24h_of_virtual_time` 只测 `FakeClock::arm/advance` 计时器本身，**没有任何消费者**——是一条孤立的时间算术断言，没有令牌、也没有「不再执行」的判定 |
| A4 | 超过保留期**删除记录**后重放令牌 → **不再执行** | **缺** | `retained_until` 全局 **0 命中**；`IdempotencyStore`（`idempotency.rs:161-174`）只有 `read`/`write`，**没有删除语义**；`InMemoryIdempotencyStore`（:197-266）无删除 API |
| A5 | 伪造 `issuedAt` / `keyVersion` → **拒绝** | **缺** | 无签名设施（见上表）；`platform-api` 侧只有 trait 契约文本（`ports/token.rs:72-76` 注释「签名不符、过期、epoch 不匹配一律 `PortError::TokenInvalid`」），**无实现、无测试** |
| A6 | 客户端**不能用新键自动重试**未知写入 | **部分** | CM-54 侧有机制：`idempotency.rs:188 IdempotencyLookup::Unreadable`，`gateway/mod.rs:249-252` 注释「绝不降级成 Miss 后另写一条」→ `GatewayError::IdempotencyVerificationRequired`。**但没有任何断言把「换一个全新的 key 再来一次」这条路径钉住**——现有用例只证明「同一个 key 不会降级」，没证明「新 key 也拿不到执行」 |
| A7 | `openSession`/`setSessionContext` 的 **owner 重启后旧令牌 → `SessionLost`** | **缺** | `ProviderError::SessionLost` 现有触发点只有 `connection/port.rs:102,116`（会话本身是终态 `Lost`）；`RuntimeError::SessionLost` 只有 `gateway/mod.rs:533`（同一理由）。**令牌与 `runtimeEpoch` 之间没有任何关联**：`IdempotencyScope`（`idempotency.rs`）带 `runtime_epoch` 但它只是作用域键，不做 epoch 校验 |
| A8 | 运行时 **receipt/token 不落盘** | **缺（需新增钉子）** | 目录侧已有先例 `tests/directory_no_disk.rs`（473 行，三条独立反面证据 + 一条故意栽泄漏验证探测器有效），**但它只覆盖 `src/directory/**`**；`src/gateway/**` 的 receipt/token 无任何落盘反面证据 |

### 小结

- **已有 2 条**（A1、A2）——CM-54 的账本已经把这两条做实了，本轨不需要重建。
- **部分 1 条**（A6）——机制在，断言缺。
- **缺 5 条**（A3、A4、A5、A7、A8）——因为 runtime 侧**根本没有签名令牌层**，保留期/删除语义也不存在。

⇒ 工作量集中在**新建一层 runtime 签名提交令牌 + 保留期/删除 + owner epoch 绑定**，
再补 6 组断言。CM-54 的账本与 `IdempotencyRecord` 结构**不动**（改动它会波及 4 处构造点，
其中 `tests/gateway_contract/acceptance.rs:188` 属既有契约测试）。

---

# 施工方案（本轮只交方案，未动代码）

## 0. 方案所依据的机械事实（本轮新查，与上一轮对照表并存）

| 查询 | 结果 |
| --- | --- |
| `impl SubmissionTokenIssuer`（含 `src-tauri`） | **0**。全仓只有 trait + DTO，端口从未被实现过 |
| `ExecutionGateway::new` 调用点 | **5 处，全在 `packages/runtime` 内**（`tests/gateway_fixtures/mod.rs:474,487,506`、`src/gateway/facade_support.rs:337`、`src/gateway/facade_tests.rs:104`）；**`src-tauri` 0 处** |
| `IdempotencyRecord` 字段数 | **3**（`execution_id` / `fingerprint` / `first_accepted_at_nanos`），被既有契约测试 `tests/gateway_contract/acceptance.rs:188` 逐字段构造 → **加字段必破冻结测试** |
| `platform-api::ports::SubmissionToken` 字段 | 只有 `{ idempotency_key, expires_at }`，**没有签名字段** |
| `PortError::TokenInvalid` | 已存在（`platform-api/src/error.rs:47`），注释逐字写着「令牌签名/有效期/作用域/owner epoch 校验失败，或签名密钥版本未知。**未知版本一律拒绝**」，且 `is_transient() == false`（`:588`） |
| `hmac` / `sha2` / `subtle` | 已在 `Cargo.lock`，且 `hmac-0.12.1.crate`、`sha2-0.10.8.crate`、`subtle-2.6.1.crate` 均在本地 registry 缓存 → **离线可加依赖** |
| 本 worktree `.codegraph/` | **不存在** → 按 `AGENTS.md`「没有就不要用」的规定走 grep |

**最关键的一条**：`platform-api` 的 `SubmissionToken` 只有 `{idempotency_key, expires_at}`，**无处安放签名**。
但 `packages/application/src/dto/requests.rs:102` 写着「键内容本身由 `SubmissionTokenIssuer` 签发与核验」，
`dto/idempotency.rs:4` 写着「令牌内容含随机 nonce」⇒ **签名本来就该落在这串不透明文本内部**。
这与 `ExecutionRequest.idempotency_key: String` 是同一个字段 ⇒ 令牌层与网关之间**不需要任何 DTO 改造**。

---

## 1. 分层落点裁定

**裁定：端口留在 `platform-api`，实现新建在 `packages/runtime/src/gateway/token.rs`；`IdempotencyStore` 用「带默认方法体的 trait 扩展」，不做结构改造。**

- 端口（`platform-api/src/ports/token.rs` 的 trait 与 DTO）**一个字都不改**。
- 新增 `packages/runtime/src/gateway/token.rs`，实现 `SubmissionTokenIssuer`。
  **先例已存在**：`platform-api` 定义 `SessionDirectory` 端口，`packages/runtime/src/directory/` 提供
  `InMemorySessionDirectory` 实现它 —— 端口在上游、实现落 runtime，是本仓既有分层惯例，不是新发明。
  依赖方向合法：`packages/runtime/Cargo.toml` 已依赖 `datazen-platform-api`（注释写明 F-04 方向）。
- `IdempotencyStore`（`idempotency.rs:161-174`）新增：
  ```rust
  fn delete(&self, scope: &IdempotencyScope) -> Result<bool, IdempotencyStoreError> {
      Err(IdempotencyStoreError::read("deleteUnsupported"))
  }
  ```
  **默认方法体** ⇒ 5 处既有实现（`InMemoryIdempotencyStore`、单测的 `BrokenStore`/`FailingStore`、
  `facade_support.rs` 的 `UnreadableStore`、`tests/gateway_fixtures/mod.rs` 的 `UnreadableStore`/`WriteFailingStore`）
  全部**零改动继续编译**；只有 `InMemoryIdempotencyStore` 覆盖它。
  语义正确的理由：读不出来的存储本来也不该允许被删，替身继承默认拒绝比假装删成功更诚实。

**排除的备选**：不做旁路删除方法。旁路方法会让「删除」脱离 store 契约，将来接真后端时必炸。

---

## 2. `IdempotencyRecord` 结构边界 —— 确认冻结

**确认不动。** 给 record 加 `expires_at_nanos` / `retained_until_nanos` 会打破
`tests/gateway_contract/acceptance.rs:188` 那处逐字段构造，而按「冻结测试是仲裁者」的规矩，
撞上它的正确做法是回退我的实现、不是改测试。

**新令牌如何与既有 record 关联 —— 靠令牌摘要，不靠 record 字段。**
令牌层自带一张 grant 登记表：`token_digest → Grant { execution_id, fingerprint, issued_at_nanos, expires_at_nanos, retained_until_nanos }`。
首次受理时同时写 CM-54 的 record 和这张 grant；两者通过 `execution_id` 关联，互不侵入对方的结构。

**这套分层正好对上 `persistence-model.md:280-296` 那一行**：「删除后 | 过期签名令牌仍被拒绝」。
删除记录之所以安全，是因为拒绝**由令牌驱动、不由记录是否还在驱动**——记录删掉只会让情况更严，
不可能更松。

---

## 3. 令牌串格式与受理闸门顺序

格式（不透明、单行、无凭据）：
```
cm70.<keyVersion>.<b64url(payload)>.<b64url(HMAC-SHA256)>
payload = nonce | operation | owner_epoch | issued_at_nanos | expires_at_nanos | key_version
```
MAC 原语：`hmac` + `sha2`（已在锁文件与本地缓存，离线可构建）。
若离线构建失败，降级为带密钥的 FNV tag，并在模块文档注释里明写「非生产级 MAC」，不假装。

**受理顺序 —— 令牌闸门插在最前（新增第 0 步）**：

```
0. 令牌闸门：验签 → keyVersion 已知？→ 未过期？→ epoch 匹配？
   任一不过 → GatewayError::SubmissionTokenRejected { reason }，不进账本、不分配 executionId
1. validate（现有）
2. session_view / reject_terminal_session（现有）
3. authorize（现有）
4. 账本查重（现有 CM-54）
5. 登记 record + grant（现有 CM-54 + 新增）
```

**「闸门在最前」就是 A4 的实现**：记录已删除时请求在第 0 步就被拒，**根本走不到第 4 步的 `Miss`**。
如果闸门放在查重之后，删掉记录就会让过期令牌重新变成 `Miss` 并被真的执行一遍——那正是 CM-70 要防的事。

---

## 4. A6 重试围栏（retry fence）

CM-54 的 `IdempotencyLookup::Unreadable` 已经在 `gateway/mod.rs:249-252` 挡住了「同 key 降级」，但没挡住「新 key」。
补法：

1. `Unreadable` 触发时，把 `(dbSessionId, RequestFingerprint)` 记进围栏表，状态 `Unknown`。
2. 之后**任何 key、任何新令牌**，只要命中同一 `(dbSessionId, 语义指纹)`，一律仍返回 `IdempotencyVerificationRequired`。
3. 只有显式 `resolve_unknown_outcome(...)` 才清围栏。

即「客户端换一个 key 偷偷重试未知写入」在语义指纹层面仍然撞墙。这不依赖 CM-70 的令牌层，
但放在同一处闸门后面实现最省事。

---

## 5. A7 `SessionLost`

令牌 payload 带 `owner_runtime_epoch`。owner 重启后 `SessionHandle.runtime_epoch` 变，
令牌与句柄不匹配 ⇒ 拒绝，映射到 `RuntimeError::SessionLost`（`connection/error.rs:94`）。

**注意这与既有 `SessionLost` 是两条不同触发路径**，两条都要留：
- 既有：会话本身已是终态 `Lost`（`gateway/mod.rs:533` → `port.rs:102,116`）
- 新增：**令牌绑定的 owner epoch 与当前 owner 不符**（owner 重启后拿旧令牌）

后者正是 CM-70 「不得被当成首次请求重建」那句话的落点。

---

## 6. 拆分建议 —— 建议**不拆多轨**，只拆 commit

理由：令牌层、保留期、接入 `accept` 这三块，**拆开任一块都不独立可验收**。
「有 `token.rs` 单测全绿、但网关压根没调它」能过自己的测试，而 CM-70 的行为一点没变——
这正是协调者警告的「半成品骗绿灯」形态。

- 建议：**1 条轨，2 个 commit**。
  - commit 1：令牌层 + 保留期/删除 + 接入 `accept` + A3/A4/A5/A7 的断言
  - commit 2：A6 围栏 + A8 不落盘钉子
- 若协调者坚持要并行：**唯一干净的切口是 A8**（纯新测试文件、不碰实现、可独立成轨）。
  围栏（A6）不能并行——它和令牌闸门都要改 `gateway/mod.rs` 的同一段 `accept`，两人同时改必冲突。

---

## 7. 范围承诺：本轨能否一次做完

**能一次做完，边界如下。**

最小可独立验收闭环 = **commit 1**，它单独就让 A3 / A4 / A5 / A7 四条从「缺」变成「已覆盖」，
这已经是 CM-70 断言面的大头。commit 2 补 A6 / A8。

**明确不承诺**：
- 不做宿主接线。`src-tauri` 对 `ExecutionGateway::new` 是 0 调用点，网关本来就没被宿主装配，
  接上去是另一条轨的活。这一条会写进「未验证项」，不装作做了。
- 不改 `platform-api` 的任何 DTO / trait。
- 不改 `IdempotencyRecord` 结构。
- 不实现 tombstone 宿主职责（`ports/token.rs` 文档明确「墓碑由宿主持有」，不在 runtime）。

---

## 8. 风险与回退

| 风险 | 处置 |
| --- | --- |
| 加 `hmac`+`sha2` 离线拉不到 | 已在 `Cargo.lock` + 本地缓存；万一失败降级为带密钥 FNV tag 并**在注释里写明非生产级** |
| 新构造函数影响既有 5 个 `ExecutionGateway::new` 调用点 | **不改 `new` 签名**，新增 `with_submission_tokens`。5 处零改动，既有 18 二进制 + 382 单测不应受影响 |
| `GatewayError` 加变体波及 `request.rs` 的 `to_persistable_json` | 同步改该 match；既有测试 `every_gateway_error_has_a_machine_readable_kind` 只加不改，应仍通过 |
| 新模块超 800 行 | 按职责拆：`token.rs`（签名/核验）、`retention.rs`（保留期/围栏）、各自的 `#[cfg(test)]` 子块外置 |

---

## 9. 预计文件清单

新增：
- `packages/runtime/src/gateway/token.rs`（签发/核验/KeyVersion/TokenRejection/提交令牌串）
- `packages/runtime/src/gateway/retention.rs`（`retained_until`、grant 登记表、retry fence）
- `packages/runtime/tests/cm70_idempotency_replay.rs` + `#[path]` 子模块（只走公开 API）
- `packages/runtime/tests/cm70_no_disk.rs`（A8 不落盘钉子，照抄 `directory_no_disk.rs` 的三段式反面证据 + 故意栽泄漏自测）

修改：
- `packages/runtime/src/gateway/idempotency.rs`（**仅** 加带默认体的 `delete`）
- `packages/runtime/src/gateway/mod.rs`（新增字段 + 新构造函数 + 第 0 步闸门）
- `packages/runtime/src/gateway/request.rs`（新增错误变体 + `to_persistable_json`）
- `packages/runtime/Cargo.toml`（`hmac` / `sha2`）

不动：`platform-api/**`、`connection/port.rs`、`IdempotencyRecord` 结构、既有测试文件、`hub.md`。

---

## 门禁实测

两次提交各自独立跑完整门禁。`CARGO_TARGET_DIR=/tmp/dz-target-p3-cm70-idempotency-replay`，
每轮门禁**首尾各记一次** HEAD 与工作区 sha（`TREE_SHA` 取 `git stash create`），
证明跑门禁期间没人动过这棵树。日志一律落 `/tmp/p3cm70-*.log`。

### commit 1（`f18bf25fc0f022de62624fbeb38bed04c831b122`，令牌层 + 保留期 + 闸门接线 + A3/A4/A5/A7）

- `cargo fmt -p datazen-runtime --check` → `FMT_CHECK_EXIT=0`
- `cargo build -p datazen-runtime` → `BUILD_EXIT=0`
- `cargo build --workspace` → `WS_BUILD_EXIT=0`
- `cargo test -p datazen-runtime --lib` ×2 → 各 `410 passed; 0 failed`
- 逐二进制 `cargo test -p datazen-runtime --test <name>` → 19/19 全 `EXIT=0`
  （含 `cm70_idempotency_replay` 34 passed、`gateway_contract` 51 passed）
- 告警增量：把 `warning:` 头行排序去重后与基线 `/tmp/p3cm70-baseline-ws-build.log` 对比，
  **58 → 58，multiset diff 为空**（新增的 `hmac` / `sha2` 没带来任何新告警）
- `git diff Cargo.lock` 只有两行新增、零删除：`+ "hmac 0.12.1",` / `+ "sha2 0.10.9",`，
  列在 `datazen-runtime` 名下（`Cargo.lock:2037-2045`）

### commit 2（A6 重试围栏 + A8 不落盘钉子）

- `cargo fmt -p datazen-runtime && cargo fmt -p datazen-runtime --check` → `FMT_CHECK_EXIT=0`
- `cargo build -p datazen-runtime` → `BUILD_EXIT=0`，该 crate 自身 `warning` 计数 0
- `cargo test -p datazen-runtime --test cm70_no_disk` → `NODISK_EXIT=0`，
  `test result: ok. 7 passed; 0 failed; 0 ignored`
- `cargo test -p datazen-runtime --lib` ×2 →
  `LIB1_EXIT=0` / `LIB2_EXIT=0`，两次都是 `test result: ok. 410 passed; 0 failed`
- 逐二进制 `cargo test -p datazen-runtime --test <name>` → **20/20 全 `EXIT=0`**
  （19 个既有二进制逐字结论行 + 新增 `cm70_no_disk 7 passed`；
  `cm70_idempotency_replay` 41 passed、`gateway_contract` 51 passed）
- `cargo build --workspace` → `WS_BUILD_EXIT=0`；
  `warning:` 头行 **58 → 58**，与基线 multiset diff **IDENTICAL**
- HEAD 前后一致：`HEAD_BEFORE=f18bf25fc…` == 门禁后的 `HEAD`（`git status` 只有本轨
  那 4 个改动 + 2 个新文件，没有外来改动）

### commit 3（第 4 轮整改：独立 Tester 判 NOT-PASS 的 H-1 / H-2 两条 HIGH）

两个 HIGH 都是协调者签字授权改 CM-54 冻结面之后才动的手。

- **H-1（争议项，按裁定改）**：`IdempotencyConflict` 原来带 `key: String`，把调用方
  自己的令牌原文回显进错误投影。账本按 `(dbSessionId, runtimeEpoch, key)` 查，冲突只可能
  出现在调用方自己的作用域里，所以这个 `key` 是**把它自己的凭据原样念回去**，零信息量、
  却是可重放凭据的泄漏面。现在字段只剩 `{ existing: ExecutionId, incoming: String }`，
  `incoming` 是 16 位十六进制的 `RequestFingerprint`——定位要的信息一点不少，凭据一份不留。
  连带修掉两处派生 `Debug` 的**嵌套**泄漏：`ExecutionRecord` 手写 `Debug` 只挡了顶层
  `idempotency_key`，`port_request: ExecuteInSessionRequest` 里还嵌着同一个令牌，
  `{:?}` 照样打印出来；现在内层走 `request::RedactedExecuteRequest` 脱敏视图，
  `connection/session.rs` 那条冻结面一个字节不动。
- **H-2（未闭合项）**：围栏原来只在 `accept` 读账本时抬（`Unreadable` 那一条），
  于是「受理成功但压根没下发」的写入不会立围栏，客户端换新键照样自动重试。
  实测证据（改前）：`FENCE_COUNT=0 / NEW_TOKEN_ACCEPTED=… / SQL_ISSUED_FINAL=2`。
  现在在 `dispatch` 里 **`mark_dispatch_issued` 之后、`self.port.execute_in_session(…)`
  之前**抬起，键是 `(db_session_id, fingerprint)`，**绝不绑令牌**（绑了等于没围）。
  刻意不看驱动的返回值：闸门失败会先 `release_dispatch_reservation` 原路退回，
  可证明根本没碰到驱动，所以不会误抬围栏。
- 附带修正一处被 H-2 打红的既有断言：`tests/gateway_contract/timing.rs` 的 p95 用例
  原本每轮只换 `idempotency_key`（同一条写入 ⇒ 同一指纹），第 2 轮起必然撞上围栏。
  改为每轮换 `call.input`，仍然是 20 次真实派发，测的还是派发开销。

### 逐项对照 A1–A8

| 断言 | 落点 |
| --- | --- |
| A1 有效期内同输入重发 → 同一张回执 | `tests/cm70/expiry.rs`、`retries.rs` |
| A1' 有效期内**不同**输入重发 → `IdempotencyConflict` | `tests/cm70/forgery.rs` |
| A3 过期重发 → 拒、且不再执行 | `tests/cm70/expiry.rs`、`token_tests.rs` |
| A4 正例：留存期内重发仍可取回回执；负例：记录已删后重放 → 拒、且不再执行 | `tests/cm70/retention.rs`、`retention_tests.rs` |
| A5 伪造 `issuedAt` / `keyVersion` / 签名 → 拒 | `tests/cm70/forgery.rs`、`token_tests.rs` |
| A6 结局未知时客户端换新键自动重试 → 仍拒（围栏锁在 `(dbSessionId, RequestFingerprint)`） | `tests/cm70/retries.rs` |
| A7 owner 重启后旧令牌 → `SessionLost` | `tests/cm70/owner_restart.rs`、`token_tests.rs` |
| A8 receipt / token 不落盘 | `tests/cm70_no_disk.rs`（7 条用例，三条独立反面证据） |

## 未验证项

- **未跑全 workspace 的 `cargo test`**：本轨的门禁口径是 `datazen-runtime` 的 `--lib` +
  逐二进制；workspace 其余 crate 本轮未触碰、未重跑（`cargo build --workspace` 通过，
  编译面无回归）。合并前的全量回归由集成侧统一跑。
- **跨平台**：只在 macOS 上跑过。`cm70_no_disk.rs` 用到 `TMPDIR` 与 `std::fs::read_dir`，
  都是跨平台的，但 Windows / Linux 上的实际行为未实测。
- **`Subtle` 常量时间比较**：`hmac` 的 `verify_slice` 已是常量时间，仓库未直接依赖
  `subtle`；自定义比较路径的抗时序侧信道性质未做基准测量。
- **并发压力下的围栏**：`GatewayState.unverified` 的围栏只在单测的串行场景里验证过；
  多任务同时对同一 `(dbSessionId, fingerprint)` 重试的竞态未做压力验证。
- **真实 SQLite 驱动端到端**：CM-70 的执行侧证据全部落在 `RecordingPort` 上，
  没有连真实数据库跑一遍「响应丢失 → 重放」。

## 已知要上报的改动（对冻结件）

1. `packages/runtime/tests/gateway_contract/invariants.rs`：把 `retention.rs` / `token.rs`
   加进 `src/gateway` 生产文件白名单，把 `retention_tests.rs` / `token_tests.rs` 加进
   test-only 白名单与 `#[cfg(test)] mod` 声明名单，并把 `tests/cm70_idempotency_replay.rs`
   与 `tests/cm70/*.rs` 加进 800 行上限名单。这是**清单式断言**，新增文件必须登记，
   否则门禁会以「文件集不匹配」失败；没有放宽任何阈值。
2. `packages/runtime/src/gateway/request.rs::every_gateway_error_has_a_machine_readable_kind`：
   补了 `SubmissionTokenRejected` 这一行枚举映射。新增错误变体本来就必须同步这条表。
3. `packages/runtime/src/gateway/token.rs`：`TokenKeyring` 原来 `#[derive(Debug)]`，
   会把**签名密钥原文**打进 `Debug` 输出。本轨改成了手写脱敏实现
   （只打 `current` 与版本号列表）。这是本轨顺手修掉的一个真实缺陷。

### commit 3 门禁实测

- `cargo fmt -p datazen-runtime && cargo fmt -p datazen-runtime -- --check` → `FMT_CHECK_EXIT=0`
- `cargo build -p datazen-runtime` → `BUILD_EXIT=0`
- `cargo test -p datazen-runtime`（整 crate，22 个测试目标）→ `TEST_ALL_EXIT=0`，
  合计 **650 passed / 0 failed**：unittests **411**、`cm70_idempotency_replay` **46**、
  `cm70_no_disk` **13**、`gateway_contract` **51**，`budget_cm65_*` 8/2/6、`budget_cm66` 7、
  `directory_*` 12/6/7/10、`registry_*` 10/11/9/6/7、`resource_*` 11/4/7/6、
  `p3_session_port_contract` 7、`registry_audit` 6
- `warning:` 归因：`packages/runtime/src/**` **0 条**；`cm70_no_disk` **0 条**；
  `cm70_idempotency_replay` 16 条、`gateway_contract` 16 条，全部是共享夹具
  `tests/gateway_fixtures/mod.rs` 里「这个二进制没用到」的 dead_code（每个集成二进制
  只用夹具的一部分，属预期）
- 行数预算：`src/gateway/mod.rs` 799、`src/gateway/request.rs` 557、
  `tests/cm70_no_disk.rs` 777、`tests/gateway_fixtures/mod.rs` 760、
  `tests/cm70/retries.rs` 363、`tests/cm70/expiry.rs` 286、`tests/gateway_contract/timing.rs` 62
  （上限 800）

## 本轮新增的已知代价

- **围栏 `HashSet` 只增不减**：每条已下发的语义写入留一条 `(db_session_id, fingerprint)`。
  这与 `GatewayState.records` 这个**既有**无界 map 是同一性质，网关整体本来就是进程内
  生命周期、没有淘汰口径。给它加清扫会直接破坏「结局未知」这个保证本身（清掉等于宣布
  「这条写没生效」），所以不做，留给网关生命周期层面的统一决策。

---

## CM-70 修复轮 2（commit 4）：3 项强制 + 2 项小项

上一轮判定 **CONDITIONAL PASS**，本轮是收口：补 M1、补 D1、落 M2 裁定、Q1 注释澄清。

### D1（强制）：`cm70_no_disk` 的进程级 `TMPDIR` 竞争

- **现象**：验证方在 HEAD `0a9a8f6e3` 上连跑 25 次，**7 绿 / 18 红**（基线 24 绿 / 1 红）。
  `git diff c6597b803 0a9a8f6e3 -- tests/cm70_no_disk.rs` 里 `set_var` 命中为 0 ⇒
  机制是既有的：`PrivateTempRoot` 改的是**进程全局** `TMPDIR`，同一二进制内两个
  `#[tokio::test]` 并行跑时各自写进对方的根。case 数从 7 涨到 13 把窗口拉宽了。
- **修法**：照搬 `tests/directory_no_disk.rs` 的房规（`TEMP_ROOT_LOCK` 持有为结构体
  字段，绕过在结构上不可能）。`static TEMP_ROOT_LOCK: Mutex<()>` + `new()` 里
  poison 容忍地取锁，**先取锁再算 `std::env::temp_dir()`**；`_serial: MutexGuard`
  字段声明在最后 ⇒ `Drop` 先跑（还原环境变量 + 删目录）之后才释放锁。
  取锁点 203 行、`temp_dir()` 206 行、`_serial` 字段 197 行。
- **没选注入式 temp root**：生产 `src/**` 对 `temp_dir`/`TMPDIR` 命中为 0，no-disk
  扫描正是它的探测器；注入要动冻结面 + 799 行的 `mod.rs`，代价与风险都更大。
- **验收**：`cargo test -p datazen-runtime --test cm70_no_disk` 连跑 **25 次全部
  `test result: ok. 13 passed; 0 failed`**。
- **反证（锁是承重的）**：把锁拆成按 label 各一把（`TEMP_ROOT_LOCK` / `TEMP_ROOT_LOCK2`，
  即复刻修复前的「互不排斥」）后连跑 25 次 → **18 绿 / 7 红**（`FAILED. 12 passed; 1 failed`）。
  竞争原样回归，随后已还原（`TEMP_ROOT_LOCK2` 命中数 0）。
- `tests/directory_no_disk.rs` 也有同名机制，但在**另一个二进制/另一个进程**里，与本文件互不影响。

### M1（强制）：输入侧 `ExecutionRequest` 的 `Debug` 泄漏令牌

- **现象**：`TA_R1_EXECUTION_REQUEST_DEBUG LEAKED=true HAS_MAC_SEGMENT=true LEN=232`。
  上一轮只堵了输出侧 `ExecutionRecord`（经 `RedactedExecuteRequest`），输入侧
  `ExecutionRequest` 仍是 `#[derive(Debug)]`，`format!("{req:?}")` 会把令牌连 MAC 段
  逐字打印。
- **修法**：`src/gateway/request.rs` 手写 `impl Debug`，只把 `idempotency_key`
  打成 `<redacted>`，其余字段（`handle`/`expected_context_revision`/`call`/`source`/
  `resource_binding_id`）原样。`#[derive(Clone, PartialEq)]` 保留。
- **教训（要保留在代码里）**：派生 `Debug` 必须防到**叶子**。只挡最外层结构体，
  派生实现照样一路走到叶子字段——上一轮的错误就是同一个文件里只堵了一半。
- **新测试** `packages/runtime/tests/cm70/redaction.rs`（2 例）。放这里而不是
  `cm70_no_disk.rs`：后者只剩 1 行额度；且 `tests/gateway_contract/invariants.rs`
  用 `read_dir` 枚举 `tests/cm70/*.rs`，新文件自动进 800 行门禁，不用改枚举表。
  两例都带前提守卫（4 段、末段 ≥32 位），保证测的是令牌而不是空壳。
- **变异证明**（两份日志都留档）：
  - 把 `idempotency_key` 改回明文 → `EXIT=101`，
    `test result: FAILED. 0 passed; 2 failed; ... 46 filtered out`，
    两条都 panic 在 `redaction.rs:33:5`（`!rendered.contains(token)`）。
  - 把 `call` 也脱敏（过度脱敏）→ `EXIT=101`，
    `test result: FAILED. 1 passed; 1 failed; ...`，panic 在 `redaction.rs:52:5`。
  - 还原后 `test result: ok. 2 passed; 0 failed; ... 46 filtered out`。

### M2（裁定，已落代码）：脱敏范围只限凭据字段，`call` 原样

- 令牌是凭据，`call` 是负载。排查幂等冲突恰恰要看 payload（指纹里就有
  `serde_json(input)`），把 `call` 一起打码会让脱敏把排障信息也毁掉。
- 代码：`ExecutionRequest` 与 `RedactedExecuteRequest` 都只脱敏凭据字段；
  `redaction.rs:52` 的断言把这条裁定**变成可执行的**（过度脱敏即失败）。
- ⚠️ 遗留：将来若有驱动命令把凭据放进 `call` 的参数里，这条口径要重新评估。
  已写进 `RedactedExecuteRequest` 的文档注释。

### Q1：围栏处注释澄清（`mod.rs`，净增 0 行）

- 原注释只解释了「为什么不等驱动返回再抬」。补上：判据是一条**禁令**（不得拿新键
  自动重试一条**结局未知**的写入），**并不要求放行什么**；对驱动语义上判定「本次没生效」
  的那类失败同样抬栏是符合判据的保守实现——网关看不到驱动内部的落库顺序，替它断言
  「这次确定没生效」只会造出一个无从核实的乐观前提。逃生舱 `resolve_unknown_outcome`
  有专条用例（`tests/cm70/retries.rs`）。
- **行数**：`src/gateway/mod.rs` 改前 799 行、改后仍 **799 行**——压缩重排既有注释
  段落实现，`wc -l` 前后都是 799。

### 方法论留痕

- SQL 是 `dispatch` 发出的，不是 `accept`。验证方第一次的探针只走了 `accept`，
  量到的 `=1` 差点被误读成「没拦住」。任何回归用例都必须经 `dispatch` 重试，
  且要断言 SQL 次数而不只是错误变体。

### commit 4 门禁实测

- 首尾各记一次：`HEAD_BEFORE=HEAD_AFTER=0a9a8f6e321ca94c8250244246c8237309aeda12`，
  `STATUS_SHA_BEFORE=STATUS_SHA_AFTER=1229f548…`（sha256 of `git status --porcelain`）
- `cargo fmt -p datazen-runtime -- --check` → `FMT_CHECK_EXIT=0`
- `cargo build -p datazen-runtime --tests` → `BUILD_EXIT=0`
- `cargo test -p datazen-runtime` → `TEST_ALL_EXIT=0`，**652 passed / 0 failed**。
  与 commit 3 的 650 逐目标对齐后**只有** `cm70_idempotency_replay` 46→48（+2，
  即 `redaction.rs` 两例），其余 20 个目标逐个不变（含 unittests 411、
  `cm70_no_disk` 13、`gateway_contract` 51）。
- `warning:` 归因：`packages/runtime/src/**` **0 条**、`cm70_no_disk` **0 条**；
  `cm70_idempotency_replay` 16 条、`gateway_contract` 16 条（1 条重复），
  全是共享夹具的 dead_code，与 commit 3 同量。
- 行数预算：`src/gateway/mod.rs` 799、`src/gateway/request.rs` 590、
  `tests/cm70_no_disk.rs` 799、`tests/cm70/redaction.rs` 86（上限 800）

### 遗留（结构债，不在本轨范围）

- 后续轨道要把 `effect_outcome` 接进 `ExecutionReceipt`，那会动 `mod.rs` 的结构，
  **必须先拆模块**（现在 799/800，没有余量）。

### `request.rs` 凭据字段扫描（结论）

带派生 `Debug` 且**含凭据**的类型：`ExecutionRequest`——唯一一个，已手写脱敏。
其余派生类型均无凭据：`AcceptanceDisposition`（只有时间戳）、
`GatewayAcceptance`（回执/来源/时刻）、`GatewayError`（CM-54/CM-70 各变体**刻意**
不带令牌与键，`IdempotencyConflict.incoming` 是语义指纹不是键）。
`RedactedExecuteRequest` 是手写 `Debug`，已脱敏。
