# TEST-INFRA-001 测试探针文件并发竞态导致无关门禁变红

**状态**：已知未修复 · 偶发（并行跑全量 `vitest` 时命中，单跑任一文件必绿）
**影响面**：仅测试基础设施（`scripts/__tests__/`）。不影响产品功能、不影响单文件测试结果；但会**周期性把与本改动无关的门禁跑红**，且失败信息指向一个与改动毫不相干的文件，极易被误判成"本次改动引入的回归"而去改错地方。
**结论**：两个脚本测试对仓库内的**同一组临时探针文件**有互相冲突的文件系统假设——一个会临时创建/删除它们，另一个会**整仓递归拷贝**仓库。

## 现象

全量 `npx vitest run` 偶发失败，失败点是：

```
FAIL scripts/__tests__/driver-init-rustfmt.test.mjs > generated driver_init.rs is rustfmt-canonical
Error: ENOENT: no such file or directory, lstat
  '<repo>/packages/ui/src/__boundaryProbe__.tsx'
```

两种变体都被观测到：`.ts` 与 `.tsx`。同一 HEAD、同一条命令**重跑即绿**；单独跑 `driver-init-rustfmt.test.mjs` 或 `boundaryMutation.test.ts` 都全绿。

## 链路实测

### 1. 删除方：`boundaryMutation.ts` 临时植入并删除探针文件

| 行 | 事实 |
| --- | --- |
| `:295-304` | `export const PROBE_PATHS` 列出 `packages/ui/src/__boundaryProbe__.{ts,tsx,js,jsx,mjs,cjs}`、`packages/ui/dist/__boundaryProbe__.{ts,js…}` 等路径 |
| `:356` | `writeFileSync(full, contents)` —— 植入 |
| `:361` | `for (const file of createdFiles) rmSync(file, { force: true })` —— 清理时删除 |

即这些探针文件**在全量运行期间会短暂存在、随后被删除**。

### 2. 拷贝方：`driver-init-rustfmt.test.mjs` 整仓递归拷贝

| 行 | 事实 |
| --- | --- |
| `:70` | `const REPO = resolve(HERE, '../..')` —— 拷贝源是整个仓库 |
| `:163-166` | `cpSync(REPO, stage, { recursive: true, filter: … })` |
| `:91-101` | `SKIP_DIRS` 只排除 `node_modules` / `target` / `.git` / `posts` / `dist` / `coverage` / `.codegraph` / `e2e` / `test` —— **`packages/ui/src` 不在排除列表里** |

`cpSync` 递归遍历时会 `lstat` 每个条目。若某个探针文件在**遍历到它的父子目录之后、`lstat` 到它之前**被 `boundaryMutation` 删掉，就抛 `ENOENT`。这是典型的 TOCTOU（检查与使用之间存在时间窗）。

### 3. 为什么只在全量时出现

`vitest` 默认多线程跑测试文件。两个文件并行执行才有重叠窗口；单独跑任一方都不存在并发删除/拷贝，故必绿。窗口很小，所以是偶发。

## 为什么本轮不修

它属于 `scripts/__tests__/` 的基础设施，与本轮的 data-browsing PRD 实现无关；且修改它需要决定"探针文件放哪、谁负责清理"的全局约定，不宜在一个 feature 分支里顺手改。

## 修复前置条件与候选方案

三个方向，按代价从低到高：

1. **让拷贝跳过探针文件**：在 `SKIP_DIRS` 之外增加一个文件名过滤（`filter: (src) => !basename(src).startsWith('__boundaryProbe__')`）。改动最小，但只是把窗口缩到"目录遍历"层面，理论上仍可能撞到探针**新建的目录**。
2. **把探针文件移出仓库树**：`withTempSourceFiles` 改为在 `tmpdir()` 里建树，凡是需要"仓库内路径"的断言改用映射后的路径。根治，但要逐个检查依赖真实相对路径的用例。
3. **给两个文件加串行约束**：把 `driver-init-rustfmt.test.mjs` 与 `boundaryMutation.test.ts` 放进同一个 `sequence` 组（或前者改用 `pool: 'forks'` + `singleThread`）。代价是需要维护一份"不可并行"清单。

**建议**：优先方案 1（立即止损），条件成熟后走方案 2（根治）。无论选哪个，都需要先写一条能**稳定复现**的测试（例如人为放大删除/拷贝窗口），否则改完无法证明修好了。

## 机制定性（协调者复核，2026-10-21）

写侧**有**互斥：`scripts/__tests__/boundaryMutation.ts` 提供 `withProbeLock`（`mkdir` 原子锁 + 死锁接管 + 超时），`withTempSourceFiles` / `withTempSourceFile` 都在锁内写删探针；`check-module-layers.test.ts` 与 `check-driver-import-boundaries.test.mjs` 都走这把锁。

**读侧没有参与锁**，这才是根因。当前有**两个**读侧会看见"半成品树"：

| 读侧 | 怎么撞上 | 症状 |
| --- | --- | --- |
| `driver-init-rustfmt.test.mjs` 的 `stageRepo()` | `cpSync(REPO, stage, { recursive: true })` **整棵活仓库**拷贝，拷贝期间探针被删 → `lstat` 抛 ENOENT | `FAIL driver-init-rustfmt.test.mjs` |
| `pnpm typecheck`（`tsc --noEmit`） | 根 `tsconfig.json` 的 `include` 含 `packages/ui`，`packages/ui/tsconfig.json` 的 `include` 含 `src` ⇒ **探针路径在 tsc 的 program 里**；glob 展开后文件被删 | `error TS6053: __boundaryProbe__.ts not found` |

**还有一个尚未被触发的隐患**：探针文件里是**故意违规的 import**（用来验证守卫能抓到）。只要它在 `tsc` 运行期间存在，`pnpm typecheck` 就会去类型检查这个**故意非法**的夹具——即使不发生竞态，typecheck 也可能因此变红。目前没暴露，只是因为探针通常不存在。

⇒ 结论：**锁只保护写者，不保护读者**。修复必须让探针对读者不可见（按路径排除 / 拷贝过滤），或让读者也参与锁；只把 ENOENT 吞掉是掩盖，不是修复。

## 修复验收标准（协调者）

修复交付后由协调者**独立**验证，逐条要成立：

1. **守卫没有被削弱**（最重要）：故意引入一次真实越界（让 `packages/ui` 导入一个它不该导入的模块），边界守卫**必须仍然报违规**。若修复方式是跳过探针、删断言、给违规加豁免，此条直接判不通过。
2. **默认命令可靠**：`npx vitest run`（全量）与 `pnpm typecheck` **连续 10 次全绿**；`npx vitest run scripts/__tests__` 单独跑**连续 10 次全绿**。**不得**要求开发者额外传参或用 `--exclude`。
3. **没有用例被削弱**：修复 diff 里**不得**出现 `.skip` / `.todo` / 删除断言 / 把 `expect` 改成恒真。
4. **没有吞异常**：不得用 `try/catch` 把 ENOENT 之类的真实失败静默忽略；只允许"明确针对已知探针路径"的重试或跳过，且必须留下可诊断信息。
5. **探针对读者不可见**：`tsc` 的 program 里**不应**包含探针路径（否则故意违规的夹具会被类型检查）；整仓拷贝**不应**把探针带进暂存目录。
6. **锁语义未破坏**：`scripts/__tests__/boundaryMutation.test.ts` 里"被杀死运行的接管""超时抛错""ownerless 宽限"等既有用例必须仍然全绿——修复不能把互斥降级成"没有互斥"。
7. **不留残渣**：跑完后 `packages/ui/src/__boundaryProbe__*` 与 `packages/ui/dist/__boundaryProbe__*` 不得残留；`git status --short` 不得出现探针。
8. **没有明显性能回退**：全量 `vitest` 与 `typecheck` 的耗时不得明显变长（修复不是靠"每次都重试整仓拷贝"实现的）。
9. **修复必须落在 `scripts/` 与配置里**，不能靠"每条轨自己证明红的不是我"来规避——那正是本缺陷存在的意义。

## 观测记录（2026-10-21）

| 观测者 | 变体 | 结论 |
| --- | --- | --- |
| 分册文档代理 | `.ts` | 全量首跑红，单跑 8 passed，全量复跑绿 |
| `insert-backend` 复测代理 | `.tsx` | 全量首跑红，同 HEAD 复跑绿 |
| `grid-selection` 编码代理 | `.tsx` | 全量首跑红，同 HEAD 复跑绿 |
| `grid-cell` 编码代理 | `.tsx` | 全量首跑红，**连续 3 次每次红的文件都不同** |
| `web-transport` 编码代理 | `.tsx` | 并发跑 typecheck 时撞上，报 `error TS6053: __boundaryProbe__.ts not found` |

## 受害面比最初判断的更广（`grid-cell` 轨的独立观测）

1. **第二个受害套件**：`scripts/__tests__/check-platform-crate-boundaries.test.ts > F-04: platform-api depending on driver-api is allowed (§2.3)` —— 与 `driver-init-rustfmt` 不同的文件、不同的断言。
2. **不跑全量也复现**：单独跑 `npx vitest run scripts/__tests__` **连续 3 次 → EXIT=0 / 1 / 1**，且**每次红的文件都不同**。⇒ 竞态在 `scripts/` 套件内部就已成立，**不需要**宿主前端套件参与。
3. **它还会污染 `typecheck`**：探针文件在 `tsc` 的 glob 展开与读取之间被删掉，`pnpm typecheck` 报 `error TS6053: __boundaryProbe__.ts not found`。
4. ⇒ 结论升级：**这不是"偶发一次、重跑就好"**，而是**门禁不可靠**——「全量 vitest 绿」「typecheck 绿」这两个结论在有并发时都不可信。
