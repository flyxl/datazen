#!/usr/bin/env node
/**
 * DDD 分层门禁：把 `docs/development/ddd-refactor-plan.md` 的 P0 断环结论变成**可执行事实**。
 *
 * ## 它核对什么
 *
 * 三条断言，全部从 `cargo metadata` 的**真实依赖图**派生，没有手写清单可以漂移：
 *
 * 1. **无环**：workspace 内的生产依赖（normal）图不得有环。`cargo` 自己也会拒绝上线
 *    Cargo 环，但它的报错在「试加一条边」的那一刻才出现，且只告诉你"cyclic package
 *    dependency"——这里给出完整的环路径，让下一个人知道该拆哪条边。
 * 2. **R2（application 与 runtime 是兄弟）**：两者不得互相依赖。这条曾经是真环的一
 *    一段（`application → schema-diff → runtime → application`），P0 把 `ApiError`、
 *    用例端口 `ConnectionUseCases`、请求 DTO 搬到 platform-api 之后才成立。
 * 3. **P0 断环的可行性仍然成立**：对每个领域包，`datazen-application` 不得出现在它的
 *    传递依赖闭包里。等价于「今天再往 application 加一条 `application → 领域包` 的边也
 *    不会闭成环」。这条是 1 的**前向不变量**：只查 1 会得到"现在没有环"，查 3 才能得到
 *    "想加的那条边仍然加得进去"。
 *
 * ## 为什么不做成「往 application 里加一条装饰性依赖」
 *
 * 计划原文（P0 验收）曾写「`application` 增依赖领域包证明环解」。落地时改成上面的第 3
 * 条：一条**没人 import 的生产依赖**会把领域包（进而 runtime）拉进 application 的依赖
 * 闭包，让 `cargo build -p datazen-application` 编出整个运行时，而它一行都不用——这是
 * 用构建成本换一句"证明"。断言 3 检查的是同一条边是否可行，且不需要它真的存在。
 *
 * ## 静默失效的防线
 *
 * 门禁最容易的死法是"扫了 0 个东西然后通过"。这里三处自证：
 * * workspace member 数必须 > 20（少于此说明 metadata 解析退化）；
 * * 环检测器必须能在**合成的**环上报警（不是只对当前图返回"无环"）；
 * * 断言 2 必须能在**合成的**边上报警。
 */

import { execFileSync } from 'node:child_process';

const APPLICATION = 'datazen-application';
const RUNTIME = 'datazen-runtime';
const FEATURE_CORE = 'datazen-feature-core';

/**
 * 领域包：D2「领域层只能依赖更内层」的对象。它们今天都（直接或间接）依赖 runtime，
 * 所以 `application → 领域包` 的安全前提就是「领域包闭包里没有 application」。
 */
const DOMAIN_PACKAGES = [
  'datazen-schema-diff',
  'datazen-data-sync',
  'datazen-data-transfer',
  'datazen-migration-common',
  'datazen-tunnel',
];

/**
 * **反方向**的消费者：它们依赖 application（宿主与共享 feature 层）。
 * 它们不能进上面的领域包表——那把尺子量的是另一条边。这里断言的是镜像不变量：
 * application 的闭包里不得出现这些包，否则 `feature-core → application → feature-core`
 * 立刻成环（`datazen` 宿主同理：宿主依赖 application，application 永远不许反向依赖宿主）。
 *
 * `feature-core` 从 P2 起**不再直接依赖** application（它只用 platform-api 的契约：
 * `use_cases::ConnectionUseCases`、`dto::requests`、`error::ApiError`），所以除了镜像
 * 不变量，还额外断言这条边不存在——否则「共享 feature 层只认契约」会在下次改动里
 * 悄悄退回去。
 */
const UPSTREAM_CONSUMERS = ['datazen-feature-core', 'datazen'];

const MIN_WORKSPACE_MEMBERS = 20;

function cargoMetadata() {
  const raw = execFileSync('cargo', ['metadata', '--format-version', '1'], {
    encoding: 'utf8',
    maxBuffer: 256 * 1024 * 1024,
  });
  return JSON.parse(raw);
}

/** 只认 normal 依赖：dev/build 依赖不进入生产图（dev 依赖环是 cargo 明确允许的）。 */
function normalDependencyGraph(metadata) {
  const workspaceIds = new Set(metadata.workspace_members);
  const byId = new Map(metadata.packages.map((pkg) => [pkg.id, pkg]));
  const graph = new Map();
  for (const node of metadata.resolve.nodes) {
    if (!workspaceIds.has(node.id)) continue;
    const deps = new Set();
    for (const dep of node.deps ?? []) {
      const isNormal = (dep.dep_kinds ?? []).some((kind) => kind.kind === null || kind.kind === 'normal');
      if (isNormal && workspaceIds.has(dep.pkg)) deps.add(byId.get(dep.pkg).name);
    }
    graph.set(byId.get(node.id).name, deps);
  }
  return graph;
}

/** 返回第一个找到的环（含起点，首尾同名），无环返回 null。 */
export function findCycle(graph) {
  const state = new Map(); // name -> 'visiting' | 'done'
  const stack = [];
  let cycle = null;

  const visit = (name) => {
    if (cycle) return;
    state.set(name, 'visiting');
    stack.push(name);
    for (const next of graph.get(name) ?? []) {
      if (!graph.has(next)) continue;
      if (state.get(next) === 'visiting') {
        cycle = [...stack.slice(stack.indexOf(next)), next];
        return;
      }
      if (!state.has(next)) visit(next);
      if (cycle) return;
    }
    stack.pop();
    state.set(name, 'done');
  };

  for (const name of [...graph.keys()].sort()) visit(name);
  return cycle;
}

function transitiveDependencies(graph, name) {
  const seen = new Set();
  const queue = [...(graph.get(name) ?? [])];
  while (queue.length > 0) {
    const next = queue.shift();
    if (seen.has(next)) continue;
    seen.add(next);
    for (const child of graph.get(next) ?? []) if (!seen.has(child)) queue.push(child);
  }
  return seen;
}

/**
 * P2 断言：共享 feature 层只认 `platform-api` 的契约，不依赖用例聚合包。
 *
 * 抽成纯函数是为了能对**合成的**依赖表自证——否则这条断言只会在真实图里跑，
 * 一旦 `cargo metadata` 退化（feature-core 解析不到），它会静默通过。
 */
function featureCoreIndependenceViolations(graph) {
  if (!graph.get(FEATURE_CORE)?.has(APPLICATION)) return [];
  return [
    `P2：${FEATURE_CORE} 直接依赖 ${APPLICATION}——共享 feature 层只认 platform-api 的契约` +
      '（用例端口、请求 DTO、ApiError），不依赖用例聚合包。',
  ];
}

function selfProof() {
  const problems = [];
  const cyclic = new Map([
    ['a', new Set(['b'])],
    ['b', new Set(['a'])],
    ['c', new Set(['a'])],
  ]);
  if (!findCycle(cyclic)) {
    problems.push('自证失败：环检测器在合成环 a→b→a 上没有报警，本门禁的"无环"断言不可信。');
  }
  const acyclic = new Map([
    ['a', new Set(['b'])],
    ['b', new Set()],
  ]);
  if (findCycle(acyclic)) {
    problems.push(`自证失败：环检测器在无环图上报了环 ${findCycle(acyclic).join(' → ')}。`);
  }
  const withFeatureCoreEdge = new Map([[FEATURE_CORE, new Set([APPLICATION])]]);
  if (featureCoreIndependenceViolations(withFeatureCoreEdge).length === 0) {
    problems.push(
      `自证失败：P2 断言在合成边 ${FEATURE_CORE} → ${APPLICATION} 上没有报警。`,
    );
  }
  const withoutFeatureCoreEdge = new Map([[FEATURE_CORE, new Set([RUNTIME])]]);
  if (featureCoreIndependenceViolations(withoutFeatureCoreEdge).length > 0) {
    problems.push('自证失败：P2 断言在 feature-core 不依赖 application 的图上误报了。');
  }
  return problems;
}

function main() {
  const metadata = cargoMetadata();
  const graph = normalDependencyGraph(metadata);
  const violations = [...selfProof()];

  if (graph.size < MIN_WORKSPACE_MEMBERS) {
    violations.push(
      `只解析到 ${graph.size} 个 workspace 生产 crate，少于自证下限 ${MIN_WORKSPACE_MEMBERS}：` +
        'metadata 解析退化会让下面三条断言全部空转通过。',
    );
  }

  const cycle = findCycle(graph);
  if (cycle) {
    violations.push(`P0/R2：workspace 生产依赖出现环 ${cycle.join(' → ')}——断环被退回。`);
  }

  for (const [left, right] of [
    [APPLICATION, RUNTIME],
    [RUNTIME, APPLICATION],
  ]) {
    if (graph.get(left)?.has(right)) {
      violations.push(
        `R2：${left} 直接依赖 ${right}（相邻两侧的签名必须住在 platform-api，见 §P0）。`,
      );
    }
  }

  violations.push(...featureCoreIndependenceViolations(graph));

  const applicationClosure = transitiveDependencies(graph, APPLICATION);
  for (const consumer of UPSTREAM_CONSUMERS) {
    if (applicationClosure.has(consumer)) {
      violations.push(
        `P0 镜像不变量：${APPLICATION} 的传递依赖里出现了 ${consumer}，` +
          '而它依赖 application ⇒ 环。应用层不得反向依赖消费者。',
      );
    }
  }

  for (const domain of DOMAIN_PACKAGES) {
    if (!graph.has(domain)) {
      violations.push(`${domain} 不在 workspace 生产图里：领域包清单已过时，请同步本脚本。`);
      continue;
    }
    const closure = transitiveDependencies(graph, domain);
    if (closure.has(APPLICATION)) {
      violations.push(
        `P0 可行性：${domain} 的传递依赖里出现了 ${APPLICATION}` +
          ' ⇒ 「application → 领域包」这条边今天会闭成环。先在领域包里把对应用层的引用下沉到 platform-api。',
      );
    }
  }

  if (violations.length > 0) {
    console.error('DDD layer gate FAILED:');
    for (const violation of violations) console.error(`  • ${violation}`);
    process.exitCode = 1;
    return;
  }

  console.log(
    `DDD layer gate PASS: ${graph.size} 个 workspace 生产 crate，无环；` +
      `${APPLICATION} ↔ ${RUNTIME} 无直接依赖；` +
      `${DOMAIN_PACKAGES.length} 个领域包的闭包内无 ${APPLICATION}；` +
      `${APPLICATION} 的闭包内无 ${UPSTREAM_CONSUMERS.join(' / ')}；` +
      `${FEATURE_CORE} 不依赖 ${APPLICATION}。`,
  );
}

if (import.meta.url === `file://${process.argv[1]}`) main();
