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

## 观测记录

| 时间 | 观测者 | 变体 | 结论 |
| --- | --- | --- | --- |
| 2026-10-21 | 分册文档代理 | `.ts` | 全量首跑红，单跑 8 passed，全量复跑绿 |
| 2026-10-21 | `insert-backend` 复测代理 | — | 未跑 `vitest`，未触发 |
| 2026-10-21 | `grid-selection` 编码代理 | `.tsx` | 全量首跑红，同 HEAD 复跑绿 |
