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

（待填）

## 未验证项

（待填）
