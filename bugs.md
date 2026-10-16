# 缺陷索引

**本文件只是索引，不是缺陷本体。** 症状、证据、精确锚点、修法都在每份 `bug-*.md` 里，本文件只回答四个问题：**有哪些、在哪、什么状态、归谁**。

**生命周期**：与 `hub.md` / `progress.md` 同级同性质 —— 开发期间允许入库、随分支流转，**合并回 main 时必须连同整批 `bug-*.md` 一起删除**，不得存活在 `main` 上。缺陷的永久落点是**代码、测试与 `docs/`**；台账消失后，结论必须仍能从代码里读出来，否则就是没做完。

## 状态词

| 状态 | 含义 |
| --- | --- |
| 进行中 | 已派发修复轨，等待独立验证 |
| 开放 | 已知缺陷，尚未指派或尚未排期 |
| 已关闭 | 关闭条件已在集成分支实测通过，且缺陷文件已删除 |

## 索引

全部路径相对**分支根目录**。

| # | 缺陷 | 详情文件 | 状态 | 关闭要点 | 归属 |
| --- | --- | --- | --- | --- | --- |
| B1 | ai-providers 手写了第二份更弱的日志脱敏 | 已删除 | **已关闭** | 全仓旧脱敏函数归零 —— 实测 0 命中（代码 + 文档 + 缺陷文件全查），`cargo test -p datazen-ai-providers` 90/0 | Wave-3 · W3-1，`66e97f69f`，独立验证 PASS |
| B2 | workspace-apps 复制了宿主的 zip 路径校验 | 已删除 | **已关闭** | 三个符号的**定义**只剩 `platform-infra/src/path_safety.rs` 一处 —— 实测 `:26` / `:29` / `:71`，仓库其余位置 0 处；`zip_path` 已不再是模块名，字样仅存于本行的历史叙述、文档里的曾在此处说明，以及 `install_from_zip(zip_path, ..)` 的同名形参 | Wave-3 · W3-2，`583666808`，独立验证 PASS |
| B3 | 边界检查在规则有 violation 时仍打印 `ok` | [bug-boundary-summary-line-always-ok.md](bug-boundary-summary-line-always-ok.md) | 开放 · 低 | 变异探针日志中不再出现「`VIOLATION F-11` 与同前缀 `ok F-11` 并存」 | 归预置脚本 `179c6c8b6`，**非下沉轨引入** |
| B4 | platform-infra 声明了零使用的 `thiserror` | 已删除 | **已关闭** | 该依赖行已删 —— 实测 `packages/platform-infra/` 下 `thiserror` 0 命中，`Cargo.toml` 只剩 `regex` | Wave-3 · W3-2，`583666808`，独立验证 PASS |
| B5 | application 的模块清单守卫并不封闭 | [bug-application-module-guard-open.md](bug-application-module-guard-open.md) | 开放 · 待实测 | 新增一个 `pub mod` 能让守卫变红（先红一次以证明有效），或测试名如实描述断言强度 | 非 Track C 引入，未排期 |
| B6 | ~~OwnerRef 契约重复定义~~ **判据不成立** | 已删除 | **已关闭 · 误判** | 实测 `src-tauri` **从无** `enum OwnerRef` 定义（`handles.rs:36` 是普通 import，`:192` 在 `#[cfg(test)]` 内），宿主 49 处引用**全部**已是契约类型 ⇒ 原判据作废；真问题另立 B12 | 原判据由 Wave-4 · w4-ownerref 实测推翻，协调者独立复核采信 |
| B7 | schema-diff 直接依赖具体 driver crate | [bug-schema-diff-concrete-driver-deps.md](bug-schema-diff-concrete-driver-deps.md) | 开放 · 从未裁定 | `packages/schema-diff/Cargo.toml` 中不再出现任何 `datazen-driver-{mysql,postgres,sqlite}` | 未排期；动手前须先实测 `driver-api` 能力边界 |
| B8 | platform/adapter.rs 867 行，超单文件 800 行推荐值 | [bug-platform-adapter-oversized.md](bug-platform-adapter-oversized.md) | 开放 · 需专项 | 该文件 ≤ 800 行，且 `test:platform-arch` 与 `cargo test -p datazen --lib` 全绿 | 需专门变更，**不得在任何下沉轨顺手做** |
| B9 | URL userinfo 密码含 `/` 时整段凭据原样入日志 | [bug-platform-infra-url-credentials-slash-password.md](bug-platform-infra-url-credentials-slash-password.md) | 开放 · 已确认 | 新增单测覆盖 `https://user:pa/ss@host/v1`，且反证 `https://host/path@x` 不被误改写 | 宿主既有（`7c2151985` 即同正则），`platform-infra` 逐字搬运；不阻断 Wave-3 |
| B10 | 安装测试把系统共享临时目录当成自己的沙箱 | [bug-workspace-apps-test-asserts-shared-tmpdir.md](bug-workspace-apps-test-asserts-shared-tmpdir.md) | 开放 · 已确认 | `install/tests.rs` 中针对 `TempDir` **直接父目录**的断言归零（按该文件范围检索计 0），改为断言本次测试自建的沙箱顶层；该 crate 测试全绿 | 本次迁移引入（`d57690b29` = Wave-2 · B1），**非宿主既有** |
| B11 | ai-providers 的 7 个 HTTP 出口绕过 NetworkProvider，SSRF 防护未覆盖 | [bug-ai-providers-reqwest-bypasses-network-provider.md](bug-ai-providers-reqwest-bypasses-network-provider.md) | 开放 · 待裁定 | 裸 `reqwest::Client` 出口归零，或豁免裁定写进 `shared-boundaries-and-ports.md` | 本轮裁定 (d) 方案 1：**现在不改**，另行排期 |
| B12 | 契约与内核各有一份同名 `OwnerRef`，3 个变体字段集不同且差异承重 | [bug-ownerref-contract-kernel-divergence.md](bug-ownerref-contract-kernel-divergence.md) | 开放 · **需设计裁定** | 两层形状差异被消除，**或**被明确裁定为长期允许并写进平台架构文档 | 既有设计债（三处代码注释已登记），**非下沉引入**；`OwnerRef` 在内核侧冻结中 |
| B13 | 边界门禁自身的规则范围描述过时（头写 F-01..F-08、错误信息写 F-01..F-07，实际 11 条） | [bug-boundary-guard-rule-range-stale.md](bug-boundary-guard-rule-range-stale.md) | 开放 · 低 | 文件内不再出现与 `RULES` 条数不符的写死范围，且门禁正常路径仍 0/0 | Wave-4 · w4-b3-summary 顺带发现，**既有描述债** |
| B14 | 安装测试对系统共享临时目录做**绝对零**断言（跨进程脆弱） | [bug-workspace-apps-inspect-tmpdir-absolute-assert.md](bug-workspace-apps-inspect-tmpdir-absolute-assert.md) | 开放 · 低 | `install/tests.rs` 中针对 `std::env::temp_dir()` 的绝对断言归零（全改差值形态），且该 crate 全绿、结论行连续两次逐字相同 | 既有测试债；Wave-4 · w4-test-guards 顺带发现但**未改**（超出 B10 边界）。与 B10 同类而不同位 |
| B15 | 门禁逐规则汇总行在规则产出 error 时仍打印 `ok`（B3 的 error 通道孪生） | [bug-boundary-rule-summary-ok-on-error.md](bug-boundary-rule-summary-ok-on-error.md) | 开放 · 待裁定修法 | 构造「扫到文件但扫描中途失败」的运行，汇总行与该规则 ERROR 行不再互相矛盾；正常路径仍 0/0 | 与 B3 同源，**未随 B3 修复一并解决**；现有 8 个变异探针全走 violation 通道，故不被触发 |
| B16 | 门禁脚本经符号链接路径调用时静默退出 0 且零输出（会被误读为 PASS） | [bug-gate-cli-symlink-silent-exit-0.md](bug-gate-cli-symlink-silent-exit-0.md) | 开放 · 中 | 全仓不再有 `import.meta.url === 'file://' + process.argv[1]` 形式的门禁入口；每个受影响脚本给出「经符号链接调用输出非空」的实测 | 既有仓库约定债（`check-platform-crate-boundaries.mjs:733` 同形），**非本波引入** |
| B17 | 同一列混用秒级与毫秒级时刻形状，字典序在整秒处翻转（含本波引入的提交边界粒度变更） | [bug-timestamp-shape-mixing.md](bug-timestamp-shape-mixing.md) | 开放 · 中 | 形状收口，或 `Timestamp` 比较不再依赖字符串序；且给出「同一整秒两种写法排序正确」的实测断言 | 由 `Clock` 落地**暴露**的既有债；`after_seconds` + 宿主 `expires_at` 秒级写入点在本波之前就存在 |
| B18 | `launch` 的 fail-closed 断言只校验「非空主体」，占位主体可溜过 | [bug-launch-failclosed-asserts-only-nonempty.md](bug-launch-failclosed-asserts-only-nonempty.md) | 开放 · 中 | 注入非空占位主体后该断言变红（并给出负控逐字结果） | **既有测试强度缺口，非本波引入**（identity.rs 与测试在本波均未功能改动，基线同样全绿）；与 B5 同一失效模式 |

合计 **18 条**：进行中 0 · 开放 14 · 已关闭 4。

## 维护纪律

- **一个缺陷一个文件**，文件名 `bug-<slug>.md`。索引只增删行，**不搬内容**；详情一律回原文件读。
- 状态只能由协调者改，依据必须是**实测**的关闭条件结论行，不是推理。
- 缺陷被修复轨关闭后：先独立验证，再删 `bug-*.md`，最后在本表把状态改成「已关闭」并在验收合并时移除该行。
- 新缺陷先写 `bug-*.md`（含症状 / 位置 / 证据 / 影响 / 修法 / 关闭条件），再在本表登记；**不要**只在本表里写一行。
- 归属列里的「非本次下沉引入」是**归因结论**，用于判断该由谁修，不用于降级优先级 —— 与 `hub.md`「跨轨分支上的『事实性错误』要先问『合并后是否仍然如此』再判缺陷」是同一条纪律的两面。

---

## 尚未登记的已知债

以下是**已知但还没有独立缺陷文件**的债。它们还没有单独的 `bug-*.md`，因此也不在本表里；一旦要动，先补文件再登记。

- **命名债**：`packages/runtime/src/registry/`（P4 引入）与 `driver_registry/`（Track B）命名不一致。
- **可选跟进（Option A 裁定的产物）**：把 `scripts/resolve-drivers.mjs:1350-1487`（132 行）抽成 `scripts/driver-stash-freshness.mjs`；修 `__boundaryProbe__` 竞态（`check-module-layers.test.ts` 与 `boundaryMutation.test.ts` 同写同删 `packages/ui/src/__boundaryProbe__.ts`，该文件 gitignored，表现为 `ENOENT … lstat`），做法是把探针指向临时目录。

---

条目数、文件名与状态于本次改判 B6 / 登记 B12 时实测（`ls bug-*.md` = 8 个文件；索引 12 行；B1 / B2 / B4 / B6 四份已随关闭删除）。

**关闭次序是「先删缺陷文件、再复跑门禁」，不可颠倒。** B1 / B2 的关闭条件是全仓某个字面量归零，
而缺陷文件自身就写着那个字面量（`bug-ai-providers-redact-endpoint-duplicate.md` 3 处、
`bug-workspace-apps-zip-path-duplicate.md` 1 处），不先删则门禁永远命中自己。删完之后只剩本索引里的
「详情文件」链接与「关闭要点」两处——所以这两处也一并改写：详情列填「已删除」，关闭要点改述实测结论、
不再写出该字面量。B1 关闭后全仓 0 命中即为实测。

**B6 是第一个「判据不成立」型关闭**：不是修好了，而是**原判据经实测不成立**（宿主从来就没有那份重复定义，
49 处引用早已是契约类型）。此时仍走同一纪律 —— 删缺陷文件、改写关闭要点；**真问题必须另立文件**（真问题成了 B12），
不得因为「原来那条不成立」就把它一起丢掉。

**本索引自身就是同类陷阱的第二个实例**：第一层是缺陷文件含被查字面量，第二层是索引的关闭要点
照抄了关闭条件。两层都要处理，只删文件不够。
