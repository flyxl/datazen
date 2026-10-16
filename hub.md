# Wave-1 共享层下沉 · 协调者台账

- 基准 commit：6d0a47ce9（feat/p4-f-assertion-mapping-and-lifecycle-journeys 的 HEAD，比 main f06ad7e5b 领先 4 个提交）
- 集成分支：feature/shared-layer-extraction（只建 ref，未在主检出检出）
- 主检出状态：仍在 feat/p4-f，含 13 处未提交改动（9 改 + 2 未跟踪文件等），全程未被任何轨道写入。

## 轨道矩阵

| 轨道 | worktree | 范围 | 宿主侧改动面 |
|---|---|---|---|
| sql-guard-runtime | .worktrees/datazen-sql-guard-runtime | sql_guard 779 行 + sql_main_keyword → packages/runtime/src/sql_guard/ | 9 个消费者 import + lib.rs 一行 |
| driver-cache-runtime | .worktrees/datazen-driver-cache-runtime | DriverRegistry 693 + cache 761 → packages/runtime/{driver_registry,schema_cache}/ | 仅 2 个门面 mod.rs，97 个调用点零改动 |
| frontend-bridge-port | .worktrees/datazen-frontend-bridge-port | bridge.rs 357 → packages/application/src/frontend_bridge.rs | adapter.rs 4 处 + platform/mod.rs 2 行 |

## 冲突面管理

- packages/runtime/src/lib.rs：三轨都会碰。用插入位置隔离（driver-cache 按字母序、sql-guard 放末尾）。冲突由协调者在合流时裁决。
- packages/runtime/Cargo.toml：预计零改动；若某轨报告需要加依赖，先回协调者。
- 其余文件零重叠：mcp/permission.rs 归 A，adapter.rs/platform/mod.rs 归 C，db/cache 归 B。
- 全部轨道禁用清单：packages/runtime/src/{application,registry,resource}/**、src-tauri/src/platform/{session_backend,session_commands}.rs、src-tauri/src/bootstrap/run.rs。

## 环境

- CARGO_TARGET_DIR=/tmp/dz-target-wave1（三轨共享，cargo 自带锁）。
- 驱动集已按 --drivers=all 重铺 codegen（避免 basic 与 CI 不可比）。
- 主检出 target/ 已 104G，磁盘余量 204Gi，未新建独立 target 目录。

## 状态

- Wave-1：Coder 已并行派发。
- Wave-2 候选：AI 模块全量 8,117 行（可下沉候选 protocol+providers 约 3.7k 行）、wapps manifest/bridge 契约约 1.7k 行、store/ 14,122 行。待两条裁定，见下。

## 协调者裁决（Track C 提出）

**冲突 1：`include_str!("bridge.rs")` 编译期引用** — 采纳 A，授权 T3 越界改 2 行。

- 实测：`handles.rs:402` 与 `ipc.rs:311` 以 `include_str!` 读入 bridge.rs 的生产代码，断言「platform 文件不写入 legacy shared slot」。文件一删即 E0463，与「bridge.rs 必须已删除」的验收直接互斥。
- 实测：另两轨不碰这两个文件（A 只改 `platform/session_backend/execution.rs`，B 只改 `db/` 与 `cache/`），扫描清单为本轨独占，合流无冲突。
- 授权范围仅四处：两处 `include_str!` 改指 `../../../packages/application/src/frontend_bridge.rs`、handles.rs 的 tuple label 同步改名。断言强度不减，禁止把新旧任一项移出清单。

**冲突 2：tracing 依赖** — 选 B（加依赖），推翻 T3 原打算的 A。

- 实测：`packages/driver-api/Cargo.toml` 已有 `tracing = "0.1"`，且同为纯 trait/命令 API 契约 crate，故「契约 crate 不带 tracing」在本仓不成立。
- 实测：Wave-1 仅 T3 会改 `Cargo.toml` 与 `Cargo.lock`，A/B 两轨不加依赖。T3 原先把锁文件判为三轨合流热点，属过度保守。
- 执行：application 加 `tracing = "0.1"`；两行日志留在搬过去的 `set_bridge_injection` 内。理由：挪回宿主调用方后，server 形态直接调 application 的注入函数时该 info 会静默消失——用观测性换一个不存在的冲突不划算。

**附带（不在本轨裁定）**：`handles.rs:192` 的 `OwnerRef::Job` 与 `packages/platform-api/src/context.rs` 的 `OwnerRef` 契约重复，留待集成分支单独裁定。

## 集成后债务（不得遗忘，不得在任一轨道顺手做）

- **`src-tauri/src/platform/adapter.rs` 867 行**（基线 849 行即已超 800 推荐值，Track C 净 +18）。拆分它会牵动 `handles.rs`/`ipc.rs` 的 `include_str!` 扫描白名单与 `bootstrap/app_state.rs`，与各轨「小冲突面」要求正面冲突，因此 Track C 不拆。须在集成分支上作为**专门变更**处理。
- **`packages/application/src/lib.rs` 的模块清单守卫其实不封闭**（待验证方实测确认）：测试名声称 exactly the seven，实现却是「7 项必须存在 + 4 个特定名字必须不存在」。新增任意第 8 个 `pub mod` 不会让它变红。黑名单四项是迁移前既有设计，**不是 Track C 缺陷**；若实测确认，应作为独立缺陷排期。
- **`src-tauri/src/platform/handles.rs:192` 的 `OwnerRef::Job` 与 `packages/platform-api/src/context.rs` 的 `OwnerRef` 契约重复**：Wave-1 只记录不改，待集成分支单独裁定。
- **`docs/development/platform-development-plan.md` §4.7 vs :276 的 ProfileRepository 口径冲突**：见下节裁定项 2。

## Track B 偏差签字（协调者裁定，HEAD = 7c7b6b34）

三条偏差全部接受，但都是**带条件的接受**——条件是验证方必须独立复验，不是采信自证。

- **偏差 1「dev-dependencies 新增 `datazen-runtime` + `test-harness`」：接受。** 已核实 `src-tauri/Cargo.toml:25` 在 `[dependencies]` 中**本已存在** `datazen-runtime = { path = "../packages/runtime" }`。所以这不是新增依赖边，只是给一条既有依赖在 dev 域追加 feature；resolver = "2" 下 dev-dep feature 不进入 normal/build，「生产依赖零新增」成立。**但这是本轨唯一触碰分层契约的动作**，因此验证方必须实测 `cargo tree -e features --edges normal,build` 无 `test-harness`、`--edges dev` 才有，并确认真实生产构建不链接 `connection::testing`。
- **偏差 2「census 用例 `every_linked_factory_resolves_or_reports_why_not` 留在宿主门面」：接受。** 它断言的对象是「**本二进制里链进来的**每个 inventory factory」，而 factory 只在宿主 dev-deps 上链入，`packages/runtime` 链不到；搬过去不但必红，断言集合还会变空——变成一个永远通过的空断言。
- **偏差 3「门面除具名项外同时 re-export 底层模块路径」：接受。** 这是保住 14 个 consumer 零改动的唯一办法。协调者已独立复核：`pub struct DriverRegistry` 与 `pub struct SchemaCache` 在全仓**各只有一处定义**（均在 `packages/runtime/`），证明门面是真 re-export 而非复制。代价：门面是临时物，宿主连接层收敛后必须删除。

## Track C 验收：PASS（独立验证方 HEAD = `985e5c65`，merge-base = `6d0a47ce9`）

验证方报告七道门禁**全部实跑、全部绿**，且实测数字与提交方声称**逐字一致**：application 108/0/0、host 1804/0/6、platform-arch 0 violation / 0 error / 3 advisory、platform-crates 7 core crates、fmt 0 行 diff、typecheck exit 0、vitest 601 files / 6315 tests。七道门禁 PRE/POST HEAD 全等 `985e5c65`、PRE/POST `git status --short` 全空，终态仍为 0 行。

**协调者补充的独立证据（验证方未覆盖，不能由它的 PASS 推导）：**
- 第二个提交 `5022280af` → `985e5c65` 的全量 patch = **1 文件 +1/−4**，恰为授权的 3 行 `///` 删除 + 测试改名 six→seven，**无夹带**。
- `git diff --name-status -M` 确认 `R080 src-tauri/src/platform/bridge.rs → packages/application/src/frontend_bridge.rs`。
- 跨轨重叠实测：`packages/runtime/src/lib.rs` **不在 C 的 diff 内** ⇒ C 不与 A/B 争该文件。
- **此前未记账的依赖面**：C 动了 `packages/application/Cargo.toml` 与 `Cargo.lock`，为随桥同迁的 `tracing::warn!/info!` 补 `tracing` 依赖。`cargo tree -p datazen-application --depth 1 -e normal` = 6 项直连，且 platform-arch 0 violation，故不违反分层契约；记录在案是因为「C 未碰依赖面」这句话此前是不准确的。

**两条观察项，均判非本轨责任、不阻断：**
- **OBS-1（迁移前既有，低）**：模块清单测试名与注释都称清单「封闭」，实测**不封闭**——验证方做了变异实验：建最小桩文件后新增第 8 个 `pub mod __dz_probe;`，该测试仍报 `1 passed` 变绿。白名单 7 项 + 黑名单 4 项，任何**不叫那 4 个名字**的第 8 个模块静默放行。协调者已比对 base `6d0a47ce9:packages/application/src/lib.rs:139` 的 `the_module_list_is_exactly_the_six_documented_units` **结构完全同构**（同样白名单、同样 4 项黑名单、同一句「封闭」注释），故属迁移前既有设计。处置：**不记本轨欠账**；作为独立改进项另开（断言改为「解析全部 `pub mod X;` 行、集合恰等于这 N 个」，或给 `scripts/check-platform-crate-boundaries.mjs` 加一条边界规则）。
- **OBS-2（低）**：`src-tauri/src/platform/{handles,ipc}.rs` 的测试改用 `include_str!("../../../packages/application/src/frontend_bridge.rs")`，把宿主 crate 的测试耦合到**另一个 crate 的仓库布局**。非发布型二进制，风险有限，记录在案。

**集成清理清单新增**：`docs/architecture/platform/shared-boundaries-and-ports.md` 的 mermaid `SLOT --> IMPL` 箭头方向读反（实现是被注入**进**槽，语义应为 `IMPL --> SLOT`）。验证方提出，非阻断，一处改写即可。

## 验证树环境事实（两条，集成树同样适用）

1. **`pnpm run` 在验证 worktree 里必须加 `--config.verify-deps-before-run=false`。** 这些 worktree 的 `node_modules` 是指向主检出的**符号链接**，pnpm 12.6.0 在 `run` 前自动做依赖校验，报 `workspace hoist directory is not a real directory` 并 exit 1。**这不是代码缺陷——判定任何 exit 1 之前先排除这条。**
2. **`src/locales/builtinLocales.ts` 是 gitignored codegen 产物，新 worktree 必须先 `node scripts/generate-builtin-locales.mjs`（exit 0）。** 缺失时 `npx vitest run` 报 `Failed to resolve import "./builtinLocales" from "src/locales/index.ts"`，**145 个测试文件 / 122 个用例失败**——看着像全线崩了，实际只少一个 codegen 文件。集成树跑 vitest 前必须先执行这一步。

## Track B 补强：三个迁移文件逐字节比对（协调者自验，HEAD = `7c7b6b34`）

先前给 Track B 的签字挂了三条条件，其中最弱的一条是「没有逐行证明 SchemaCache 语义未变」。
本节以 `diff -u` 直接比对 base `6d0a47ce9` 的宿主文件与 B 提交的 runtime 文件，该条件**解除**：

| 文件 | base 行数 → B 行数 | 实际改动 |
|---|---|---|
| `src-tauri/src/cache/schema_mutate.rs` → `schema_cache/schema_mutate.rs` | 75 → 75 | **0 行**（完全一致） |
| `src-tauri/src/cache/schema_cache.rs` → `schema_cache/schema_cache.rs` | 683 → 683 | **10 行**，全部是 `use` 路径 |
| `src-tauri/src/db/registry.rs` → `driver_registry/registry.rs` | 681 → 612 | 73 行 = 1 处 import 排序 + 移出 69 行普查测试 |

**那 69 行没有丢**：`every_linked_factory_resolves_or_reports_why_not` 连同其完整文档注释
被逐字搬进宿主门面 `src-tauri/src/db/mod.rs`（12 → 97 行）。留在宿主是正确取舍——该测试遍历
`iter_driver_factories()` 的宿主链接结果，不是 runtime 单元。门面 re-export 增加
`ResourceProviderLookup` 正是为此测试所需。

**`#[allow(dead_code)]` 不是本次新增**：与 base 的属性集合逐项相同（`CachedSchema` /
`db_version` / `registry` 字段三处 base 即已存在）。`self.registry` 在 base 与 B 中均无读取点，
即该字段自迁移前就是「只存不读」，不存在迁移中丢失使用点的情况。此前把这一点列为可疑项，经查证排除。

## Track A 修复提交复核 + 独立验证已派发（HEAD = `d6b9eb644`）

**修复提交由协调者独立复核**（不采信 Coder 自述）：parent = `7f5c3db8`，即**新增提交、未改写历史**
（无 amend / rebase / squash）。改动 3 文件 +7/−5，逐行核对恰为：`keyword.rs` 的定义改名 +
`tokenize_sql` 内调用改名、`sql_guard/mod.rs` 顶层 re-export、宿主 `mcp/permission.rs` 的 import 与
一处调用。改名后 `strip_comments_for_tokenize` 只在 `keyword.rs`、`strip_sql_comments` 只在
`safety.rs:33`，全仓不再有同名不同语义的一对。`6d0a47ce9..d6b9eb644` 范围内
`Cargo.toml` / `Cargo.lock` **零改动**，告警集合 52→52 逐字一致。

**独立验证已派发**：全新 Tester（非 Coder），detached 于 `.worktrees/verify-sql-guard-runtime` @
`d6b9eb644`。该树已 bootstrap（node_modules 符号链接、`generate-builtin-locales.mjs` exit 0、
`resolve-drivers.mjs` exit 0、pro-extensions 已复制）。

- **已知基线偏移，不是缺陷**：该树 `git status --short` 会显示 `M src-tauri/Cargo.toml`，
  那是 `resolve-drivers.mjs` 注入驱动 feature 的 codegen 产物；判定工作区是否被污染须排除这一条。

## 冲突面预测的一处自我更正（不得再引用旧结论）

先前记入本台账的「三对分支 `git merge-tree` 冲突标记数 = 0」**不构成有效证据**：该探针从未被验证过能识别冲突。
我两次尝试给探针做自测，两次都不成立——第一次重跑的是同一对 A×B，第二次用 B 对 `B~1`（快进，根本不可能冲突）。

**可用的证据只有这些**：

- 文件重叠面（`comm -12`）：A×B 恰 1 个文件（`packages/runtime/src/lib.rs`），A∩C = ∅，B∩C = ∅。
- 改动锚点互不相干：A 在 `tunnel` 之后追加 `pub mod sql_guard;`（:38）；B 在 `directory` 之后插入
  `driver_registry`（:21）、在 `resource` 之后插入 `schema_cache`（:35）。
- 三者合并后 `pub mod` 总数应为 14（base 11 + A 1 + B 2）。

**最终裁定权在集成**：真合并后 `cargo fmt --all -- --check` 与全部门禁在集成树实跑。预测不替代验证。

## 集成基线漂移（协调者实测，动集成树前必读）

**主 worktree 现状**：已切到 `main` @ `f06ad7e5b`，`git status --short` **为空**。
先前记录的「9 个已改 + 2 个未跟踪」早已进历史（`4addf2ae3` / `c9a6d3637` / `71aa7e3d1` /
`6d0a47ce9` 四个提交），该风险点**作废**。`main` 落后集成基线 4 个提交——与 Wave-1 无关，**不动它**。

**P4 分支已前移**：`feat/p4-f-assertion-mapping-and-lifecycle-journeys` 现为 `b3faa0e12`，
超出集成基线 3 个提交（新增 CM-33 只读池/预算观测口、表格并发与池计数旅程、CM-33 收口文档）。
集成基线 `6d0a47ce9` 因此**已落后**。

**漂移不影响 Wave-1 的合并结论**（实测，非推断）：

- P4 的 13 个改动文件与 A、B、C 三条轨道的文件重叠**各为 0**。
- P4 的改动清单**不含 `packages/runtime/src/lib.rs`**，即三路之间唯一的那处重叠面未被触及。
- P4 触及的 `packages/runtime/src/{registry,resource,application}/**`、
  `src-tauri/src/platform/{session_backend,session_commands}.rs`、
  `src-tauri/src/bootstrap/run.rs` 正是 Wave-1 全局禁用清单里为 P4 预留的那批路径，两边没有踩脚。

**结论**：合并前应先把集成分支 `--ff-only` 到 `b3faa0e12`（该分支无自有提交，快进安全），
而不是先合 A/B 再回头处理 P4。**但必须在 A、B 的验证结论到手之后才动集成树。**

**命名提醒（非缺陷，列集成后债务）**：P4 的 `packages/runtime/src/registry/` 是**会话**登记表，
Track B 的 `packages/runtime/src/driver_registry/` 是**驱动**登记表；同一 crate 内两处注释都用
「登记表」二字，中文语境下易混。Coder 选 `driver_registry` 而非复用已占用的 `registry`，
恰好避开了类型撞名——合并后 `pub struct DriverRegistry` 全仓唯一。

## 全仓失效路径字面量：提交方漏报部分（协调者补扫）

`git grep` 全仓命中 7 处、5 类，**提交方只报了第 1 类**。已交验证方独立复扫并出完整清单。

| # | 位置 | 性质 | 裁定 |
|---|---|---|---|
| 1 | `docs/architecture/platform/driver-capability-migration.md:13,331,682` | 本轨迁移致失效 | **选项 B（确认）**。该文头部自证是带基线的证据快照（明写「基线：`8592b0fe1`」「宿主侧只读 `src-tauri/src/db/registry.rs`」、`§0`「本文的每一格『当前』结论都来自对代码的逐项核对」）。改路径并重标行号会毁掉它的证据价值。改为**在文首既有的基线说明块内追加一行**：「此后宿主注册表已迁至 `packages/runtime/src/driver_registry/registry.rs`，文中行号仍指基线」。 |
| 2 | `packages/driver-api/src/capabilities.rs:707` | **活契约 crate 的 doc 注释** | 集成分支清理。原句称 host gate 住在 `src-tauri/src/db/registry.rs`，迁移后定位已不准。 |
| 3 | `packages/drivers/postgres/src/resource/tests_behaviour.rs:200`、`packages/drivers/redis/src/resource/tests_behaviour.rs:492` | 驱动 crate 内测试注释 | 集成分支清理。 |
| 4 | `scripts/check-driver-protocol-release.mjs:46,489` | 脚本注释 + **会打到 CI 输出的开发者可见错误串** | 集成分支清理。Wave-1 不动 `scripts/**`，按约定由轨道上报、由集成分支改。 |
| 5 | `docs/blogs/diagrams/datazen-driver-runtime-architecture.html:6421,6423`、`diagrams/src/datazen-driver-runtime-architecture.json:129` | 已发布博客产物 | **保留不改**。HTML 内是 commit 固定的 GitHub 永久链接，永远可达；改它反而破坏保真度。 |
| 6 | `docs/architecture/backend/cache.md:621` | 迁移前既有失效 | 集成分支**重写该节**，不是一行能修的：它引用 `cache/invalidation.rs` 与 `InvalidationStrategy`，而 `git grep 'pub struct InvalidationStrategy'` 全仓零命中、该文件也不存在。须按实际失效机制重写。 |
| 7 | `docs/architecture/backend/drivers.md:25` 写 `PROTOCOL_VERSION = 4`，实际 `packages/driver-api/src/lib.rs` 为 `6` | 迁移前既有陈旧事实 | 集成分支一行订正。 |

以上 7 处**均不阻断 Wave-1**：全为注释 / 文档 / 字符串，零运行时行为影响，且没有一处落在任一轨道的可写范围内。全部转入集成分支清理清单。

## 用户裁定（Wave-2 范围已解锁，原「待裁定」两项作废）

**1. P7 Web shell 含 AI 面板 → 选项 B（含，Wave-2 并行下沉 protocol + providers）。**

后果，不是备注：
- Wave-2 的 AI 轨从「可后置」升为**并行必做**，且**不能只落 ai-api 契约**。`src/components/ai/AiChatPanel.tsx` / `DiagnosisPanel.tsx` 走的是 `src/commands/ai*` 的 Tauri IPC；要进 Web shell 必须先有非 Tauri 传输路径，选项 (C) 被此裁定排除。
- 该传输路径必须复用既有分层方向（`platform-api` → `application` → `runtime` → `backend-client`），**不得为 Web shell 单开旁路**。它与 Track C 刚建立的 `frontend_bridge` 端口是同一类问题的两种形态，AI 下沉时若绕过 `backend-client` 层，等于在 Wave-1 刚清干净的分层契约上重新开口子。
- 规模实测：AI 协议 + providers 约 3.7k / 8,117 LOC。

**2. ProfileRepository 口径 → 选项 A（语义同一 + 双适配器，改 §4.7 措辞）。**

后果：
- `docs/architecture/platform/shared-boundaries-and-ports.md` §4.7:490 的「同一实现，服务端数据库」要改写为**同一 trait 契约、两套适配器**：桌面 `DesktopProfiles`（`src-tauri/src/platform/repositories.rs:136`，372 行）留在宿主，服务端另走 SQLite 适配器。契约位置不动：`packages/platform-api/src/ports/profile.rs:147`。
- `store/`（**14,122 行**，AES-256-GCM + 系统钥匙串主密钥，`DATAZEN_KEYRING=file` 时会写 `{appData}/.key` 明文）**不下沉**——这正是选 A 的关键收益，也避免把桌面端 crypto 语义拖进共享层。
- 因此原 Wave-2 候选里的 `store/` 一轨**取消**，Wave-2 只剩 AI 轨与 wapps 轨（wapps manifest/install/bridge，约 1.7k LOC）。

**未受本裁定影响**：Track C 的 `handles.rs:192` `OwnerRef::Job` 与 `packages/platform-api/src/context.rs` 重复，仍留在「集成后债务」，见上节。
## Track B 验收：PASS（独立验证方 + 协调者链接级复核）

独立验证方（`3ff90ec2`，detached @ `7c7b6b34`）7/7 门禁**全部实跑**，A–H 全 PASS，无未验证项。
三条值得留存的强证据：

- **test-harness 不泄漏**：`cargo tree -e features --edges normal,build` 中 `test-harness` 出现 **0** 次；
  另有链接级铁证——`nm -a` 产出 6,028 个 `datazen_runtime` 符号作阳性对照（证明探针非空转），
  9 个受 test-harness 控制的符号在 `nm` 与 `strings` 双探针下**全部 0 命中**。
- **门面是真再导出**：注释掉 `db/mod.rs:18` 后 `cargo check -p datazen` 以 `E0432` 失败（exit 101），
  还原后 sha256 与变异前逐字节相同。
- **glob 再导出无遮蔽**：验证方没有止步于 `ambiguous_glob_reexports` 为空（该 lint 查不出「具名 pub use
  被 glob 静默遮蔽」），而是枚举 `packages/driver-api` 的 301 个公开名，确认门面再导出的 5 个名字
  **各自零命中**，集合不相交。**这条方法论值得复用。**

### D1：本轨道新引入的编译告警（已复核，修法与验证方所提不同）

`src-tauri/src/cache/mod.rs:12:55` `warning: unused import: schema_mutate`。
机理：`src-tauri/src/lib.rs:6` 是 `mod cache;`（**私有**），其中的 `pub use` 仅 crate 内可见，
触发 `unused_imports`；基线是 `pub mod schema_mutate;`，模块声明从不触发该 lint，故为迁移新引入。

**协调者链接级复现**：`touch` 后 `cargo build -p datazen` → `EXIT=0`，`unused import` 计 **16** 条，
其中落到本轨道改动文件的**仅此 1 条**（新增的 runtime 文件产出 0 条）。CI 无 `-D warnings` 门禁，故非阻断。

**验证方给的修法照抄会炸**：它建议「把 `schema_mutate` 从 L12 再导出列表去掉」。实测删掉**整行**后
`cargo build -p datazen` 直接 `EXIT=101`——同一行还带着 `schema_cache`，而它**有消费方**：
全仓唯一引用是 `src-tauri/src/services/schema_metadata/columns.rs:2` 的
`crate::cache::schema_cache::CachedColumns`。

**正确修法**（协调者已实测）：`pub use datazen_runtime::schema_cache::{schema_cache, schema_mutate};`
→ `pub use datazen_runtime::schema_cache::schema_cache;`，只摘一个名字。实测 `EXIT=0`、
`unused import` 16 → **15**、0 条 `schema_mutate` 告警、0 error。并须同步改写该文件第 8 行文档注释
（现称「`schema_cache` / `schema_mutate` 模块与具名项原样转出」，删后即失准——**文档与代码不一致本身是缺陷**）。
修复轮已派新实例 `a2446165`（原 Coder 的 id 从未记录，无法续用）。

### 验证方指出的三处提交方口径偏差（报告问题，非代码缺陷）

1. 基线 `db/registry.rs` 实为 **8** 个测试而非 7；`7+10+3=20` 实为 **HEAD runtime** 的计数，归属说反了
   （守恒结论仍精确成立：宿主 −20 / runtime +20）。
2. 去重后消费方 **17** 个而非 14（`crate::db::registry` 11 个 + `crate::cache` 8 个）。
3. 其结构化计数漏掉 `#[tokio::test(start_paused = true)]`——以真实运行计数 509 → 529 = +20 为准。

## Track A 验证方崩溃与改派

`1ba4cb9b` **中途崩溃，未留下任何结论**（无 closing message）。按协议不得退回作者本人自验，
已改派全新实例 `31a4e36d`。

崩溃后现场核查（结论：**无变异残留，树可直接复用**）：

- HEAD = `d6b9eb644835609cf3b127d2d8d8e35f6f89dfd6`，detached，正确。
- 唯一脏项 `M src-tauri/Cargo.toml`（+9/−1）是 `resolve-drivers.mjs` 注入驱动 feature 的 **codegen 产物**
  （环境事实 3），非验证方遗留。
- `src/locales/builtinLocales.ts`、`src-tauri/src/driver_init.rs` 均**已存在**，无需重新 bootstrap。
- 作者树 `.worktrees/datazen-sql-guard-runtime` 仅未跟踪 `progress.md`，属正常。

新实例要求过程结论**持续写入 `/tmp/dz-trackA-report.md`**，即使再次崩溃也不丢证据。
## Track B 收口：D1 已修，轨道关闭（HEAD = `4b068ec99c`）

修复轮由新实例 `a2446165` 完成（Track B 原 Coder 的 id 从未记录，无法续用）。提交 `4b068ec99`
`fix(cache): 清除迁移引入的 unused re-export 告警`，`HEAD~1` = `7c7b6b345` **完好保留**，
baseline `6d0a47ce9`，轨道共 **2 个提交**，无 amend/rebase/squash。

**协调者复核**：`git diff --name-status HEAD~1..HEAD` = 仅 `M src-tauri/src/cache/mod.rs`，
`1 file changed, 3 insertions(+), 3 deletions(-)`。提交内容与我此前**已在链接级验证过**的变体逐字比对：

- 第 12 行（`pub use ... schema_cache;`）——**逐字节一致**，即那道我实测过
  `EXIT=0` / unused_import 16→15 / 0 schema_mutate 告警 / 0 error 的改法，**验证结论可直接平移**。
- 第 8–9 行——比我的验证变体多出的部分，**正是任务书要求的文档注释同步修正**：删掉 `schema_mutate`
  后原句「`schema_cache` / `schema_mutate` 模块与具名项原样转出」已失准，改为只描述
  `schema_cache` 模块与具名项 `sql_may_mutate_schema` / `SchemaCache`。提交版该文件中
  `schema_mutate` 出现 **0** 次，文档与代码一致。

4 道门禁由提交方实跑且数字自洽：`cargo build -p datazen` EXIT=0 且 unused_import 16→15；
`cargo test -p datazen --lib` **1791 passed / 0 failed / 6 ignored**；`cargo test -p datazen-runtime --lib`
**529 passed / 0 failed**；`cargo fmt --all -- --check` 输出 **0** 行。
整轨 diff 对 baseline 仍为 **12 files / +141 / −83**（本次修复净增删为 0，与修复前一致，自洽）。

注意：`//! ` 文档注释不含代码块，不产生 doctest；`cargo test --lib` 不跑 doctest，二者均无需额外门禁。

**Track B 至此关闭。** 遗留：其 worktree 的未跟踪 `progress.md` 按纪律在合并收尾时删除。

## 集阻断 G2：40 条 E0599（根因是环境，不是 Track B 的设计）

**症状**（协调者在集成树实跑）：`cargo test -p datazen --lib` EXIT=101，
`error[E0599]` **40 条完全同构**——`Arc<datazen_runtime::driver_registry::DriverRegistry>`
上找不到 `register_test_driver` / `register_test_factory` /
`register_test_driver_with_capabilities`。调用点分布在 6 个宿主测试文件
（`services/connection_manager/tests/cm04_baseline.rs:250`、
`commands/cm73_baseline_tests.rs:397`、`commands/cm73_idle_eviction_tests.rs:309`、
`commands/sync/jobs_contract.rs:102`、`services/connection_manager/tests.rs`、
`services/db_tools.rs:309`）。

**第一个假设是错的**：曾判为 `resolve-drivers.mjs` 的标记块注入把 Track B 写进
`[dev-dependencies]` 的内容吞了。已实测排除——两个标记 `# <<driver-dependencies>>` /
`# <</driver-dependencies>>` 同在 `src-tauri/Cargo.toml:69/:70`（`[dependencies]` 内，
见 :17 起），`replaceMarkerBlock` 实现（`scripts/resolve-drivers.mjs:1161-1171`）是非贪婪的，
且脚本全文**没有 `dev-dependencies` 这个字面量**，不会去删它。

**真实机制**：`scripts/driver-file-stash.mjs` 在注入前把被托管文件存进
`.driver-file-stash/`（gitignored，`.gitignore:61`），`injectCargoToml`
（`scripts/resolve-drivers.mjs:1180`）随后经 `managedReadPath(relPath)` **从 stash 读基线**。
集成树 bootstrap 时 `resolve-drivers.mjs` 跑在**合并 Track B 之前**，于是把 pre-B 的
`src-tauri/Cargo.toml` 冻成了注入基线；此后每次重跑都从这个陈旧基线重新注入，
Track B 的 `[dev-dependencies]` 四行（现 `src-tauri/Cargo.toml:117-120`）被反复抹掉。
**一次"代码生成"把一次真实合并静默回滚了。**

**修法**：`rm -rf .driver-file-stash` → `git checkout -- src-tauri/Cargo.toml` →
重跑 `node scripts/resolve-drivers.mjs`（EXIT=0）。此后 dev-dep 计数为 1，新建的 stash
与 `HEAD:src-tauri/Cargo.toml` 逐字节相同（diff 退出 0）。G2 随即 EXIT=0。

**归因更正（本条上一版结论错误，已撤回）**：曾记为 `stashManagedFiles()`
（`scripts/driver-file-stash.mjs:122-129`）「守卫形同虚设」。该段守卫逻辑本身**正确**，
抛错分支完备，从未出错。真实缺陷是它**根本没被调用**：修复前的
`scripts/resolve-drivers.mjs:1378` 在 stash 已存在时短路为
`if (allStashed()) { console.log(...) }`，直接跳过 `stashManagedFiles()`。
所以不是「守卫失效」而是「守卫未执行」。已由 Option A 修复并独立验证通过，
详见下方「Option A 合并」。CI 与全新 clone 不受影响（无陈旧 stash）。

### 连带作废：两个验证方的测试数字

Track B 报的 `1791 passed` 与 Track C 报的 `1804 passed` 都是在被污染的树上量的，
**一并作废**。正确基数不是 STEP 0 的 `6d0a47ce9`，而是 **P4 `b3faa0e12`**
（P4 给 `packages/runtime` 贡献 **+6** 个 test 属性，1024 → 1030）。

修正后的账（全部实跑自洽）：

| crate | 算式 | 结果 |
| --- | --- | --- |
| `datazen`（宿主） | 1811 − 74(A) − 20(B) − 7(C) | **1710** ✓ |
| `packages/runtime` | 509 + 6(P4) + 74(A) + 20(B) | **609** ✓ |
| `packages/application` | 101 + 7(C) | **108** ✓ |
| runtime test 属性普查 | 1030(P4) + 74 + 20 | **1124**（= 1030+74+20，逐项相等）✓ |

## 环境事实（补充三条，续上文「验证树环境事实」）

**（五）陈旧 `.driver-file-stash/` 会静默回滚合并后的清单改动。** 这是本次唯一的
集阻断。触发条件：bootstrap 的代码生成早于一次会改被托管文件的合并。判别信号是
「代码错误地指向一个**本该存在却不存在**的 API」，且**只影响 merge 之后**。
恢复动作：删 stash + `git checkout --` 还原 + 重跑 `resolve-drivers.mjs`。

**（六）`test:platform-arch` 的 advisory 条数不是稳定门禁数字。** 它取决于
`resolve-drivers.mjs` 注入了哪些驱动；`driver-redis` 被注入时 `tauri-plugin` 进入闭包，
稳定产出 2 条 advisory（P4 基线副本与集成分支实跑逐条相同）。**只有 violations / errors
（0 / 0）是稳定判据**，advisory 条数不可用于跨树比对。

**（七）真实验收基线是 P4 `b3faa0e12`，不是 `6d0a47ce9`。** 任何"合并后测试数应当是多少"
的推算都必须以 P4 为起点，否则差 6。

## 文档债务清理提交 `d40a96f5d`（集成树，15 files / +55 / −89）

把 Wave-1 造成的失效引用与两处比"路径过期"更严重的内容失真一并收口：

- **失效路径改正**：`capabilities.rs:707`、两个 `tests_behaviour.rs`、
  `live_write_and_ddl.rs`、`check-driver-protocol-release.mjs:46,489`；
  `AGENTS.md` 删掉已不存在的 `sql_guard/`、把 `db/` 与 `cache/` 标为 re-export 门面、
  关键模块表指向 `packages/runtime/src/sql_guard/`、补齐目录树缺失的四层、
  删除**根本不存在**的 `packages/themes/`（实测 `ls` 失败、`git ls-files` 计数 0）。
- **PROTOCOL_VERSION 断言错误**：实测真值 `packages/driver-api/src/lib.rs:114` 为 **6**
  （:120 的 MIN 为 1），而 6 处文档 / 注释仍写 4 或 3，已全部改正。
  `driver-capability-migration.md` 是钉在基线 `8592b0fe1` 的证据快照，
  **只在其基线说明块追加一条后补注记**，被引用的代码块与行号一律不动。
- **内容失真（本次实质修复）**：`backend/cache.md` §1.5 整节重写。原节描述的
  `InvalidationStrategy` / `DdlDetector` / `cache/invalidation.rs` **在仓库中根本不存在**，
  属凭空叙述。现按代码写成三个彼此独立的机制（读时 TTL / 容量淘汰 / 事件驱动失效），
  给出三个失效入口的真实调用点、`scope_key` 分片方式与 `generation` 计数构成的并发屏障，
  并如实记录代价：DDL 走 `clear_connection` 清整个连接，per-table 的
  `invalidate(.., Some(table))` 在生产中**零调用**（仅单测）。
- **共享层文档口径**：§4.7 的 `ProfileRepository` 行按已定口径改为
  「同一端口、另一适配器」；前端桥注入图方向由 `SLOT --> IMPL` 改为 `IMPL --> SLOT`。

已核实**正确故不动**：`backend/ai.md:14`（`AI_PROTOCOL_VERSION` 当前 1 / 最低 1）、
`src/extensions/generated.ts:41`（`DRIVER_PROTOCOL_VERSION = 1`，是另一枚 TS 常量）。
**有意保留**：`docs/blogs/diagrams/*.html|json`（提交固定 permalink）、
`AGENTS.md:301,302` 与 `driver-api/README.md:50` 等泛指表述、
以及 `packages/runtime/src/lib.rs:20` 与 `driver_registry/mod.rs:3` 两处
**故意引用旧宿主路径作出处标注**。

## 最终合并门禁（HEAD = `d40a96f5d`，两链首尾 HEAD 与工作区 sha 一致）

Rust 链（`CARGO_TARGET_DIR=/tmp/dz-target-int`）：

| 门禁 | EXIT | 结论 |
| --- | --- | --- |
| G1 `cargo build -p datazen` | 0 | `error[E` = 0；`schema_mutate` 提及 **0** 次 |
| G2 `cargo test -p datazen --lib` | 0 | **1710 passed / 0 failed / 6 ignored** |
| G3 `cargo test -p datazen-runtime` | 0 | **609 passed** + 40 个集成测试二进制全绿 |
| G4 `cargo test -p datazen-application` | 0 | **108 passed** / 0 failed |
| G5 `cargo fmt --all -- --check` | 0 | 输出 **0** 行 |

JS 链：

| 门禁 | EXIT | 结论 |
| --- | --- | --- |
| J1 `test:platform-arch` | 0 | 26 members / 7 rules / 27 crates / 0 violations / **0 errors** / 2 advisories |
| J2 `test:platform-crates` | 0 | 7 core crates，内建 `60 passed` |
| J3 `typecheck` | 0 | `error TS` **0** 条 |
| J4 `npx vitest run` | 0 | **601 files / 6315 tests**，0 FAIL |

两链 `HEAD_START = HEAD_END = d40a96f5d857939b3f89e70a798c15bfe57b10d9`，
`WS_START = WS_END = caefffdf550221cb`，运行期间无人改动。
G1 的 60 行 `^warning` 以各 crate 末尾的汇总行为主，逐条去重后全是宿主既有
`unused import`；已用 `git show d40a96f5d --unified=0 -- '*..rs'` 证明本次提交改动的
14 行 `.rs` **全部是 `///` / `//!` 注释正文**（12 行注释行 + 2 行同一段注释的折行续行），
**零可执行代码**，故不可能引入新告警。

## 待裁定（交用户决策，附位置与实测数字）

1. ~~**是否加固 `stashManagedFiles()`**~~ — **已结**，2026-10-16。
   选项 A（复用点设闸、保留 stash 语义）已实现、已独立验证 PASS 并合并为 `7f32ba2a6`。
   选项 B（不再依赖 stash）未采纳：会推翻 stash 的备份用途。
   真正的缺陷点也一并更正为 `resolve-drivers.mjs:1378` 的短路，而非守卫本身。
2. **`adapter.rs` 867 行**（超 800 行硬限）与登记表命名债
   （P4 的 `packages/runtime/src/registry/` 会话注册表 vs Track B 的 `driver_registry/`；
   `pub struct DriverRegistry` 目前全仓唯一）。
3. `src-tauri/src/platform/handles.rs:192` 的 `OwnerRef::Job` 与
   `packages/platform-api/src/context.rs` 的重复归属判定。

## Wave-1 收尾状态

三轨全 PASS 并已真合并。**Wave-2（AI 协议与 providers，强制复用四层链，不走 Web-shell 旁路；
wapps manifest/install/bridge）已解锁**；`store/` 下沉经用户裁定**取消**。
遗留：各轨 worktree 的未跟踪 `progress.md` 与本文件 `hub.md` **必须在并入 main 时删除**。

## Option A 合并：陈旧 `.driver-file-stash/` 改为硬失败

轨 `feature/stash-staleness`，实现 `9f829d1a9cb94df7ce3f50a8f3ea4a64763ece2e`，
**独立验证方 `c2de9cb4`（全新实例，未复用实现者）在自己独立的 detached worktree 上 PASS，零阻断缺陷**。
合并提交 `7f32ba2a6`，5 files +724/−6，零冲突标记。

合并后集成分支门禁（首尾 HEAD 一致 `7f32ba2a6`，期间无人改动）：

| 门禁 | 退出码 | 逐字结论 |
| --- | --- | --- |
| `node scripts/resolve-drivers.mjs --drivers=all` | 0 | codegen 正常，仅 `src-tauri/Cargo.toml` 被注入 |
| `pnpm test:scripts` | 0 | `43 passed (43)` / `760 passed (760)` |
| `pnpm typecheck` | 0 | `error TS` 计数 0 |

### 反证：修复前的缺陷确实真实（不是断言出来的）

验证方回到真父提交 `d12d98c5`（断言 `resolve-drivers.mjs` 1471 行 / `driver-file-stash.mjs` 321 行，
零 `StaleDriverFileStashError`），构造同一个陈旧现场后实测：`EXIT=0`、**stderr 0 字节**、
一个存在于合并提交里的行从工作区消失（`src-tauri/Cargo.toml` 的 sha256 由 `4f7dee48...` 变为 `969b9552...`）。
修复后同一现场：`EXIT=1`，该行**存活**。修复前错误、修复后正确，两头都有实测。

### 五项遗留的裁定

1. **无旁证旧 stash 硬失败** — **接受**。实测影响面一次性、自愈、零数据丢失：
   跨升级旧 stash 落 `missing-metadata` → 退出 1 → 报错给出恢复命令 → 按提示重跑退出 0。
   stash 内容不销毁，工作区文件不动。这是正确的 fail-closed 默认：年龄无法证明就不要猜。
   代价是升级当天每人第一次跑会看到一次硬失败，已写入提交信息。
2. **`resolve-drivers.mjs` 现 1622 行**（建议 ≤800）— **接受现状，另开轨道**。
   该文件修复前已 1471 行，本次 `+154/−3` 是既有债加深而非新引入。其中 **132 行是单块自包含代码**
   （新行 `1350-1487`：错误类 + `staleStashMessage` + `assertStashReusable`），
   只依赖 5 行导入，抽成独立模块可零行为变更把文件拉回历史规模。**不阻断本次修复**。
3. **`__boundaryProbe__.ts` 竞争 — 归因更正，另开轨道**。实现者报的竞争方
   `scripts/__tests__/driver-init-rustfmt.test.mjs` **实测零引用** `__boundaryProbe__` 或 `packages/ui`。
   真实竞争方是 `scripts/__tests__/check-module-layers.test.ts` 与 `scripts/__tests__/boundaryMutation.test.ts`，
   两者都在 `packages/ui/src/__boundaryProbe__.ts` 落探针并删除。该文件 gitignored（`.gitignore:199`），
   不会被 `git add` 收进去；**四个相关文件均不在 `9f829d1a9` 改动清单内**，本提交在结构上不可能引入它。
   「既有、超出范围」结论不变，被点名的文件不对。
4. **`--restore` 不设新鲜度闸门** — **维持原样**。它是在已知坏现场上的显式操作员动作，
   再加硬闸会堵死唯一的自救通道；`wantsRestoreOnly()` 在 `:1491-1494` 于任何新鲜度判定之前返回。
   `driver-file-stash.mjs status` 可只读地先拿新鲜度结论。**如实记录的代价**：
   执意无视警告的操作员仍可通过 `--restore` 拿到旧的静默回滚。这是有意依赖提示而非第二道硬停的取舍。
5. **文档与 `AGENTS.md` 是否充分** — **充分**。`docs/development/worktree-isolation.md:90-141`
   每处断言都对着代码核过；恢复命令逐字跑通。原 §5 → §6 重编号**未破坏任何链接**：
   全仓 4 处文件级引用（`AGENTS.md:76`、`scripts/resolve-drivers.mjs:1439`、
   `scripts/driver-file-stash.mjs:22`、`scripts/new-feature-worktree.sh:764`）**均不带节号**。

### 验证方对实现者的三处不同意（两处成立，一处已被采纳修文档）

1. `__boundaryProbe__` 归因错误 —— 成立，已按第 3 条更正。
2. 实现方的 `/private/tmp/stale-stash-repro.mjs` **高估了自己证明的范围**：它在合成临时 git 仓库里
   以库函数方式调用，从不进入 `main()`，而缺陷恰恰发生在 `main()` 的 catch 自动还原上。
   作为单元级证据成立，不足以自证缺陷；端到端证明由验证方的真实 CLI 复现补上。
   **这不是交付缺陷**（代码正确），是证据表述过头。
3. 文档示例用根 `Cargo.toml`，实测 `--drivers=all` 下真正被重写的是 `src-tauri/Cargo.toml`
   —— 已在本轮改为后者，并补一句「全 path 驱动时根 `Cargo.toml` diff 为空」。

### 集成方驳回验证方的一条合并清单项

验证方在 merge checklist 中写「按 AGENTS.md 删除 `hub.md`」。**驳回**：`hub.md` 的删除时点是
**并入 main 的收尾步骤**，不是任何一条 feature 分支合并的步骤。用户明确要求「不能影响主 worktree」，
并入 main 未执行，本文件是 Wave-2 唯一的跨轨台账，此刻删除等于销毁在途台账。台账在并入 main 前必须存活。

## Wave-2 定案（用户裁定）：A2 + B1 两轨并行

用户裁定：**决策 B 选 B1**（新建 `packages/workspace-apps`）、**决策 A 选 A2**（新建 `packages/ai-providers`），
AI-3 用例逻辑推到 Wave-3。

| 轨 | 新 crate | 下沉规模 | 宿主 `tauri::` 耦合 |
| --- | --- | --- | --- |
| A2 | `packages/ai-providers` | 5,129 LOC（protocol 3,767 + providers/registry 1,362） | **0** |
| B1 | `packages/workspace-apps` | 3,035 LOC（6 文件下沉） | **0**（protocol.rs 留下） |

**B1 不是纯搬迁**：`src-tauri/src/wapps/protocol.rs`（789）**必须留在宿主**——
它就是 Tauri `datazen:` URI scheme handler（`:25` `use tauri::{http, Emitter, Manager, Runtime, UriSchemeContext}`、
`:311` `register_uri_scheme_protocol`），并持有 `ASSET_CSP`（`:45-47`）。
`protocol_security_tests.rs`（694）测它，同样留下。搬任何一个都会让共享 crate 依赖 tauri，
正是 Wave-1 要消灭的反向依赖，违反 `INV-S8`。

**两轨并行的唯一已知冲突面**：各自向 `src-tauri/Cargo.toml` 加一行 `[dependencies]`。
两轨都不改根 `Cargo.toml`、不改 `scripts/lib/cargoWorkspace.mjs` —— 由集成方在派发前**预置**，
使跨轨合并不可能产生冲突。


## Wave-2 预置基线（协调者亲做，先于两轨存在）

基线提交 **`265e4dbaf`**，取代此前以 `d12d98c57` 为基线的记账。9 个文件 +87/−1：

| 文件 | 改动 |
| --- | --- |
| 根 `Cargo.toml` | `members` +2（`packages/ai-providers`、`packages/workspace-apps`，置于 `packages/runtime` 之后） |
| `scripts/lib/cargoWorkspace.mjs` | `LAYERS` +2（`LAYERS` 现为 15 项） |
| `scripts/run-platform-crate-tests.mjs` | `TESTED_LAYERS` 多行 +2 |
| `scripts/check-platform-crate-boundaries.mjs` | `RULES` +22 行，新增 **F-09 / F-10** |
| `docs/architecture/platform/shared-boundaries-and-ports.md` | §2.4 表新增 F-09 / F-10 两行（`:148`/`:149`）；§2.5 新增 Wave-2 段落（`:176-183`） |
| 两个新 crate 骨架 | 各 2 个文件（`Cargo.toml` + 4 行 `lib.rs`），依赖已钉死 |

根 `[workspace.dependencies]` 仍只有 7 条 path 条目、**无任何三方 crate**，故两个新 crate 各自直接声明依赖，
预置**不需要**改根清单的依赖段。

两条新边界规则（文档与代码 `RULES` 同步落盘，`checkSpecConsistency` 只遍历代码 `RULES`，
只改文档会静默失效）：

- **F-09**（`ai-providers` + `workspace-apps`）：normal + build 依赖闭包不含任何 `tauri*` crate，也不含宿主 `datazen`。
- **F-10**（**仅** `workspace-apps`）：不含 `axum` / `actix-web` / `warp` / `tonic`。`ai-providers` **故意不是**主体——
  它带 HTTP *客户端*（`reqwest`）是本职工作；把它纳入 HTTP 禁令会当场制造一条假红。

### 预置基线门禁（8/8 实跑，全绿）

| # | 门禁 | 退出码 | 结论行 |
| --- | --- | --- | --- |
| 1 | `cargo build -p datazen-ai-providers -p datazen-workspace-apps` | 0 | 两个新 crate 均可构建 |
| 2 | `cargo fmt --all -- --check` | 0 | 0 行差异 |
| 3 | `node scripts/check-platform-crate-boundaries.mjs` | 0 | `ok F-09: datazen-ai-providers, datazen-workspace-apps` / `ok F-10: datazen-workspace-apps` / `PASS — 28 workspace member(s) classified, 9 rule(s) evaluated over 30 crate(s), 1 rule×subject combo(s) vacuous: 0 violation(s), 0 error(s), 3 advisory(ies)` |
| 4 | `pnpm test:scripts` | 0 | `Test Files 43 passed (43)` / `Tests 760 passed (760)` |
| 5 | `pnpm test:layers` | 0 | `[check-module-layers] ok (4 rules, 87 files examined, 1 advisory)` |
| 6 | `pnpm test:platform-crates` | 0 | `PASS — 9 core crate(s) tested`（命令行含两个新 crate） |
| 7 | `pnpm test:platform-arch` | 0 | 与 #3 同口径 PASS |
| 8 | `pnpm typecheck` | 0 | `error TS` 计数 0 |

门禁 3 的 `ok F-09` / `ok F-10` 两行是「规则真的被求值」的证据——`1 rule×subject combo(s) vacuous`
意味着 F-09/F-10 没有落进空转分支。

提交前 `resolve-drivers.mjs` 把 `Cargo.lock` 弄脏，先 `git checkout -- src-tauri/Cargo.lock`，
再按**显式路径** `git add`（**绝不 `git add -A`**，`Cargo.lock` 永不入库）。

### 两轨工作树（均 `NEW_WT_DRIVERS=all` 引导，落在 `265e4dbaf`）

| 轨 | 工作树 | 分支 | 引导结果 |
| --- | --- | --- | --- |
| A2 | `.worktrees/datazen-ai-providers` | `feature/ai-providers` | 「未铺成：0 项 —— 轨道开箱即用」，locales 生成 EXIT=0 |
| B1 | `.worktrees/datazen-workspace-apps` | `feature/workspace-apps` | 同上 |

主 checkout 全程未动（停在 `@b3faa0e12`）。B1 额外风险：`packages/wapps/`（Wapp 的 JS 源码包）
**已存在**，与新 crate `packages/workspace-apps/` 是两回事，不得混淆、合并或改作他用。

## Wapp 服务端安装设计结论已落档（不再只存在于对话）

用户两轮追问的答复已改写为文档事实，不再停留在聊天里：

- `docs/architecture/platform/team-server-and-auth.md` §5 RBAC 词表：资源类型新增 **`wapp`**，安装/卸载/启用/停用归 **`write`** verb，
  读取安装态归 `read`；并写明客户端置灰只是提示、不构成授权。
- 同文新增 **§15.3 服务端安装与 Wapp 资源地址**，三条实测结论：
  1. `install_from_zip` / `install_from_dir` / `storage_*` 已按 `wapps_dir` **参数化**，服务端安装只需换存储根目录，
     隔离口径 `(organizationId, principalId, wappId)` 由 adapter 承担；
  2. manifest 的自定义资源字段 `backend` 在**安装时即被拒绝**（`Rule 3b: backend plugins are reserved for P2`），
     全仓 `.backend` 仅命中该拒绝点，**没有需要跨形态搬运的解释器**；P2 若落地必须只在共享内核实现一次；
  3. 真正随形态变化的是**资源地址不是 schema**：`datazen://` scheme → HTTPS origin，`entry`/`icon`/theme 路径重写属 adapter 职责，
     内核一旦感知 origin 就不再形态中立（F-10 的存在理由）。
- 另记 `WappManager` 是进程全局单例、启用态靠 `.enabled` 标记文件，服务端须改为按 `(organizationId, principalId)` 解析。

## 公共基础设施层 packages/platform-infra：裁定、证据与预置（Wave-2 第三轨）

### 一、为什么建这个 crate（不再依据「当前消费者数量」）

用户已明确否决过我的推论：共享基础设施的消费者是**尚未建出的形态**（web / server），
不是当前这棵树。web 版不会去读 `src-tauri/` 里的实现，只能照着 host 再写一份，
而且必然更弱——因为它看不见 host 代码。**后续所有落点判断一律从目标形态出发。**

这不是推演，本波已有两处实证：

| 事实 | 位置 | 性质 |
| --- | --- | --- |
| 脱敏 | A2 新写 `packages/ai-providers/src/protocol/mod.rs:85-104` `redact_endpoint_for_log` | 零 `Regex::new`、零 `OnceLock`，手工切字符串；只处理 userinfo 与整段 query，**漏掉 `Bearer`、`?token=`、`password` 字段** |
| zip 路径安全 | B1 新写 `packages/workspace-apps/src/zip_path.rs` | 与宿主 `src-tauri/src/app_data_archive.rs` 的 `validate_zip_entry_path` **逐字节相同（各 39 行）**，两份 `pub const MAX_COMPRESSION_RATIO: u64 = 100;` |

两处根因相同：**抽出域代码时看不到原实现，于是复制一份**。抽出动作本身没错，
错在没有承载原实现的公共位置。B1 Tester 已把后者记为非阻断合并分叉隐患：
日后任一处改动 `validate_zip_entry_path` 而不同步另一处，两套测试仍各自全绿。

### 二、命名与规则设计

- crate / 目录 / layer id 三者同名 **`platform-infra`**，crate 名 `datazen-platform-infra`。
  刻意**不叫 common**：本仓库的 `migration-common` 之所以健康，正因为名字点明领域、
  依赖面极窄（504 行 / 2 个依赖 / 零 trait）。叫 `common` 等于放弃这层唯一有效的自律。
- 规则 **F-11** 挂在 `scripts/check-platform-crate-boundaries.mjs` 的 `RULES` 末尾，用
  `allowedLayers: ['platform-infra']` 的**反向 allow-list**：`platform-infra` 不得依赖任何其它层，
  且**未来任何新加入 `LAYERS` 的层自动进入禁止集，无需改规则文件**。
  用 deny-list 会静默放行所有后来层——那正是它会退化成耦合中枢的路径。
  `forbiddenCrates: 'spec'` 加 `specCrates: [tauri, axum, actix-web, warp, tonic, react]`
  挡住形态/传输依赖。
- **变异测试实证规则会咬人**：向 `packages/platform-infra/Cargo.toml` 注入
  `datazen-platform-api = { path = '../platform-api' }` 后 `test:platform-arch` EXIT=1，
  报 `VIOLATION F-11 datazen-platform-infra (packages/platform-infra/Cargo.toml) -> datazen-platform-api`；
  `git checkout` 还原后回到 0。规则非空洞：门禁输出含 `ok F-11: datazen-platform-infra`。

### 三、协调者预置提交 179c6c8b6（8 文件 +75/-1）

新 crate 注册是 5 处，本轨全部登记完毕：根 `Cargo.toml` 的 `members`（紧跟 `platform-api`）、
`scripts/lib/cargoWorkspace.mjs` 的 `LAYERS`、`scripts/run-platform-crate-tests.mjs` 的
`TESTED_LAYERS`、`scripts/check-platform-crate-boundaries.mjs` 的 F-11、
`docs/architecture/platform/shared-boundaries-and-ports.md` 的 §2.4 行加新增 §2.5.1。
不进根 `[workspace.dependencies]`——它没有 `datazen-*` 依赖，加了反而要 `{ workspace = true }`。

**本轮修正了两件事，记在这里免得再犯**：

1. 预置提交最初把「路径安全」也写进 §2.5.1，但实作时已砍掉——已改写为「尚未并入本层的同类项」，
   并写明它**要等 Wave-2 合并后才能收敛**（本分支看不到 `packages/workspace-apps`，硬做会造出第三份）。
2. 预置提交的 message 最初写「门禁 … fmt 均 0」，而**当时 fmt 实为 EXIT=1**——
   缺的是 gitignored 的 codegen 文件 `src-tauri/src/driver_init.rs`，报
   `failed to resolve mod driver_init`，长得像格式问题。跑 `node scripts/resolve-drivers.mjs`
   生成后复跑 EXIT=0、零 diff。已 amend 让 message 只陈述实测到的结论。
   **教训：任何门禁结论不得先写进 message 再去凑。**

### 四、本轨范围与排序约束（★ 合规红线）

本轨 base = `78038d02b`（集成 HEAD），**不含 A2 / B1 任何代码**。因此本轨只能做 `redaction`
（源头 `src-tauri/src/log_redact.rs` 118 行，宿主内 15 处调用点），`path_safety` 一律不做。
**不要 rebase 本轨。** A2 / B1 的提交不碰根 `Cargo.toml` 与三个 scripts 文件，预置与二者干净合并。

Wave-2 三轨合并后的第二趟才是本层兑现价值的地方，届时**必须**做：
删 `ai-providers` 的 `redact_endpoint_for_log` 改引用本层；
把 `zip_path.rs` 与 `app_data_archive.rs` 的两份 `validate_zip_entry_path` 合成一份，宿主那份也改引用。

### 五、Track C3（infra 轨）工作树

| 项 | 值 |
| --- | --- |
| 工作树 | `.worktrees/datazen-platform-infra` |
| 分支 | `feature/platform-infra` |
| base | `78038d02b` |
| 预置 | `179c6c8b6` |
| CARGO_TARGET_DIR | `/tmp/dz-target-infra` |

同波并行：A2 修复轮（唤醒**原** Coder `4830a53d`，不新建）、C3 新 Coder `798513d9`。
A2 Tester 判定 `7beb43d6e` FAIL，**唯一缺陷是 AGENTS.md 的 L34 / L56 / L112 文档漂移**：
L34 仍称宿主 `ai/` 含 protocol（实测该目录已不存在，余 8 文件），L56 的 packages 树漏列 ai-providers，
L112 散文只提 ai-api。10 道门禁全绿、B/D/E/F/G 各节全 PASS，测试数 `1710 - 86 = 1624` 精确对账，
Tester 另用变异探针自证推翻了对 `restore_config` 覆盖不足的怀疑（删掉后 23 个宿主测试失败）。

## Wave-2 合并面实测（干跑，非推断）

合并前用一棵**一次性 detached 工作树** `.worktrees/merge-dryrun` 把三轨按 B1 -> A2 -> infra(pre-seed) 顺序
依次 `git merge --no-ff --no-commit` 干跑，跑完即 `git worktree remove --force` 删除。
`feature/shared-layer-extraction` 的 ref 在整个干跑期间**未移动**（仍 `082de2ee6`），主 checkout 未被触碰。

| 合并步骤 | 退出码 | 冲突文件 |
| --- | --- | --- |
| B1 workspace-apps -> 集成 | 0 | **0** |
| A2 ai-providers -> 集成 | 0 | **0** |
| infra 预置 -> 集成 | 0 | **0** |

**证伪了一条此前写进计划的预判**：我原以为三轨各自往 `src-tauri/Cargo.toml` 加一行会撞车，
并据此预告过「合并期需要人工和解」。实测 git 自动把它们安放在 `:20`（`datazen-ai-providers`）与
`:34`（`datazen-workspace-apps`），无冲突标记。同理 `shared-boundaries-and-ports.md`
（B1 与 infra 预置都改过）也是自动合并——spec 里 `platform-infra` 相关行已在合并结果中就位（4 处命中，
含 §2.4 F-11 行 `:150`、§2.5.1 `:197`、命名取舍 `:224`）。

**仍未被干跑覆盖的一处**：infra 轨的 redaction 提交尚未产生（Coder 在途，改动中 5 个文件）。
它要同时碰 `AGENTS.md`（在 `platform-api/` 之后插入 `platform-infra/` 行，落在第 53 行附近）
与 `src-tauri/Cargo.toml`，而 A2 已在 `AGENTS.md` 第 57 行插入 `ai-providers/`。
两处插入相距 4 行、diff3 上下文（各 3 行）**看起来不重叠**，但这是推断，不是实测——
待 Coder 交付 `READY_FOR_TEST` 后必须重跑一次干跑再正式合并。

教训：合并冲突预判是推断，「已知冲突类型」写进计划时应当标注为待实测。干跑成本约 10 秒，
比事后解冲突便宜一个数量级。

## Wave-2 A2/B1 并入集成分支与收轨

### 验证结论

| 轨 | 验证方 | 轮次 | 结论 |
| --- | --- | --- | --- |
| B1 workspace-apps | `91db8b14` | 1 | PASS，零缺陷（11 门禁全绿） |
| A2 ai-providers | `1c1aec4f` | 1 | FAIL，唯一缺陷 DEFECT-1（`AGENTS.md` 结构漂移，纯文档） |
| A2 修复后 | `95dfb818` | 2 | **PASS，零缺陷** |

A2 第 2 轮的关键推理值得记下来：修复提交 `c274843d1` 的
`git diff --stat 7beb43d6e c274843d1 -- '*.rs' '*.ts' '*.tsx' '*.json' '*.mjs' '*.toml'`
**空输出**，即修复零代码变更，于是第 1 轮对代码的 10 道门禁结论**原样继承**，不必重跑 168 秒的
vitest 与全套 cargo。验证方把这个推理明写出来，而不是悄悄跳过——否则「没跑」和「不必跑」无法区分。

### 合并

| 步骤 | 提交 | 冲突 |
| --- | --- | --- |
| merge B1 `feature/workspace-apps` | `88a7fdaac` | 0 |
| merge A2 `feature/ai-providers` | `77be2b19b` | 0 |
| 登记新成员 lockfile 条目 | `514528e30` | — |

与上一节干跑预测一致：两次合并均零冲突。合并后集成分支 `514528e30`。

### 合并后门禁（实测逐字结论）

- `pnpm --config.verify-deps-before-run=false typecheck` → EXIT=0，`error TS` 计数 0
- `npx vitest run` → EXIT=0，`Test Files 602 passed (602)` / `Tests 6335 passed (6335)`

602/6335 与合并前 A2 验证时的数字**逐字相同**，反证这两轨是纯 Rust 下沉、前端一行未动。

### 自纠：Cargo.lock 的禁则过宽

此前台账记的规则是「`Cargo.lock` 绝不提交」，理由是 `resolve-drivers.mjs` 会把它弄脏。
本次合并后 `Cargo.lock` 出现 `+32 行 0 删除`，实测其内容**只有**两个新 workspace 成员的依赖条目，
驱动噪音（redis/mysql/postgres/sqlite）**0 处**、`source = "git"` **0 处**。

查仓库自身先例：`f418e2f93 refactor: extract shared migration primitives`（新增 `datazen-migration-common`）
**当年就提交了 `Cargo.lock`**，diff 里正是 `+name = "datazen-migration-common"`。情形完全同类。

**结论：真正的禁则是「不得提交 `resolve-drivers` 的驱动噪音」，不是「不得提交新成员条目」。**
已按此提交 `514528e30`。否则 A2 验证方指出的问题会一直留存：`datazen-ai-providers` 从未进入任何
已提交的 `Cargo.lock`（`c274843d1` / `7beb43d6e` / `265e4dbafc` 三处计数均为 0），
`cargo metadata --locked` 会失败。CI 目前不被阻断，仅因 `--locked` 只用在 `cargo publish -p datazen-driver-api`。

### 收轨

移除 13 棵工作树、删除 6 条分支（A2 / B1 / A / B / C / stash-staleness），
删除前逐条验证 `git merge-base --is-ancestor <branch> feature/shared-layer-extraction` 全部为真。

一处值得记的护栏：在主 checkout（HEAD = `main`）执行 `git branch -d` 对 6 条分支**全部 exit=1**。
这是 git 在正确工作——这些分支并未并入 `main`。**不得用 `-D` 绕过**；正确做法是切到集成分支
（HEAD 即合并目标）再删，全部 exit=0。全过程主 checkout 仍为 `7c2151985`、porcelain 0 行、HEAD 未切换。

### 剩余

infra 轨已提交 `d24779e2f`（`refactor(platform-infra): 日志脱敏下沉为形态中立基础设施原语`），
验证方在途。它同样要改 `AGENTS.md` 与 `src-tauri/Cargo.toml`，与本节已并入的两轨重叠——
正式合并前**仍须重跑一次干跑**。

## Wave-2 C3 infra 轨：交付、干跑与两处裁定

### 交付

提交方 `798513d9` 交付 `d24779e2f4d87b4c889d17160769f829c2d88191`，父提交仍是协调者预置的 `179c6c8b6…`（未 amend）。
5 文件 +213 / −117：`packages/platform-infra/src/redaction.rs`（1 → 140 行）、
`src-tauri/src/log_redact.rs`（118 → 8 行薄壳）、`src-tauri/Cargo.toml` +2、
`AGENTS.md` +1、`progress.md`（新建）。`packages/platform-infra/Cargo.toml` 与
`shared-boundaries-and-ports.md` 均零改动。

两份**预测与实测吻合**的数字值得记下：宿主 `--lib` 由 **1710 → 1705 passed**，
差额恰为随实现一同下沉的 5 个脱敏单测（`platform-infra` 侧 5 passed / 0 failed），两侧闭合。
这两个数是本轨开工前就写进简报的预测值，不是事后追认。

### 合并前干跑（第三次，仍是零冲突）

上一节遗留的未测冲突类就是本轨要碰的 `AGENTS.md` 与 `src-tauri/Cargo.toml`。实测：
`git merge --no-ff --no-commit feature/platform-infra` → **EXIT=0，冲突 0**，`AGENTS.md` 与
`shared-boundaries-and-ports.md` 与 `src-tauri/Cargo.toml` 三处均为自动合并。
合并态依赖行：`Cargo.toml:20` ai-providers、`:29` platform-infra、`:36` workspace-apps，
三者互不挤占；`AGENTS.md` 目录树 `platform-infra` 落在 `:53`，与 A2 的 `ai-providers`(`:58`) 不冲突。

至此本项目三次干跑、两次真实合并，**预测冲突全部证伪**。这条经验要保留：
凡预测会有冲突的地方，先花十秒干跑，再谈解决。

### 裁定一：`stash@{0} dzinfra-baseline`

提交方为取门禁基线所建，未自行 drop（`.git stash` 为多轨共享状态，旁边另有 3 条属其它轨），请求协调者裁定。
**核查后 drop**：逐文件 sha256 比对，stash 的 `AGENTS.md` / `redaction.rs` / `log_redact.rs`
与提交**完全相同**；唯一差异在 `src-tauri/Cargo.toml`，diff 方向已判明——stash 版多出的正是
4 行驱动注入与 `default = ["driver-postgres", "driver-mysql", "driver-sqlite", "driver-redis"]`，
即被纪律禁止入库的驱动噪音；stash 的 `Cargo.lock` 含 4 个 `datazen-driver-*` 条目，同属该类。
结论：stash 独有的内容没有一样可提交。已 `git stash drop stash@{0}`，`stash@{0..2}` 其余 3 条完好。

### 裁定二：本轨 `Cargo.lock`

沿用上一节自纠后的规则处理：**新成员条目应入库，驱动噪音不得入库**。
但提交方工作区的 `Cargo.lock` 已被 `resolve-drivers` 注入弄脏，**不能**原样提交。
正确做法是待本轨合入集成分支后，在集成分支上跑一次 `cargo metadata` 重新生成干净的 lock，
核对其 diff 只含 `datazen-platform-infra` 新成员条目后单独提交——与 A2/B1 走过的 `514528e30` 同一条路径。

### 合并后收敛面（Wave-3 的输入，实测于集成分支 `d268a7ded`）

建这个 crate 的意义就在于此。实测出两处重复实现，是 Wave-2 合并后必须兑现的收敛：

| # | 重复实现 | 副本 A | 副本 B | 收敛去向 |
| --- | --- | --- | --- | --- |
| 1 | 日志脱敏 | `packages/ai-providers/src/protocol/mod.rs:85-104` `redact_endpoint_for_log`（手写切分，零 `Regex`、零 `OnceLock`；测试 `:356-374`） | `platform-infra::redaction` 3 个 pub fn | 删除 ai-providers 私有副本，改调 `datazen_platform_infra::redaction` |
| 2 | zip 条目路径校验 | `packages/workspace-apps/src/zip_path.rs:24` `validate_zip_entry_path` + `:21` `MAX_COMPRESSION_RATIO`（另有 `:66` `validate_theme_zip_path`） | `src-tauri/src/app_data_archive.rs:159` + `:203` | 两者合并为 `platform-infra::path_safety`，宿主 `app_data_archive.rs:356` 与 `commands/wapps/storage.rs:77` 一并改调 |

副本 1 的私有实现**比形态中立版本弱**（漏 `Bearer`、`?token=`、`password`）——这正是重复实现无边界约束时的典型退化。
副本 2 的 `zip_path.rs:10` 注释已自行承认「宿主保留自己的副本」，是有意识的欠账。

两处收敛都必须等 infra 合入集成分支后才能开工：当前 infra 工作树的基线 `78038d02b` 既不含 A2 也不含 B1 代码。
**不得 rebase infra 分支**。A2/B1 提交均未触碰根 `Cargo.toml`、`cargoWorkspace.mjs`、
`run-platform-crate-tests.mjs`、`check-platform-crate-boundaries.mjs`，因此收敛轨道不会再动那 4 个注册点。

### 验证

全新验证方 `c2d54385` 已派往 detached 工作树 `.worktrees/verify-platform-infra`（detached 于 `d24779e2f`，
`CARGO_TARGET_DIR=/tmp/dz-target-infra`，codegen 已生成）。该实例被引擎标记为 finished 但**未留下收尾消息**，
而其任务量不可能在数秒内完成——按纪律不据此判定死亡，已用 `send_message` 唤醒并要求其先自查再续跑。
**这是第三次遇到「finished 但无收尾消息」**：前两次（A2 R1、B1 的 Testers）最终都证明是活着却已停摆，
唤回后正常完成。结论固化：**`list_agents` 的 `inactive` 与引擎的 `finished` 都不等于死亡**，一律先 `send_message`。

### 裁定二的实测答案（合并前预演得到，非事后追认）

为免「合并后再看」，先在一次性工作树 `.worktrees/lockprobe` 上预演整条路径：
合入 infra → 提交 → `resolve-drivers` → `git checkout -- src-tauri/Cargo.toml` → `cargo metadata`。

结果：

- 合入 `feature/platform-infra` **冲突 0**（本项目第四次干跑零冲突）。
- 重新生成后的 `Cargo.lock` 增量 = **恰好 +9 行，只新增 `name = "datazen-platform-infra"` 一个包**，
  删除 0 条，`datazen-driver-` 与 `source = "git"` 噪音计数 **0**。

这同时解释了提交方报的 **+37 行**与此处 **+9 行**的差额：差额正是它工作区里被
`resolve-drivers` 注入的驱动噪音。**结论：提交方工作区的 `Cargo.lock` 不可原样入库**（纪律正确）；
正式做法是待本轨合入集成分支后，在集成分支上跑一次 `cargo metadata` 生成干净 lock，
核对其 diff 只含 `datazen-platform-infra` 新成员条目后单独提交——与 A2/B1 走过的 `514528e30` 完全同一条路径。
探针工作树已 `git worktree remove --force` 移除，集成分支 HEAD 与 porcelain 均未被它改动。

### 验证方派发踩坑：超长简报会静默失败

验证方 `c2d54385` 首次派发时被引擎标记为 finished 且**无收尾消息**。按纪律不据此判死，先取只读证据：
`.worktrees/verify-platform-infra` 的工作区指纹与派发前**逐字节相同**（仍只有建树时的
` M src-tauri/Cargo.toml` 残差），`/tmp/dzinfraV*` 日志数 **0**，target dir 30 分钟内无新产物——
说明它**一行命令都没执行**，属根本没启动，而非中途崩溃。

随后 `send_message` 发一条极小探针（要求只回 `PONG` + 一个 `git rev-parse HEAD`），**秒回且 HEAD 正确**，
证明实例与引擎均可用 ⇒ 病因是**那份一次性塞了 15 道门禁的超长简报**，不是环境或权限。
改法：**长任务分段派发**，先发「结构性核查（不跑 cargo，数十秒完成）」，回报后再单独发「cargo 门禁段」。

这条与「finished 不等于死亡」是**两个独立**的坑，前者管活性，后者管输入体积，都要写进台账。
仓里可留的证据：
