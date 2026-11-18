# BUG-001：HostExtensionLoader 同 id 并发 load 覆盖致先完成实例订阅泄漏

- 状态：已关闭（2026-11-11 tester 回归验证通过；原始复现路径及全部外部并发语义已修复，见文末「回归验证」。同根因的更窄路径——activate 同步段内重入 load——另立 [BUG-002](BUG-002-reentrant-load-bypasses-queue.md) 跟踪，不属本条复现范围）
- 严重程度：一般（健壮性）
- 发现阶段：issue-32 T01 独立测试验证（探针 P19）
- 归属判定：**既有行为，先于 T01 存在，非本次改动引入**（`git show HEAD:packages/extension-points/src/lifecycle.ts` 核实：改动前 `load()` 已是「has 检查 → await activate → set 记录」形态；T01 只是在 activate 外包了 try/catch 失败清理，未触碰该时序）。因在被 T01 修改的文件中经对抗性探针复现，按流程记录并移交。

## 发现方法

T01 对抗性探针（/tmp/probe-t01/probes/concurrency.test.ts，P19）：用受控 deferred gate 交错两个同 id 的 `load()` 调用，验证并发时序下激活失败清理与加载器状态完整性。

## 复现步骤

```ts
import { HostExtensionLoader, type ExtensionModule } from '@datazen/extension-points';

function deferred(): { promise: Promise<void>; resolve: () => void } {
  let resolve!: () => void;
  const promise = new Promise<void>((r) => { resolve = r; });
  return { promise, resolve };
}

const loader = new HostExtensionLoader();
const disposed: string[] = [];
const gateA = deferred();
const gateB = deferred();

const moduleA: ExtensionModule = {
  async activate(ctx) {
    ctx.subscriptions.push(() => disposed.push('A'));
    await gateA.promise; // 挂起 A 的激活
  },
};
const moduleB: ExtensionModule = {
  async activate(ctx) {
    ctx.subscriptions.push(() => disposed.push('B'));
    await gateB.promise;
  },
};

const loadA = loader.load('race-ext', moduleA); // has()=false，进入激活
const loadB = loader.load('race-ext', moduleB); // A 尚未 set 记录，has() 仍= false，也进入激活
gateB.resolve(); await loadB;                   // B 先完成，activeExtensions.set(id, entryB)
gateA.resolve(); await loadA;                   // A 后完成，set(id, entryA) 覆盖 entryB

await loader.unload('race-ext');                // 只能触达 entryA
console.log(disposed);                          // => ['A']，B 的订阅永远不会被释放
```

## 期望行为

同 id 的并发 `load()` 不应静默丢弃先完成实例的生命周期资源：后到激活应等价于串行重载（对先完成实例执行 deactivate + LIFO 清理），或至少以显式错误拒绝第二次并发加载。

## 实际行为

两个 `load()` 并行通过 `has()` 前置检查；后完成者覆盖 `activeExtensions` 条目，先完成实例的 context（含其全部 subscriptions，如经 `registerValuePresentation`/`registerExtensionPoint` 登记的释放句柄）从加载器不可达，其订阅与注册贡献永久泄漏，且无任何告警。

## 根因分析

`HostExtensionLoader.load()` 的「检查-激活-记录」不是原子序列：`await module.activate(context)` 期间 `activeExtensions` 尚无该 id 条目，另一个同 id `load()` 的 `has()` 检查读不到进行中的激活，双双完成后后写者覆盖先写者。同步 activate 因单线程事件循环天然互斥不受影响；仅 async activate 交错时触发。

## 建议修复方向

任选其一（coder 评估）：

1. 以「进行中加载」登记表（id → pending promise）串行化同 id 加载：第二个 `load()` await 第一个完成后再按既有重载语义处理；
2. 或在 `set` 前检测条目已被本轮之外的加载占据时，先对被覆盖条目执行 deactivate + LIFO 清理再覆盖；
3. 或明确文档化「宿主必须串行调用同 id load」并在检测到并发时抛错（fail-closed）。

注意：修复时同步核查 `unload`/`reset` 与进行中激活的交错；并为修复补充回归测试（可直接采用上述探针形态）。

## 影响评估

- 触发条件苛刻：需同一 HostExtensionLoader 实例对同一 extensionId 发起交错的 async 激活。当前宿主对 EP 加载为宿主控制的串行流程，正常使用不触发。
- 后果：泄漏的 subscriptions 中的注册（如 valuePresentation 贡献、EP 实现）无法经卸载释放，表现为「停用扩展后贡献仍残留」一类症状。
- 因属既有行为且不影响 T01 验收基准（激活失败清理语义已由 T01 正确交付），不阻塞 T01；建议单独排期。

## 修复记录

采用建议方向 1：`HostExtensionLoader` 内建按 id 的 FIFO 操作队列（id → settled-safe 尾 promise），同 id 的 `load`/`unload` 全部经队列入队，观察上等价于按调用顺序串行执行；后到 load 对先前实例执行既有重载语义（deactivate + LIFO），不再覆盖泄漏。交销语义决策（已在 `lifecycle.ts` 类文档与测试中锁定）：

- **unload × 进行中加载**：`unload(id)` 等待同 id 进行中加载 settle 后卸载其记录的实例（加载失败则 no-op）。理由：`await unload(id)` 之后必须能依赖 `isLoaded(id) === false`，否则进行中加载会在其后完成并留下已加载状态。代价：挂起的 activate 也会挂起排在它后面的操作（与 load-load 串行化同一取舍，本文件一贯无超时）。
- **reset × 进行中加载**：`reset()` 对「已激活 ∪ 有进行中操作」的每个 id 在其队列后追加 unload，fire-and-forget 加载无法在 reset 之后落盘存活；reset 开始后才发起的操作不被等待；不同 id 的卸载并行执行，任一 deactivate 抛错时仍等全部完成再重抛首个错误（旧顺序循环会在首个失败处放弃其余 id）。
- **失败隔离**：某次 load 的激活拒绝不会毒化排在它后面的同 id 操作（队列尾 promise 吞掉前驱结果）；调用方仍拿到各自的原始结果。

回归测试（`loader-concurrency.test.ts`，9 项）：双并发 load 串行重载、valuePresentation 贡献释放、三并发 FIFO、unload 等待进行中加载、load/unload/load 交错、失败激活不毒化后续队列、reset 排空进行中加载、跨 id 不互相串行、串行语义回归（deactivate + LIFO、失败重载留空、未知 id no-op）。对照验证：8/9 项在修复前（HEAD 版本 lifecycle.ts）失败，修复后全部通过。

## 回归验证

- 验证人：tester（独立回归，未修改任何被测代码）
- 日期：2026-11-11
- 方法：
  1. **门禁复跑**：`npx vitest run packages/extension-points` 79/79 通过（70 既有 + 9 新增）；`--retry=0` 复跑 79/79（排除根配置 retry 掩盖）；`pnpm typecheck` 与 `pnpm typecheck:tsc` 双引擎 OK。
  2. **代码审读**（`enqueue`/`loadExclusive`/`unloadExclusive`/`reset` 逐行）：(a) 尾 promise 以身份比对删除——`set(id, tail)` 与 `operation()` 同步块内先后执行，微任务清理回调要么见到自己的尾（删除）要么见到更新的尾（跳过），无提前删除、无丢失串行；(b) `tail = run.then(noop, noop)` 使链对前驱失败免疫，`previous.then(operation, operation)` 双臂为纵深防御，失败 load 不毒化后继（探针 R5：连续 5 个并发失败 load 后同 id 生命周期完全健康）；(c) `loadExclusive` 内部重载走私有 `unloadExclusive`（不经过公共 enqueue），无自我入队路径，load→unload→load 交错为纯 FIFO 链（探针 R1–R4、coder 测试 5）；(d) `reset` 以 `allSettled` 并行排空「已激活 ∪ 进行中」的每个 id 后重抛首个拒绝——探针 R8 验证两个抛错 deactivate + 一个正常实例：三 id 全部卸载且首个错误上抛（旧实现会在首个失败处放弃其余 id）；非首个错误被静默丢弃（无日志），属可接受取舍；(e) 队列条目在成为已 settle 的当前尾后被删除，空闲 id 不积累（探针 R12：繁忙期后同 id 新 load 同步启动，证明无残留尾 promise；5 种子 × 240 op 压力下无增长）。
  3. **对抗探针**（/tmp/probe-bug001，19 项 × 3 轮全绿）：R1/R2/R3 原始复现场景（保持 bug 文件调用顺序与 gate 顺序，B 的 gate 先开）——B 被压在 A 后、串行重载对 A 执行 deactivate + LIFO、最终 disposed 含 A 与 B、字面脚本中 `await loadB` 在 gateA 开启前不再 settle（覆盖窗口消失）；R4 五种子随机交错压力（load/unload/reset × 门控/失败/抛错 deactivate 混合，240 op/种子）：零泄漏（push 句柄全数 dispose）、零重复 dispose、结束全卸载、零 unhandled rejection；R5 队列毒化；R6/R7 reset 竞态（void load 未 settle 时 reset：排空、可复用；并发双 reset + 多 id 在飞）；R9 全句柄处理场景零 unhandled rejection；R11 deactivate 内触发同 id unload（no-op、无死锁）；R13 fire-and-forget 失败 load 观测性对比（见下）。
  4. **检出力基线复核**：coder 的 9 项回归测试对 HEAD 版 lifecycle 8/9 失败（coder 声明核实无误）；对 T01 时代（BUG-001 修复前的正确基线，含激活失败清理、无队列）6/9 失败——全部 6 项 BUG-001 相关并发测试均检出，测试有效。
- 结论：**通过，关闭**。期望行为（后到激活等价于串行重载，对前一实例执行 deactivate + LIFO 清理）在全部外部并发交错下成立；既有 70 项测试无回归。
- 边界与移交：
  - 同根因的更窄路径未覆盖：activate **同步段内**重入 `load(same id)` 绕过队列（`enqueue` 的 `set(id, tail)` 在 `operation()` 返回后才执行，重入调用读不到本操作尾 promise），inner 实例先记录后被覆盖、订阅泄漏。经 T01 基线复核证实**先于本修复存在**（非修复引入），触发比 BUG-001 更苛刻，另立 [BUG-002](BUG-002-reentrant-load-bypasses-queue.md)。
  - 观测性差异（非缺陷，记录备查）：`tail = run.then(noop, noop)` 为调用方拿到的 `run` 挂了拒绝处理器，fire-and-forget 的失败 load 不再触发 process 级 `unhandledRejection`（T01 基线为 1 次，修复后 0 次，探针 R13/B2 实测）。await 调用方的拒绝语义不变；仓库内唯一消费者 `resolve-pro.mjs` 为 await 用法，不受影响。
