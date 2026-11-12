# BUG-002：activate 同步段内重入 load(same id) 绕过 FIFO 队列，实例订阅泄漏

- 状态：已关闭（2026-11-11 tester 聚焦回归验证通过，见文末「回归验证」）
- 严重程度：一般（健壮性；触发比 BUG-001 更苛刻）
- 发现阶段：issue-32 BUG-001 修复回归验证（tester 对抗探针 R10 → 特征化 C1–C4）
- 关联：[BUG-001](BUG-001-concurrent-load-overwrite-leak.md)（同根因：检查-激活-记录非原子；本条是其修复未覆盖的更窄路径）
- 归属判定：**既有行为，先于 BUG-001 修复存在**（T01 时代基线复核 B1：无队列版本上同样泄漏；非修复引入，修复只是未触及此路径）

## 发现方法

BUG-001 回归探针设计中的重入用例：在 `activate` 执行体内对**同一 id** 再次调用 `loader.load()`，验证队列对重入调用的串行化。期望它排在本操作之后（等价串行重载）；实际绕过了队列。

## 复现步骤

```ts
import { HostExtensionLoader, type ExtensionModule } from '@datazen/extension-points';

const loader = new HostExtensionLoader();
const order: string[] = [];
let inner!: Promise<void>;

const outer: ExtensionModule = {
  activate(ctx) {                      // 同步 activate（无 await）
    ctx.subscriptions.push(() => order.push('outer-sub'));
    inner = loader.load('win', {       // 在自己激活的同步段内重入 load(same id)
      activate(ctx2) {
        ctx2.subscriptions.push(() => order.push('inner-sub'));
      },
    });
  },
};

await loader.load('win', outer);       // outer 激活中，重入 load 已启动
await inner;
await loader.unload('win');
console.log(order);                    // => ['outer-sub']：inner-sub 永不释放
```

窗口边界（特征化探针 C2 证实）：重入调用若发生在 activate 内**首个 `await` 之后**（即 `enqueue` 的 `set(id, tail)` 已落地），则正确串行、无泄漏（`order = ['outer-sub', 'inner-sub']`）。泄漏仅在 activate 同步前缀内的重入调用发生；C3 证实双 async activate（重入点在同步前缀）同样泄漏。

## 期望行为

`lifecycle.ts` 类文档锁定的契约：「Every state-mutating operation on one id runs inside a per-id FIFO queue… Concurrent `load(id)` calls serialize in call order.」重入的 `load(same id)` 是与本操作重叠的并发调用，应串行化（inner 排在 outer 完成之后，对 outer 实例执行 deactivate + LIFO 后再激活），两个实例的订阅最终都释放。

## 实际行为

重入 load 在 `enqueue` 的 `set(id, tail)` 执行**之前**被调用（`operation()` 先于 `set` 执行，重入点恰在 `operation()` 的同步前缀内），`operationQueues.get(id)` 返回 `undefined` → `previous === undefined` → inner 的 `loadExclusive` 立即并发执行：其 `has()` 检查时 outer 尚未记录 → 跳过重载清理 → 两个实例的「激活-记录」交错，后记录者覆盖先记录者（本例 inner 先记录、被 outer 覆盖）→ 被覆盖实例的 `context.subscriptions` 从加载器不可达，**与 BUG-001 相同的静默泄漏**。

相邻窗口（同一未注册窗口的其它表现，均无泄漏，仅语义不一致，随本条一并修复即可）：

- 同步段内重入 `unload(same id)`：立即运行为 pre-record no-op，而外部并发 unload 会等待加载完成后卸载（C4）。
- 同步段内重入 `reset()`：快照读不到进行中操作，排空失准（本 id 可能在 reset 后落盘存活）。

## 根因分析

`enqueue` 的语句顺序：`const run = previous ? previous.then(op, op) : operation();` 先调用 `operation()`，`this.operationQueues.set(extensionId, tail)` 在其后执行。`operation()` 的同步前缀（`loadExclusive` 的 has 检查、context 创建、`module.activate(context)` 的同步部分）运行时，本操作的尾 promise 尚未注册——只有在该窗口内发起的重入调用能观察到这一点，外部调用因单线程事件循环不可能插入该窗口，故 BUG-001 的外部并发修复对其不可见。

## 建议修复方向

在调用 `operation()` **之前**注册占位尾 promise，使重入调用链接到它（保持「仅当已 settle 且仍为当前尾才删除」的条目管理不变）：

```ts
private enqueue<T>(extensionId: string, operation: () => Promise<T>): Promise<T> {
  const previous = this.operationQueues.get(extensionId);
  let signal!: () => void;
  const tail = new Promise<void>((res) => { signal = res; });
  this.operationQueues.set(extensionId, tail);      // 窗口在此关闭
  const run = previous ? previous.then(operation, operation) : operation();
  void run.then(
    () => { signal(); this.maybeDelete(extensionId, tail); },
    () => { signal(); this.maybeDelete(extensionId, tail); },
  );
  return run;
}
```

要点：`signal()` 释放链接在占位尾上的重入操作；删除仍需身份比对（重入操作在 enqueue 时已用自己的尾覆盖 Map，清理回调 identity check 失败即跳过，由该重入操作的尾自行负责删除）。此方向保持空闲 id 同步启动（R12 契约）不变。备选：文档化排除重入调用并 fail-closed（抛错），由 coder 评估。修复时建议将特征化探针 C1/C3 翻转为断言无泄漏、C4 断言与外部语义一致，并补 reset 重入用例。

## 影响评估

- 触发条件：扩展模块在自己的 `activate` 同步前缀内对同一 extensionId 发起 `load`。现实触发比 BUG-001 更苛刻（自我热重载式用法）；仓库唯一 loader 消费者 `resolve-pro.mjs` 为外部 await 单 id 用法，不触发。
- 后果：与 BUG-001 相同——被覆盖实例的注册（EP 实现 / valuePresentation 贡献）无法经卸载释放，表现为「停用后贡献残留」类症状，且无告警。
- 判定：一般级（健壮性边界缺口），不阻塞 issue-32 后续任务；建议与 BUG-001 修复同文件排期。

## 修复记录

按建议方向修复：`enqueue` 现在先构造占位尾 promise 并 `operationQueues.set(id, tail)`，**然后**才启动 `operation()`。时序：①读 previous → ②创建占位尾（捕获 `signal` resolver）→ ③`set(id, tail)`（重入窗口在此关闭）→ ④`previous ? previous.then(op, op) : op()`（空闲 id 仍同步启动，R12 契约不变）→ ⑤`run.then(settleQueueEntry, settleQueueEntry)`。`settleQueueEntry` 在本操作 settle 后 `signal()` 释放链接在占位尾上的重入操作，并按身份比对决定是否删除 Map 条目（重入操作在窗口内已用自己的尾覆盖 Map，身份比对失败即跳过删除，由该尾自行负责）。占位尾永不 reject，保持 settled-safe 语义；调用方仍拿到原始 `run` promise，失败 load 的拒绝行为不变。

语义补充（已在类文档与测试锁定）：`activate` 内**不得** await 同 id 重入 load——队列保证其仅在本 load settle 后运行，该 await 是构造性自死锁（重入调用对激活中模块是 fire-and-forget）。

回归测试（`loader-concurrency.test.ts` 新增 2 项，总 11 项）：①同步 activate 内重入 load(same id)（bug 探针 C1 形态）——重入调用排队等待外层完成后按串行重载语义清理（`['outer-sub','inner-sub']` 无泄漏）；②async activate 同步前缀内重入 load + 重入 unload（C3+C4 形态）——两者均正确排队，重入 unload 不再是 pre-record no-op 而是等待外层记录后真正卸载。对照验证：仅这 2 项在 BUG-001 版 enqueue（set-after-operation）下失败，修复后 11/11 通过；既有 79 项（BUG-001 的 9 项、hotplug 15 项、T01 16 项等）全部不受重排影响。

## 回归验证

- 验证人：tester（聚焦回归，未修改任何被测代码）
- 日期：2026-11-11
- 方法：
  1. **门禁复跑**：`npx vitest run packages/extension-points --retry=0` **81/81** 通过（79 既有 + 2 新增）；`pnpm typecheck` 与 `pnpm typecheck:tsc` 双引擎 OK。
  2. **代码审读**（新 `enqueue` + `settleQueueEntry` 时序，逐点）：
     - (a) **signal 释放路径无遗漏**：`run.then(settleQueueEntry, settleQueueEntry)` 双臂覆盖成功/失败；同步抛错经 `previous ? previous.then(op, op) : op()` 的空闲分支虽会同步逃出 `enqueue`，但逃逸即 `run` 为 rejected promise，`run.then(…, …)` 的拒绝臂仍会执行 `settleQueueEntry`——且当前 `load`/`unload` 闭包均为 async 方法体，`operation()` 同步抛错实际不可达（与 BUG-001 审读同一结论，占位尾 `set` 在 `operation()` 之前，窗口内重入不会挂死）。挂在占位尾上的重入操作由 `signal()` 必然释放（promise resolve 无条件执行）。
     - (b) **身份比对删除在占位尾语义下仍正确**：重入操作在窗口内经自己的 `enqueue` 用新占位尾覆盖 Map；外层 `settleQueueEntry` 的 identity check 失败即跳过删除，由重入操作的尾自行负责——探针 S3b（同窗口挂 load+load+unload 三操作）与 S3d（重入突发后 Map 无残留、空闲 id 新 load 仍同步启动，R12 契约保持）行为级证实。
     - (c) **`signal()` 与删除的顺序**：`settleQueueEntry` 先 `signal()` 后按身份决定删除；`signal()` 使挂在占位尾上的重入 `previous.then(op, op)` 在**下一个微任务**启动，而本操作的 `activeExtensions.set`（对 load 而言）在 `run` settle 前已同步完成——重入操作启动时看到的必然是外层已完整落盘的状态，串行语义不破坏。交错确认：`signal()` 的续体排在当前微任务之后，外层 settle 链内不存在「重入已启动但外层未清理」的可见窗口。
  3. **聚焦探针**（/tmp/probe-bug002，6 项 × 3 轮全绿）：S1 原始复现（同步 activate 重入 load）：`['outer-sub','inner-sub']` 无泄漏、外层订阅不被提前释放、记录数正确；S2 重入 + 外层失败：失败清理释放外层订阅、占位尾上的重入 load 不被毒化、最终无泄漏；S3a 嵌套重入（重入操作自身同步前缀再重入第三层）：三层全排队、`['outer-sub','mid-sub','deep-sub']` 全释放；S3b 同一占位尾挂多操作（load+load+unload）：FIFO 逐个串行重载/卸载；S3c 窗口内重入 reset：排空外层记录的实例、fire-and-forget 负载无法幸存 reset、loader 可复用；S3d 重入突发后 Map 条目清理（无残留尾）。
  4. **既有套件交叉复核**：BUG-001 回归期的 /tmp/probe-bug001 全套 19 项对新代码复跑：16 项（BUG-001 全部并发/毒化/reset/unhandled 断言 + 特征化 C2 窗口边界证明）**全过**；3 项失败恰为特征化探针 C1/C3/C4 的「断言泄漏存在」断言——按预期翻转（实际输出已变为无泄漏/正确卸载），即 bug 文件建议的「C1/C3 翻转为无泄漏断言」已被 coder 的 2 项回归测试以等价形式覆盖。无修复引入的回归。
- 结论：**通过，关闭**。期望行为（activate 同步段内重入同 id 调用与外部并发调用同队串行，无覆盖泄漏）成立；BUG-001 修复的 79 项语义零回归。
- 观察项（一句话，不立新条目）：类文档已如实声明「activate 内 await 同 id 重入 load 为构造性自死锁（重入调用应为 fire-and-forget）」，该约束属接口契约而非缺陷；预存的理论边界（如 `enqueue` 空闲分支 `operation()` 若为同步抛错会逃出 enqueue）在当前 async 闭包形态下不可达，不再展开。
