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
| B2 | workspace-apps 复制了宿主的 zip 路径校验 | 已删除 | **已关闭** | 三个符号的**定义**只剩 `platform-infra/src/path_safety.rs` 一处 —— 实测 `:26` / `:29` / `:71`，仓库其余位置 0 处；`zip_path` 字样全仓归零 | Wave-3 · W3-2，`583666808`，独立验证 PASS |
| B3 | 边界检查在规则有 violation 时仍打印 `ok` | [bug-boundary-summary-line-always-ok.md](bug-boundary-summary-line-always-ok.md) | 开放 · 低 | 变异探针日志中不再出现「`VIOLATION F-11` 与同前缀 `ok F-11` 并存」 | 归预置脚本 `179c6c8b6`，**非下沉轨引入** |
| B4 | platform-infra 声明了零使用的 `thiserror` | 已删除 | **已关闭** | 该依赖行已删 —— 实测 `packages/platform-infra/` 下 `thiserror` 0 命中，`Cargo.toml` 只剩 `regex` | Wave-3 · W3-2，`583666808`，独立验证 PASS |
| B5 | application 的模块清单守卫并不封闭 | [bug-application-module-guard-open.md](bug-application-module-guard-open.md) | 开放 · 待实测 | 新增一个 `pub mod` 能让守卫变红（先红一次以证明有效），或测试名如实描述断言强度 | 非 Track C 引入，未排期 |
| B6 | OwnerRef 契约重复定义 | [bug-owner-ref-duplication.md](bug-owner-ref-duplication.md) | 开放 · 待裁定 | `enum OwnerRef` 只剩 `platform-api/src/context.rs` 一处 | 未排期；**先定契约，再改宿主，顺序不可反** |
| B7 | schema-diff 直接依赖具体 driver crate | [bug-schema-diff-concrete-driver-deps.md](bug-schema-diff-concrete-driver-deps.md) | 开放 · 从未裁定 | `packages/schema-diff/Cargo.toml` 中不再出现任何 `datazen-driver-{mysql,postgres,sqlite}` | 未排期；动手前须先实测 `driver-api` 能力边界 |
| B8 | platform/adapter.rs 867 行，超单文件 800 行推荐值 | [bug-platform-adapter-oversized.md](bug-platform-adapter-oversized.md) | 开放 · 需专项 | 该文件 ≤ 800 行，且 `test:platform-arch` 与 `cargo test -p datazen --lib` 全绿 | 需专门变更，**不得在任何下沉轨顺手做** |
| B9 | URL userinfo 密码含 `/` 时整段凭据原样入日志 | [bug-platform-infra-url-credentials-slash-password.md](bug-platform-infra-url-credentials-slash-password.md) | 开放 · 已确认 | 新增单测覆盖 `https://user:pa/ss@host/v1`，且反证 `https://host/path@x` 不被误改写 | 宿主既有（`7c2151985` 即同正则），`platform-infra` 逐字搬运；不阻断 Wave-3 |

合计 **9 条**：进行中 0 · 开放 6 · 已关闭 3。

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

条目数、文件名与状态于集成分支 `b58f8583b` 实测（`wc -l bug-*.md` = 6 个文件，B1 / B2 / B4 三份已随关闭删除）。

**关闭次序是「先删缺陷文件、再复跑门禁」，不可颠倒。** B1 / B2 的关闭条件是全仓某个字面量归零，
而缺陷文件自身就写着那个字面量（`bug-ai-providers-redact-endpoint-duplicate.md` 3 处、
`bug-workspace-apps-zip-path-duplicate.md` 1 处），不先删则门禁永远命中自己。删完之后只剩本索引里的
「详情文件」链接与「关闭要点」两处——所以这两处也一并改写：详情列填「已删除」，关闭要点改述实测结论、
不再写出该字面量。B1 关闭后全仓 0 命中即为实测。

**本索引自身就是同类陷阱的第二个实例**：第一层是缺陷文件含被查字面量，第二层是索引的关闭要点
照抄了关闭条件。两层都要处理，只删文件不够。
