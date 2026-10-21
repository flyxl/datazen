# 轨道 G：identity-resolver — 进度台账

> **本文件是开发期台账，交付即销毁。** 验收合并时必须删除，不得存活在 `main` 上（AGENTS.md「进度台账」条）。
> 结论已同时落成代码、测试与 `docs/architecture/platform/` 里的正式文档事实。

分支：`feature/identity-resolver`（基线 `65dac1e2c` = 集成分支，含 A/B/C 轨成果 + `OrganizationScope`）

---

## 1. 做了什么

**目标**：`IdentityResolver` 端口（`packages/platform-api/src/ports/identity.rs`）自落地以来**无人实现**。本轨实现桌面形态的执行身份解析，让 web 多租户形态有可用的身份解析路径。

**新增**：

- `src-tauri/src/platform/execution_identity.rs`（442 行）
  - `DesktopIdentityResolver` —— 全仓**唯一**的 `impl IdentityResolver`。
  - `IdentityDerivationKey` —— 执行身份派生盐；`Debug` 脱敏；只从宿主主密钥派生。
  - `DelegationScopeBinding` —— 宿主侧协作者：把已校验的委托落到它被授权的执行目标上
    （端口的 `resolve_delegation` 不接收 `ExecutionTarget`，而 `ExecutionIdentity` 必须带 `connectionId`）。
  - `MonitorDelegationScope` —— 桌面实现：委托目标取自监控授权单（monitor grant）登记的 scope。
- `src-tauri/src/platform/execution_identity_tests.rs`（16 条用例，零 I/O）

**改动**：

- `src-tauri/src/platform/mod.rs` —— 注册新模块。
- `src-tauri/src/platform/adapter.rs` —— `PlatformAdapter::inject` 装配解析器（§6.2 第 4 步「环境适配」）；新增 `identity_resolver()` 取用口。
- `docs/architecture/platform/shared-boundaries-and-ports.md` —— §4.3 补「桌面实现（已落地）」语义段；§4.7 形态适配表 `IdentityResolver` 行改为已实现事实。
- `docs/architecture/platform/connection-management.md` —— §9.6 补 `executionIdentityKey` 的桌面派生语义。
- `docs/architecture/platform/system-overview.md` —— §6.4 补 `IdentityResolver` 桌面实现已落地的事实。

**未改动**（按纪律）：`packages/platform-api/src/ports/identity.rs`（端口契约一字未动）、其它
`packages/platform-api/src/ports/*`、`packages/runtime/**`、`packages/feature-core/**`、
`packages/{data-sync,data-transfer,schema-diff}/**`、`src-tauri/src/commands/**`、其它 worktree。
`hub.md` 未改（协调者台账，由协调者维护）。

---

## 2. 身份键派生方案

| 项 | 取值 |
| --- | --- |
| 派生输入 | `(organization_id, connection_id, policy_isolation_key)` |
| 服务端固定盐 | 宿主主密钥（`{appData}/.key` 或系统钥匙串）经 `HMAC-SHA256(master, "datazen.execution-identity.derivation-key.v1")` 域分离派生的子钥（HKDF-Extract 形态） |
| 消息编码 | 域标签 + 每字段 `u32` 长度前缀 + 字段内容（长度前缀避免 `("a","b\|c")` 与 `("a\|b","c")` 撞成同一条消息） |
| 输出 | `HMAC-SHA256(subkey, message)` → URL-safe base64（无填充）→ `ExecutionIdentityKey` |
| 本人路径域标签 | `datazen.execution-identity.key.v1` |
| 委托路径域标签 | `datazen.execution-identity.delegation-key.v1`（**与本人路径不同**） |
| 委托路径额外输入 | `delegation_id` + 授权作用域 `scope` |

**为什么满足三条性质**

- **稳定**：HMAC 是纯函数，输入里没有随机数、时间戳、进程内计数器；主密钥是安装级、跨进程重启不变 ⇒ 同一三元组恒得同一键（INV-02 会话连续性与池复用依赖这一点）。
- **不可伪造**：客户端拿不到主密钥就算不出身份键；身份键不是客户端可传的字段；主密钥取不到时**失败关闭**，不退回任何可预测的常量盐。
- **不含材料**：主密钥与子钥都**不进入**身份键——身份键是子钥对输入字段的摘要，摘要不可逆推主密钥，也不可逆推密码。`ExecutionIdentityKey` / `PolicyIsolationKey` 的 `Debug` 本就脱敏（`opaque_id!`）。

**委托路径为何独立**

1. **不同域标签** ⇒ 同样的字段值也派生出不同摘要 ⇒ 委托身份**不可能**等于本人身份（CM-06）。
2. **不同输入集**：委托路径额外绑定 `delegation_id` 与授权 `scope` ⇒ 换委托/换授权范围即换键，池与缓存不会把两个权限范围混用。
3. **独立实现**：`resolve_delegation` **不转调** `resolve`，它自己走「`verify_delegation` 校验 → 取授权目标 → 独立派生」。
4. **失败关闭**：校验不过、跨组织委托、授权单覆盖多连接 ⇒ 一律 `PortError::TokenInvalid`，**不降级**为调用者本人权限。
5. **本人路径拒绝委托上下文**：`resolve` 在 `ctx.is_delegated()` 时直接拒绝，否则「两条独立路径」只是两个方法名。

---

## 3. 验收标准逐条自证

| # | 标准 | 证据 |
| --- | --- | --- |
| 1 | `grep -rn "impl.*IdentityResolver for" src-tauri/src/` 有且只有一条真实实现 | `src-tauri/src/platform/execution_identity.rs:309` —— 唯一命中 |
| 2 | 5 条端口语义在桌面实现上重验（≥4 条 host 单测逐条钉住） | 15 条用例，见 §4 清单 |
| 3 | `ExecutionIdentity` 不含秘密材料（debug 字符串断言） | `执行身份不含秘密材料` + `派生盐的调试输出脱敏` |
| 4 | 不改动 `packages/platform-api/src/ports/identity.rs` 既有契约 | `git diff --stat` 无该文件；契约一字未动 |
| 5 | 门禁全绿 | 见 §5 |

### 端口 5 条语义 → 用例映射

| 端口语义 | 用例 |
| --- | --- |
| ① 身份键不得由客户端 principal 派生 | `身份键不随请求主体变化`（隔离键固定下换主体 ⇒ 键不变）、`派生入口的入参里没有主体与凭据`（结构性）、`跨组织的连接拿不到执行身份` |
| ② 同一 `(org, conn, iso)` 稳定同键 | `同一输入两次解析得到同一身份键`、`换一把解析器实例身份键仍然相同`（跨实例/跨重启）、`不同组织派生不同身份键`、`不同连接派生不同身份键`、`不同隔离键派生不同身份键` |
| ③ 轮换凭据只推进 `credential_revision` | `轮换凭据只推进版本不动身份键`（真 Store、真轮换）、`凭据版本推进时身份键不动`（替身层面，钉 `SecretProvider::revision` 通路） |
| ④ 两条独立路径 | `委托身份与本人身份不同且未授权委托失败关闭`、`委托上下文不得走本人路径`、`跨组织的委托引用被拒绝`、`两条路径的域标签不同` |
| ⑤ debug 不含 password/material | `执行身份不含秘密材料`、`派生盐的调试输出脱敏` |

---

## 4. 新增用例清单（24 条，全部在 `src-tauri/src/platform/execution_identity_tests.rs`）

首版 16 条（本轨自写）+ 复测轮补 8 条（Tester 提交 `5186fb522`，见 §9）。

1. `身份键不随请求主体变化`
2. `派生入口的入参里没有主体与凭据`
3. `同一输入两次解析得到同一身份键`
4. `换一把解析器实例身份键仍然相同`
5. `不同组织派生不同身份键`
6. `不同连接派生不同身份键`
7. `不同隔离键派生不同身份键`
8. `跨组织的连接拿不到执行身份`
9. `轮换凭据只推进版本不动身份键`
10. `凭据版本推进时身份键不动`
11. `委托身份与本人身份不同且未授权委托失败关闭`
12. `委托上下文不得走本人路径`
13. `跨组织的委托引用被拒绝`
14. `两条路径的域标签不同`
15. `执行身份不含秘密材料`
16. `派生盐的调试输出脱敏`
17. `长度前缀让字段切分不撞车`（复测轮补）
18. `委托路径的消息必然不同于本人路径`（复测轮补）
19. `跨组织委托即便策略放行也被拒`（复测轮补）
20. `委托身份键绑定委托标识与授权作用域`（复测轮补）
21. `委托路径不转调本人路径`（复测轮补，源码结构断言）
22. `桌面装配的派生盐来自宿主主密钥`（复测轮补，独立复算黄金值）
23. `身份键不含主密钥与派生盐`（复测轮补）
24. `授权单覆盖多个连接时失败关闭`（复测轮补，覆盖生产类 `MonitorDelegationScope`）

---

## 5. 门禁实测（退出码单独打印）

隔离 target：`CARGO_TARGET_DIR=/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/datazen-identity-resolver/target`（11G 热 target，本树专用）

| 命令 | 退出码 | 结论行 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | **0** | 无输出 |
| `cargo clippy -p datazen-platform-api -p datazen --all-targets` | **0** | 新文件 0 warning（仓库既有 warning 不计） |
| `cargo check -p datazen-platform-api -p datazen --all-targets` | **0** | `Finished dev profile` |
| `cargo check -p datazen --features webdriver --all-targets` | **0** | `Finished dev profile` |
| `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-platform-api --lib` | **0** | `test result: ok. 224 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` |
| `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen --lib` | **0** | `test result: ok. 1290 passed; 0 failed; 9 ignored; 0 measured; 0 filtered out` |

**基线校正（实测，非推断）**：任务简报写 host 基线 **1269**，本树实测基线是 **1266 passed; 0 failed; 9 ignored**
（`git stash push -u` 后在 `HEAD=65dac1e2c` 上实测，EXIT=0）。复测轮后的 1290 = **1266 + 16（本轨）+ 8（复测轮补）**，
既有 1266 条一条未动、零失败。platform-api 基线 224 与改后一致（该 crate 本轨未改）。

### 5.1 既有 flake：本轨一度把它从"跑不出来"推到"2/3 概率出来"，已修掉

首版用例每条都建一份真 `Store`（AppDb + HistoryDb 两份 SQLite），9 条用例 = 18 份库。那点并发 I/O
把 `platform::live_source::tests::*` 的**进程级 env 竞态**窗口撑大：那个家族的 `DesktopUserGuard`
在 Drop 时把 `DATAZEN_DESKTOP_USER` 还原成「之前没有」，会把并行跑的兄弟用例的桌面主体读歪，
于是 `DesktopPolicy::authorize` 判否、报 `connection permission denied`。

实测（基线树用 `git worktree add --detach` + 独立克隆的 target，跑完已删除）：

| 树 | 跑法 | 失败率 | 失败用例 |
| --- | --- | --- | --- |
| 基线 `65dac1e2c` | 整包 x6 | 2/6 | `native_tool::…overflow…reaped`（任务简报已列的既有 flake） |
| 基线 `65dac1e2c` | 只跑 `platform::live_source` 家族 x5 | **4/5** | live_source 家族自身互撞（与本轨无关，独立可复现） |
| 本轨首版 | 整包 x6 | **4/6** | 3× live_source + 1× native_tool |
| 本轨首版 | 整包 `--skip platform::execution_identity` x6 | **0/6** | ——（证明是本轨用例把窗口撑大的） |
| 本轨 16 条版 | 整包 x8 | 2/8 | 2× `native_tool::…overflow…reaped`（**仅**简报已列的那条） |

结论与处置：live_source 那条竞态是 `hub.md` §8 已登记的既有缺陷（Wave 4 修），但**本轨一度把它
从「整包跑不出来」推到「整包跑 2/3 概率出来」**，不能留给别人背。因此提交 `a0733c5b1` 把用例改成
零 I/O（大部分走替身；只有「凭据轮换」与「安装级稳定」两条需要真 Store，且共用同一份
`OnceCell`，全模块只建一次）。改后整包 8 次跑只剩简报已列的 `native_tool` 既有 flake
（单跑 3/3 绿），live_source 竞态不再在整包跑里出现——**失败画像与基线一致**。

**未改 `live_source_tests.rs`**（范围外，且那是 Wave 4 的项）；本轨只保证自己不再触发它。

### 5.1.1 复测轮（24 条用例）后的失败画像与基线对照

复测轮补了 8 条用例（模块 16 → 24），真 Store 仍是模块内那一份（`OnceCell`）。同一方法重测
（基线树用 `git worktree add --detach` + 独立克隆的 target，跑完已删除）：

| 树 | 跑法 | 跑次 | 失败数 | 失败集合 |
| --- | --- | --- | --- | --- |
| **本轨** `487c6dd4b` | 整包 | **36** | **3** | 全部 `platform::live_source::tests::*`：`open_databases_reports_the_driver_pool_list_verbatim` ×2、`close_database_routes_the_session_pool_demand_to_the_driver` ×1、另有 1 次同时挂该家族 3 条用例 |
| 本轨 | 整包 `--skip platform::execution_identity` | 12 | 0 | —— |
| **基线** `65dac1e2c` | 整包 | **42** | **1** | `platform::live_source::tests::open_databases_reports_the_driver_pool_list_verbatim` ×1 |

判读（实测，非推断）：

- 失败集合**只有** `platform::live_source::tests::*`，且**基线树自己也会挂**（1/42）；Tester 也独立观察到
  `--skip` 身份模块时同样出这条竞态。因此这是**既有的进程级 env 竞态**（`DesktopUserGuard` 在 Drop 时
  把 `DATAZEN_DESKTOP_USER` 还原成「之前没有」，把并行跑的兄弟用例的桌面主体读歪），
  **不是本轨引入的缺陷**。
- 本轨 3/36（≈8%）与基线 1/42（≈2%）的差距在**噪声内**（Fisher 精确检验 p≈0.3，不显著）。本轨**首版**的
  4/6（≈67%）才是真正被放大过的形态，已由 `a0733c5b1` 的零 I/O 化压回噪声带。
- **未改 `live_source_tests.rs`**（范围外，`hub.md` §8 已排期 Wave 4）。要彻底消除，修法就是台账里写的那条：
  把 env 写入收进**进程内串行守卫**（约 10 行）——协调者若要，本轨可另起一笔**独立提交**供其取舍。

### 5.2 本轨最有价值的一条教训：正常路径用例钉不住「少一个环节」的性质

复测轮 Tester 用**首版的 16 条用例**跑 5 个变异（M5 长度前缀 / M9 跨组织委托检查 /
M10 多连接失败关闭 / M11 派生盐来自主密钥 / M12 子钥域分离）——**全部全绿**。
也就是说这 5 条性质**首版一条都没钉住**。

**机理**：性质写在实现里，而用例写的是「正常路径的行为」。一旦实现里少一个环节，正常路径用例断言的**关系**往往仍然成立 ⇒ 用例照绿。

⚠️ 但「输出完全不变」**只对不在派生路径上的性质成立**。**长度前缀与域标签进 HMAC 输入**：去掉它们 ⇒ 每个正常路径调用的身份键**值**都变 ⇒ 等价于一次**全量身份键轮换**（连接池与缓存整体失效），只是**关系**不变所以用例照绿。**只有变异能暴露它。**

（复测轮 3 的 Tester 用探针实测确认了这个分类：去长度前缀 / 去子钥域标签 / 改本人域标签字面值 → 正常路径的身份键**值**都变；去跨组织委托检查 / 放宽多连接失败关闭 / 换派生盐来源 → 正常路径的**值不变**。）

| 性质 | 去掉它会怎样 | 正常路径的键**值**变吗 | 首版有用例吗 | 后补的用例 |
|---|---|---|---|---|
| 字段 `u32` 长度前缀 | `("ab","c")` 与 `("a","bc")` 拼成同一条消息 ⇒ 同一身份键 | **变**（全量轮换） | ❌ | `长度前缀让字段切分不撞车` + `没有长度前缀时两种字段切分会撞车` + `消息编码的字节布局是域标签加长度前缀加字段内容` |
| 委托路径的域标签 | 委托消息与本人消息可能同源 | **变**（全量轮换） | ❌ | `两条路径的域标签不同` + `委托路径的消息必然不同于本人路径` + `域标签的字面值与文档一致` |
| 跨组织委托的显式检查 | 跨组织委托可被策略放行 | 不变 | ❌ | `跨组织委托即便策略放行也被拒` |
| 授权单多连接失败关闭 | 跨连接授权单取第一个了事 | 不变 | ❌ | `授权单覆盖多个连接时失败关闭` |
| 派生盐来自主密钥 + 子钥域分离 | 派生链可被换成常量盐/共享域 | **变**（全量轮换） | ❌ | `桌面装配的派生盐来自宿主主密钥` + `身份键的黄金值` + `主密钥一换全部身份键都变` + `身份键不含主密钥与派生盐` |

**这不是本轨的失误，是这类实现的通病。** 处置：

1. **代码注释**：`src-tauri/src/platform/execution_identity.rs` 的模块头新增「改这个文件之前先看变异」一节，写明哪些性质钉不住、为什么、该怎么验。
2. **正式文档**：共享边界 §4.3 的 `IdentityResolver` 段新增引用块，把同一条结论落成永久事实（含 8 条补测用例的清单）。
3. **用例**：复测轮补的 8 条针对性用例（`5186fb522`）。

**方法论结论**：验收这类「组合起来才成一条性质」的实现时，**「用例全绿」不是证据**，
变异才是。评审时应把「哪些性质是用例钉的、哪些是结构性的、哪些根本没被钉住」逐条列出来。

---

## 6. 遗留项与待裁定项

| # | 项 | 说明 | 建议 |
| --- | --- | --- | --- |
| 1 | `hub.md:24` 的「无人实现的端口」清单仍列 `IdentityResolver` | 那是协调者台账 | **协调者已改**（复测轮裁决：G-01 不在本轨修复范围） |
| 2 | `resolve` 在委托上下文上一律拒绝 | 端口契约没写这条，是本实现的**收紧**选择：让「两条独立路径」不只是方法名。代价：桌面若将来要在委托上下文里走本人路径，需改这一处 | 保留；若协调者认为应当放行，请裁定后由本轨改。**已同步写入 §4.3 / 连接 §9.6 / 概要 §6.4 三份文档**（G-07） |
| 3 | 委托授权单覆盖**多个连接**时失败关闭 | 端口的 `resolve_delegation` 不接收 `ExecutionTarget`，而 `ExecutionIdentity` 只有单个 `connectionId`，一份授权单跨连接无法映射成单一身份 | 保留失败关闭；真需要跨连接委托时，端口签名要加目标或加「按连接解析」的入口——属契约变更，需概要先给语义。**已由复测轮用例 `授权单覆盖多个连接时失败关闭` 钉住** |
| 4 | `DelegationScopeBinding` 是**宿主侧**新协作者（不在 `platform-api` 端口表里） | 端口层不替实现方决定「委托的目标从哪来」；做成端口会要求所有形态都提供同一份事实，而各形态的授权单形状不同 | 保留在宿主侧；若协调者希望它进 `platform-api`，需先定跨形态的委托目标语义 |
| 5 | `MonitorDelegationScope` 与 `DesktopPolicy` 各查一次授权单 | 一次授权单读两次（`verify_delegation` + `authorized_target`） | 正确性优先，未做缓存；若成为热点再合并 |
| 6 | 派生盐取宿主 **AES 主密钥** | 同源但已做域分离（HMAC 子钥），AES 与身份派生不使用同一把密钥。**主密钥轮换 ⇒ 全部身份键变化、连接池整体失效；当前仓库无主密钥轮换路径**（`key_store.rs` 只有 load-or-create） | 保留。**该约束已写进永久文档**（共享边界 §4.3 一条 + 实现文件模块注释），不再只存在于本台账（G-05） |
| 7 | 身份解析端口已接线但**尚无调用方**（本实现目前「备而不用」） | `packages/runtime/**` 禁改，runtime 的 `PoolKeyInputs::execution_identity_key`（`packages/runtime/src/connection/types.rs:460`）仍是裸 `String`；本轨只把端口实现出来并接线，未改 runtime。**web 多租户形态要真正用上它，需要 runtime 从 `IdentityResolver` 取键** | 后续 initiative（runtime 改动本轨是禁区）。协调者已记进 Wave 4（G-09） |
| 8 | `live_source::tests::*` 的进程级 env 竞态仍在 | 既有缺陷（`hub.md` §8）。**基线树自己也会挂**（整包 1/42），本轨整包 3/36，差距在噪声内（Fisher p≈0.3）；本轨首版的 4/6 才是被放大过的形态，已由 `a0733c5b1` 压回噪声带 | 仍归 Wave 4（协调者已排期「优先级高」）。修法 = 把 env 写入收进进程内串行守卫（约 10 行）；本轨可另起独立提交供取舍 |
| 9 | 「主密钥取不到 ⇒ 失败关闭」这条性质**不可测** | `Store.encryption_key` 恒为 `[u8;32]`，`master_key_bytes` 不可能失败 ⇒ 兜底分支**不可达**，变异实测 24 条全绿 | **接受为结构性满足**：代码里**没有**任何常量盐兜底分支（不是「有兜底但测不到」，是**根本没有兜底**）。⚠️ 这条性质**没有被测试钉住**，后人若加一条「取不到就退回常量盐」的兜底，**没有任何用例会红**——改这块必须跑变异 |

---

## 7. 门禁首尾指纹

- 开工前：`HEAD=65dac1e2cf79fe7099b57988ca4ecfc7095e2ee1`，工作区 sha（`git diff HEAD` + 未跟踪文件清单）= `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`（空树）
- 第一轮收尾：`HEAD=a0733c5b13d15d781010448bed2752edeae692bb`，工作区 sha 同上（空树）
- 复测轮收尾（门禁跑完前后各记一次，两次一致）：`HEAD=487c6dd4b…`，工作区 sha = `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`（空树，`git status --porcelain` 0 条目）

---

## 8. 提交

| commit | 内容 |
| --- | --- |
| `c42f43d7f` | 实现 `IdentityResolver` 端口的桌面形态 + 接线 + 15 条用例 + 3 份文档 |
| `a0733c5b1` | 把用例改成零 I/O（修掉本轨对既有 env 竞态的放大），用例数 15 → 16 |
| `b4abf9de8` | 台账：门禁实测、基线校正、既有 flake 影响面分析 |
| `91408c59d` | **cherry-pick 自 Tester 的 `5186fb522`**：补 8 条首版没钉住的性质，用例 16 → 24（只动测试文件，生产零改动） |
| `487c6dd4b` | 复测轮 4 项要修：G-02 失败半径注释改写、G-05 主密钥轮换约束、G-06 消息编码与域标签、G-07 三份文档同口径；并把 G-04 教训落成永久事实（§4.3 引用块 + 实现文件模块头） |
