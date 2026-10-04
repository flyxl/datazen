# P3 Hub

> 集成分支的跨轨汇总。**临时进度文件**——P3 开发完成并通过验收后由协调者删除。
> 设计结论请写进 `docs/architecture/`，不要停留在此文件。

## 基准

- 集成分支：`main`
- 当前基点：`060053afb`（`feat(p3-budget): merge`）

## 波次划分

| Wave | 内容 | 状态 | 合并提交 |
| --- | --- | --- | --- |
| 0 | `registry/port.rs` 接缝（`SessionPort` trait + `SessionView` 投影） | ✅ 已合入 | — |
| 1 | `directory` / `resource` / `budget` 三轨并行 | ✅ 全部 TEST_PASSED 并合入 | `60f0ac6b9` / `2c7ceac6e` / `060053afb` |
| 2 | `registry` / `gateway` 两轨并行 | 🔄 Coder 进行中 | — |

Wave 2 之后无已定义的 Wave 3。剩余工作为 P3 退出门禁对账（CM-02～07、20～32、37～39、54～56、60、61～74），其中 CM-73 / CM-74 必须转绿。

## Wave 1 汇总

| 轨 | 合并提交 | 轮次 | lib 测试 | 集成测试 |
| --- | --- | --- | --- | --- |
| `directory` | `60f0ac6b9` | 2 | 172 | 12/7/10/6 |
| `resource` | `2c7ceac6e` | 1 | 185 | 7/4/6 |
| `budget` | `060053afb` | 1 | 162 | 8/2/6/7 |

main 合并后门禁：lib **223 passed / 0 failed**（=148+24+37+14）、build 0 warning、fmt 0 字节、平台边界 `0 violation(s), 0 error(s), 3 advisory(ies)`。

## Wave 2 轨道

| 轨 | 分支 | 工作树 | 状态 |
| --- | --- | --- | --- |
| `p3-registry` | `feature/p3-registry` | `.worktrees/datazen-p3-registry` | 🔄 Coder 进行中 |
| `p3-gateway` | `feature/p3-gateway` | `.worktrees/datazen-p3-gateway` | ✅ Coder 已 `READY_FOR_TEST`（`19545268d`）→ 🔄 Tester 验收中 |

各轨细节见对应分支的 `progress.md`。

### gateway 轨交付快照（2026-10-04）

- HEAD `19545268d`，基线 `060053afb`，工作区 0 行；提交链 `e36228175` → `2a1ba7ada` → `19545268d`
- 23 个改动路径全在允许范围；`lib.rs` 恰好一行 `pub mod gateway;`
- 自报：lib `323 passed`（= 223 + 100）、build 0 warning、fmt 0 字节、边界 PASS、`--test gateway_contract` `44 passed`
- **以上均为 coder 自报，未经复核。** Tester 须独立复跑并自行设计新变异，不得采信。

### gateway 轨三条待裁定 → 协调者已答

1. **D-01 归属**：早已裁定——`CancelReceipt` 由 **registry 轨**定义并从 `datazen_runtime::registry::` 导出（放 `registry/receipt.rs`，因 `connection/**` 在 registry 的禁改区），gateway 轨只消费、**禁碰 `registry/**`**。gateway 现用 `CancelDisposition` / `CancelOutcome` 前缀类型 + `cancel::disposition_from_port_state` 单点收敛，属**合并前的临时形态**；registry 落地后该函数改为透传，`CancelOutcome` 形状不变。Tester 需验证「收敛点确实只有一个出口」，否则合并时会长出第二份三态语义。
2. `GatewayError` 归属：确认为 gateway 轨自有错误枚举（`RuntimeError` 是冻结 DTO，§4 禁止加变体，且缺 `PermissionDenied` / 幂等核验变体）。`GatewayError::Runtime` 为透明变体，**绝不降级**。
3. `registry::SessionView` / `SessionHandle` 缺失：确认 registry 轨只做**再导出**（§3.2 #5）。gateway 现从定义处 `datazen_runtime::connection::…` 取用，registry 补上再导出后 import 机械换路径，语义不变。**这不是缺陷**，但若合并时需要大改则是阻塞项。

### 协调者已裁定（各轨不得重新讨论）

1. §3.2 #5 registry **重导出** `SessionView`/`SessionHandle`，绝不重新定义。
2. gateway 模块路径是 `packages/runtime/src/gateway/`（`execution` 已被 `connection/execution.rs` 占用）。
3. 真实命令形状 `CommandCall { command: String, input: serde_json::Value }`。
4. tokio 不是禁用依赖（F-03 黑名单 = `axum`/`actix-web`/`warp`/`tonic`）。
5. D-05：`SessionPort` 返回 `RuntimeError` 而非 `PortError` 是有意分层，文档中出现 0 次；§4 的 11 个 port 全是跨边界端口。
6. D-01：必须用真实 `CancelReceipt` DTO 补齐，不得在 Wave 2 就地私造结构体绕过去。
7. D-02：epoch 不匹配必须只留一个对外错误码出口。

### 冻结面缺口分配

- **D-01** 归 `registry`：改 `registry/port.rs`、定义并导出 `CancelReceipt`。因 `connection/**` 在禁改区，`CancelReceipt` 落在 `registry/receipt.rs` 而非 `connection/execution.rs`。
- `gateway` 只消费 registry 导出的 `CancelReceipt`，**禁碰 `registry/**`**。

### 文件冲突面

`packages/runtime/tests/common/` 属目录轨、`tests/support/` 属预算轨，均已在 main 上。Wave 2 两轨只能新建 `tests/registry_fixtures/` 与 `tests/gateway_fixtures/`，不得触碰前两个目录，否则合并必冲突。

## 基线不变量（只许涨不许跌）

- main lib：**223**（硬地板）
- 集成测试：directory 12/7/10/6、resource 7/4/6、budget 8/2/6/7、`p3_session_port_contract` 10

## 待办与遗留

| 项 | 状态 |
| --- | --- |
| Wave 2 文档批次：`shared-boundaries-and-ports.md` §3.2 第 6 行 `execution` → `gateway`；记录 `SessionView`/`SessionHandle` 重导出而非重定义的规则；将 `SessionPort` 登记为内部接缝（非 §4 的 11 个跨边界端口） | 待 Wave 2 合并后由协调者执行 |
| P3 退出门禁对账（CM-02～07、20～32、37～39、54～56、60、61～74） | 待 Wave 2 验收后 |
| `cargo build --workspace` 仍有 58 条 warning（55 diagnostics + 3 cargo summary）。**不得**记作已清除 | 未处理 |
| `datazen-driver-redis` 的 `approximate_constant` ×2 在禁改区 | 已知遗留 |
| 候选门禁 `cargo clippy -p datazen-platform-api -p datazen-driver-api -p datazen-application -p datazen-ai-api --lib -- -D warnings`（已验证安全）可关闭一度藏过真实 lint 的范围缺口 | 未执行 |
| CM-72 是唯一零引用条目 | Wave 2 registry 轨负责 |
| 每驱动 `case_rules` 对 `Unknown` 的真值研究（sqlite 大小写不敏感导致 `Users`/`users` 两种缓存身份） | 用户裁定记为 P2 遗留 |

## Wave 1 遗留建议（不阻塞，记录备查）

- `resource/src/adoption.rs` 命名不在 §3.2 词表内，建议合并/重命名为 `replacement.rs`
- CM-37「权限不跨缓存」覆盖偏薄
- resource 用内联 fixture，directory 用 `test-harness` dev-deps，两者不一致
- resource 的 CM-68/69/73 缺真实驱动 E2E
- D-2：`FORBIDDEN` 子串表漏掉 `tempfile` / `fs::copy` / alias import（已接受为低危）

## 操作纪律要点

- 同一棵工作树不得同时被提交方和验证方使用；Tester 的变异必须在 `git worktree add --detach` 的独立工作树里做。
- 门禁重跑首尾各记录一次 HEAD 与工作区 sha。
- 长输出（测试命令）落系统临时目录再取结论行，退出码单独打印。
- 绝不读取或打印 `.env` / `.env.test` 内容（程序读文件合法，内容回显进上下文才违法）。
- 编译产物放各自 worktree 内 `target/`，合并后随 worktree 清理。