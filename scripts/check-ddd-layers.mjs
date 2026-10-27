#!/usr/bin/env node
/** D8 DDD dependency gate: the modules crate is the sole shared business layer. */
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const MODULES = 'datazen';
const MIN_WORKSPACE_MEMBERS = 20;
const FORBIDDEN_MODULES = new Set([
  'datazen_desktop',
  'datazen-runtime',
  'datazen-application',
  'datazen-platform-api',
  'datazen-kernel',
]);

function metadata() {
  return JSON.parse(execFileSync('cargo', ['metadata', '--format-version', '1'], {
    cwd: ROOT, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024,
  }));
}

function graphOf(meta) {
  const memberIds = new Set(meta.workspace_members);
  const graph = new Map();
  for (const node of meta.resolve.nodes) {
    if (!memberIds.has(node.id)) continue;
    graph.set(node.id, new Set(node.deps
      .filter((dep) => dep.dep_kinds.some((kind) => kind.kind === null))
      .map((dep) => dep.pkg)
      .filter((id) => memberIds.has(id))));
  }
  return { graph, byId: new Map(meta.packages.map((pkg) => [pkg.id, pkg.name])) };
}

function namedGraph(meta) {
  const { graph, byId } = graphOf(meta);
  const named = new Map();
  for (const [id, deps] of graph) named.set(byId.get(id), new Set([...deps].map((dep) => byId.get(dep))));
  return named;
}

function closure(graph, root) {
  const seen = new Set();
  const queue = [...(graph.get(root) ?? [])];
  while (queue.length) {
    const next = queue.shift();
    if (seen.has(next)) continue;
    seen.add(next);
    queue.push(...(graph.get(next) ?? []));
  }
  return seen;
}

function cycle(graph) {
  const visiting = new Set();
  const visited = new Set();
  function visit(node, path) {
    if (visiting.has(node)) return [...path.slice(path.indexOf(node)), node];
    if (visited.has(node)) return null;
    visiting.add(node);
    for (const next of graph.get(node) ?? []) {
      const found = visit(next, [...path, node]);
      if (found) return found;
    }
    visiting.delete(node);
    visited.add(node);
    return null;
  }
  for (const node of graph.keys()) {
    const found = visit(node, []);
    if (found) return found;
  }
  return null;
}

function modulesSourceViolations() {
  const problems = [];
  const root = path.join(ROOT, 'modules', 'src');
  const forbidden = /datazen_(application|runtime|platform_api|kernel)::|datazen-(application|runtime|platform-api|kernel)/;
  function walk(dir) {
    if (!existsSync(dir)) return;
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const file = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(file);
      else if (entry.name.endsWith('.rs')) {
        const raw = readFileSync(file, 'utf8');
        const production = raw.split('#[cfg(test)]', 1)[0];
        const source = production.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '');
        if (forbidden.test(source)) problems.push(path.relative(ROOT, file));
      }
    }
  }
  walk(root);
  return problems;
}

function selfProof() {
  const graph = new Map([['a', new Set(['b'])], ['b', new Set(['a'])]]);
  return cycle(graph) ? [] : ['cycle detector failed synthetic proof'];
}

function main() {
  const named = namedGraph(metadata());
  const problems = selfProof();
  if (named.size < MIN_WORKSPACE_MEMBERS) problems.push(`only ${named.size} workspace crates found`);
  const foundCycle = cycle(named);
  if (foundCycle) problems.push(`workspace production dependency cycle: ${foundCycle.join(' -> ')}`);
  if (!named.has(MODULES)) problems.push(`${MODULES} is not a workspace member`);
  else {
    for (const forbidden of FORBIDDEN_MODULES) {
      if (closure(named, MODULES).has(forbidden)) problems.push(`${MODULES} depends on retired/host crate ${forbidden}`);
    }
  }
  const sourceViolations = modulesSourceViolations();
  if (sourceViolations.length) problems.push(`modules source references retired layers: ${sourceViolations.join(', ')}`);
  if (problems.length) {
    console.error('DDD layer gate FAILED:');
    for (const problem of problems) console.error(`  • ${problem}`);
    process.exitCode = 1;
    return;
  }
  console.log(`DDD layer gate PASS: ${named.size} 个 workspace 生产 crate，无环；modules 独立于旧层与宿主。`);
}

if (import.meta.url === `file://${process.argv[1]}`) main();
