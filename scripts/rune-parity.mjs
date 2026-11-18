#!/usr/bin/env node
// Parity ledger for the React → Rune UI migration.
//
// Reads `packages/frontend/src`, classifies every non-test `.tsx` by how it can
// be ported, and buckets the work into tasks that each fit in one working day.
// The ledger is derived data: do not commit it, regenerate it.
//
//   node scripts/rune-parity.mjs                 summary by area
//   node scripts/rune-parity.mjs --tasks 40      the first 40 day-sized tasks
//   node scripts/rune-parity.mjs --md /tmp/ledger.md
//   node scripts/rune-parity.mjs --json /tmp/ledger.json
//
// The committed tasklist keeps one line per task; its status is that line's
// checkbox, so a status edit is a one-character diff:
//
//   node scripts/rune-parity.mjs --tasklist      refresh, keeping known statuses
//   node scripts/rune-parity.mjs --status T007=done
//   node scripts/rune-parity.mjs --progress      count, nothing written back

import { readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const REACT = join(ROOT, 'packages/frontend/src');

const HEAVY = [
  ['codemirror', '@codemirror/'],
  ['xyflow', '@xyflow/react'],
  ['recharts', 'recharts'],
  ['xlsx', 'xlsx'],
  ['dagre', '@dagrejs/dagre'],
  ['marked', 'marked'],
  ['virtual', '@tanstack/react-virtual'],
];

/** Dependency order: shared primitives first, engine-heavy areas last. */
const ORDER = [
  'components/ui',
  'components/DataTable',
  'components/filter',
  'components/schema',
  'components/history',
  'components/query',
  'components/connection',
  'components/chart',
  'components/ai',
  'components/migration',
  'components/sql-editor',
  'components',
  'windows/settings',
  'windows/connection',
  'windows/workspace',
  'windows/dashboard',
  'windows/workflow',
  'windows/main',
  'windows/onboarding',
  'windows/wapps',
  'windows/data-transfer',
  'windows/data-sync',
  'windows/schema-diff',
  'windows/migration',
  'windows/backup',
  'windows',
  'lib',
  'hooks',
  'stores',
  'types',
  'i18n',
  'locales',
  'platform',
  'extensions',
  'bootstrap',
];

/**
 * Dependency order for the `.ts` foundation layer (types, pure helpers, state,
 * IPC shells). These tasks are appended after the `.tsx` ones so that existing
 * task ids keep pointing at the same file.
 */
const TS_ORDER = ['lib', 'types', 'stores', 'hooks', 'commands', 'platform', 'components', 'windows', 'extensions'];

/** Porting effort in working days; size-aware so a 300-line file is not "small". */
function daysFor(cls, loc) {
  if (cls === 'X') return 0;
  // A port behind a third-party engine: the file's plumbing on top of a landed
  // engine (the engines themselves are Phase 0 work, tracked by hand).
  if (cls === 'H') return Math.max(2.0, Math.round((loc / 250) * 10) / 10);
  if (cls === 'C') return loc > 320 ? 1.3 : 0.9;
  return loc <= 120 ? 0.3 : loc <= 260 ? 0.5 : 0.8;
}

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    if (name === '__tests__' || name === 'node_modules') continue;
    const path = join(dir, name);
    if (statSync(path).isDirectory()) walk(path, out);
    else out.push(path);
  }
  return out;
}

function snake(name) {
  return name
    .replace(/\.tsx$/, '')
    .replace(/([a-z0-9])([A-Z])/g, '$1_$2')
    .replace(/([A-Z]+)([A-Z][a-z])/g, '$1_$2')
    .toLowerCase();
}

function areaOf(rel) {
  const segs = rel.split('/');
  return segs.length > 1 && !segs[1].includes('.') ? segs.slice(0, 2).join('/') : segs[0];
}

/** P: presentational, C: connected, H: behind a third-party engine, X: re-export shell. */
function classify(text) {
  if (/^\s*(export \*|export \{)/m.test(text) && text.split('\n').length <= 6) return 'X';
  if (HEAVY.some(([, needle]) => text.includes(needle))) return 'H';
  if (/from '[^']*stores\//.test(text)) return 'C';
  if (/from '[^']*(commands|platform|backend-client)/.test(text)) return 'C';
  if (/\buse[A-Z]\w*Store\(|\.getState\(\)/.test(text)) return 'C';
  return 'P';
}

const files = walk(REACT).filter((p) => p.endsWith('.tsx') && !p.endsWith('.test.tsx'));
const rows = files.map((path) => {
  const text = readFileSync(path, 'utf8');
  const rel = relative(REACT, path).split(sep).join('/');
  const loc = text.split('\n').length;
  const cls = classify(text);
  const area = areaOf(rel);
  return {
    react: `packages/frontend/src/${rel}`,
    rune: `crates/datazen-ui/src/views/${rel.split('/').pop().replace(/\.tsx$/, '.rs').replace(/([a-z0-9])([A-Z])/g, (m, a, b) => `${a}_${b.toLowerCase()}`)}`,
    area,
    order: ORDER.findIndex((prefix) => area === prefix || area.startsWith(`${prefix}/`)),
    loc,
    cls,
    elements: (text.match(/<[A-Z][A-Za-z0-9.]*/g) ?? []).length,
    hooks: (text.match(/\buse[A-Z]\w*\s*\(/g) ?? []).length,
    keys: new Set(text.match(/\bt\(\s*'([^']+)'/g) ?? []).length,
    days: daysFor(cls, loc),
  };
});

/**
 * The `.ts` foundation layer: everything a `.tsx` port imports but the ledger
 * above never counted. Locale packs are excluded — they feed `build.rs`
 * instead of being ported. These files never render, so the element/hook
 * counters stay at zero and the class comes from the same rules.
 */
const tsFiles = walk(REACT).filter(
  (p) =>
    p.endsWith('.ts') &&
    !p.endsWith('.test.ts') &&
    !p.endsWith('.spec.ts') &&
    !p.endsWith('.d.ts') &&
    !p.split(sep).includes('locales') &&
    !p.split(sep).includes('test') &&
    !p.split(sep).includes('__mocks__'),
);

const tsRows = tsFiles.map((path) => {
  const text = readFileSync(path, 'utf8');
  const rel = relative(REACT, path).split(sep).join('/');
  const area = areaOf(rel);
  return {
    react: `packages/frontend/src/${rel}`,
    rune: `crates/datazen-ui/src/${rel.replace(/\.ts$/, '.rs').replace(/([a-z0-9])([A-Z])/g, (m, a, b) => `${a}_${b.toLowerCase()}`)}`,
    area,
    order: TS_ORDER.findIndex((prefix) => area === prefix || area.startsWith(`${prefix}/`)),
    loc: text.split('\n').length,
    cls: classify(text),
    elements: 0,
    hooks: (text.match(/\buse[A-Z]\w*\s*\(/g) ?? []).length,
    keys: new Set(text.match(/\bt\(\s*'([^']+)'/g) ?? []).length,
    days: daysFor(classify(text), text.split('\n').length),
  };
});

const byOrder = (list) =>
  [...list].sort(
    (a, b) => (a.order === -1 ? 999 : a.order) - (b.order === -1 ? 999 : b.order) || a.react.localeCompare(b.react),
  );

const sorted = byOrder(rows);

/**
 * The `.ts` files every DataTable port needs first, so the foundation batch
 * unblocks real pages before it fills in the long tail of helpers.
 */
const TS_FIRST = [
  'lib/formatters.ts',
  'lib/cellTypes.ts',
  'lib/cellEditorKind.ts',
  'lib/dataTypeColors.ts',
  'components/DataTable/editors/toEditString.ts',
];

const tsSorted = [...tsRows].sort((a, b) => {
  const rel = (r) => r.react.replace('packages/frontend/src/', '');
  const fa = TS_FIRST.indexOf(rel(a));
  const fb = TS_FIRST.indexOf(rel(b));
  if (fa !== fb) return (fa < 0 ? 99 : fa) - (fb < 0 ? 99 : fb);
  return (a.order === -1 ? 999 : a.order) - (b.order === -1 ? 999 : b.order) || a.react.localeCompare(b.react);
});

/** Greedy buckets of at most one working day; a single file is never split mid-port. */
function bucket(list) {
  const tasks = [];
  let cur = [];
  let load = 0;
  for (const row of list) {
    if (row.cls === 'X') continue;
    if (row.days > 1.0) {
      if (cur.length) tasks.push({ area: cur[0].area, rows: cur, days: load });
      tasks.push({ area: row.area, rows: [row], days: row.days, split: true });
      cur = [];
      load = 0;
      continue;
    }
    if (cur.length && load + row.days > 1.0) {
      tasks.push({ area: cur[0].area, rows: cur, days: load });
      cur = [];
      load = 0;
    }
    cur.push(row);
    load += row.days;
  }
  if (cur.length) tasks.push({ area: cur[0].area, rows: cur, days: load });
  return tasks;
}

// `.tsx` tasks keep their historical ids (T001…); the `.ts` foundation is appended.
const tasks = [...bucket(sorted), ...bucket(tsSorted)];
const allRows = [...rows, ...tsRows];
const args = process.argv.slice(2);
const flag = (name, fallback = null) => {
  const i = args.indexOf(name);
  return i >= 0 ? (args[i + 1] ?? true) : fallback;
};

const totals = {
  files: allRows.length,
  loc: allRows.reduce((n, r) => n + r.loc, 0),
  days: Math.round(allRows.reduce((n, r) => n + r.days, 0) * 10) / 10,
  tasks: tasks.length,
  filesTs: tsRows.length,
  locTs: tsRows.reduce((n, r) => n + r.loc, 0),
  tasksTs: tasks.length - bucket(sorted).length,
};

/**
 * The tasklist is the one committed artefact of this script: one line per task,
 * status carried by that line's checkbox alone. Everything else on the line is
 * generated, so refreshing the list never rewrites a status by hand and a status
 * edit never touches any other line.
 */
const TASKLIST = join(ROOT, 'docs/development/rune-migration/tasklist.md');

const MARK_OF_STATE = { todo: ' ', doing: '~', done: 'x', blocked: '!' };
const STATE_OF_MARK = { ' ': 'todo', '~': 'doing', x: 'done', '!': 'blocked' };

const TASKLIST_HEADER = [
  '# React → Rune 逐文件迁移台账',
  '',
  '一个 task 一行，**改状态只改该行方括号里的一个字符**：',
  '',
  '- `- [ ]` 待办 · `- [~]` 进行中 · `- [x]` 完成 · `- [!]` 阻塞',
  '',
  '状态不落到任何汇总数字里（那会逼着改状态时连带改别的行）：进度用',
  '`node scripts/rune-parity.mjs --progress` 现算。行尾的 area、工作量、涉及文件由脚本生成，',
  '不要手改；React 源码变动后跑 `node scripts/rune-parity.mjs --tasklist` 刷新，',
  '已存在的 T 编号保留原状态。',
  '',
  'T001–T252 是非测试 `.tsx`（页面与组件），T253 起是它们依赖的 `.ts` 基础层',
  '（`lib/`、`types/`、`stores/`、`hooks/`、`commands/`）。`.ts` 的第一批五项',
  '（`lib/formatters`、`lib/cellTypes`、`lib/cellEditorKind`、`lib/dataTypeColors`、',
  '`editors/toEditString`）排在 T253–T256，因为 DataTable 主体没有它们无法 1:1 迁。',
  '',
];

const TASK_LINE = /^- \[(.)\] (T\d{3}) · (.+)$/;

function readTasklist(path) {
  let text;
  try {
    text = readFileSync(path, 'utf8');
  } catch {
    return new Map();
  }
  const seen = new Map();
  for (const line of text.split('\n')) {
    const m = TASK_LINE.exec(line);
    if (m && STATE_OF_MARK[m[1]] !== undefined) seen.set(m[2], m[1]);
  }
  return seen;
}

function tasklistLine(index, task, mark) {
  const files = task.rows.map((r) => r.react.replace('packages/frontend/src/', '')).join(' ');
  const split = task.split ? ' · split' : '';
  return `- [${mark}] T${String(index + 1).padStart(3, '0')} · ${task.area} · ${task.days.toFixed(2)}d · ${files}${split}`;
}

if (flag('--tasklist')) {
  const path = flag('--tasklist') === true ? TASKLIST : flag('--tasklist');
  const previous = readTasklist(path);
  const kept = [...previous.keys()].filter((id) => Number(id.slice(1)) > tasks.length);
  const lines = [...TASKLIST_HEADER, ...tasks.map((t, i) => tasklistLine(i, t, previous.get(`T${String(i + 1).padStart(3, '0')}`) ?? ' '))];
  writeFileSync(path, `${lines.join('\n')}\n`);
  const done = [...previous.values()].filter((m) => m === 'x').length;
  console.log(`wrote ${path} (${tasks.length} tasks, ${done} already done${kept.length ? `, dropped ${kept.length} stale ids` : ''})`);
} else if (flag('--status')) {
  const spec = String(flag('--status'));
  const [rawId, rawState] = spec.split('=');
  const id = rawId?.toUpperCase();
  const mark = MARK_OF_STATE[rawState];
  if (!/^T\d{3}$/.test(id ?? '') || mark === undefined) {
    console.error(`bad --status "${spec}": want T###=todo|doing|done|blocked`);
    process.exit(2);
  }
  const path = flag('--file') === true || flag('--file') === null ? TASKLIST : flag('--file');
  const lines = readFileSync(path, 'utf8').split('\n');
  const at = lines.findIndex((line) => TASK_LINE.exec(line)?.[2] === id);
  if (at < 0) {
    console.error(`${id} is not in ${path}`);
    process.exit(2);
  }
  const before = STATE_OF_MARK[TASK_LINE.exec(lines[at])[1]];
  const current = lines[at];
  lines[at] = `${current.slice(0, 3)}${mark}${current.slice(4)}`;
  writeFileSync(path, lines.join('\n'));
  console.log(`${id}: ${before} -> ${rawState}`);
} else if (flag('--progress')) {
  const path = flag('--progress') === true ? TASKLIST : flag('--progress');
  const marks = readTasklist(path);
  const id = (i) => `T${String(i + 1).padStart(3, '0')}`;
  const count = (m) => tasks.filter((_, i) => (marks.get(id(i)) ?? ' ') === m).length;
  const done = count('x');
  const daysDone = tasks.reduce((n, t, i) => n + ((marks.get(id(i)) ?? ' ') === 'x' ? t.days : 0), 0);
  console.log(`${done}/${tasks.length} tasks done (${count('~')} doing, ${count('!')} blocked, ${count(' ')} todo)`);
  console.log(`${daysDone.toFixed(1)} of ${totals.days} working days landed on this track`);
} else if (flag('--json')) {
  writeFileSync(flag('--json'), JSON.stringify({ generatedAt: new Date().toISOString(), totals, tasks, files: allRows }, null, 2));
  console.log(`wrote ${flag('--json')} (${totals.tasks} tasks)`);
} else if (flag('--md')) {
  const lines = [
    `# React → Rune parity ledger`,
    '',
    `${totals.files} files, ${totals.loc} loc, ${totals.tasks} tasks, ${totals.days} working days on one track.`,
    '',
    '| # | area | days | react file | loc | class | hooks | keys |',
    '| --: | --- | --: | --- | --: | --- | --: | --: |',
    ...tasks.flatMap((t, i) =>
      t.rows.map((r) => `| ${i + 1} | ${t.area} | ${t.days.toFixed(2)}${t.split ? ' split' : ''} | \`${r.react.replace('packages/frontend/src/', '')}\` | ${r.loc} | ${r.cls} | ${r.hooks} | ${r.keys} |`),
    ),
  ];
  writeFileSync(flag('--md'), `${lines.join('\n')}\n`);
  console.log(`wrote ${flag('--md')} (${totals.tasks} tasks)`);
} else if (flag('--tasks')) {
  const limit = Number(flag('--tasks'));
  console.log(`${totals.tasks} tasks, ${totals.days} working days on one track\n`);
  for (const [i, t] of tasks.slice(0, limit).entries()) {
    console.log(`T${String(i + 1).padStart(3, '0')}  ${t.area.padEnd(26)} ${t.days.toFixed(2)}d`);
    for (const r of t.rows) console.log(`        ${r.cls} ${String(r.loc).padStart(4)}L  ${r.react.replace('packages/frontend/src/', '')}${t.split ? '   [one file > 1 day: split by sub-component]' : ''}`);
  }
} else {
  const byClass = {};
  for (const r of allRows) {
    byClass[r.cls] ??= { files: 0, loc: 0, days: 0 };
    byClass[r.cls].files += 1;
    byClass[r.cls].loc += r.loc;
    byClass[r.cls].days += r.days;
  }
  console.log('class  files    loc   days');
  for (const [k, v] of Object.entries(byClass)) {
    console.log(`${k.padEnd(5)} ${String(v.files).padStart(5)} ${String(v.loc).padStart(6)} ${v.days.toFixed(1).padStart(6)}`);
  }
  const byArea = {};
  for (const r of allRows) {
    byArea[r.area] ??= { files: 0, loc: 0, days: 0 };
    byArea[r.area].files += 1;
    byArea[r.area].loc += r.loc;
    byArea[r.area].days += r.days;
  }
  const areas = Object.keys(byArea).sort((a, b) => {
    const oa = ORDER.findIndex((p) => a === p || a.startsWith(`${p}/`));
    const ob = ORDER.findIndex((p) => b === p || b.startsWith(`${p}/`));
    return (oa === -1 ? 999 : oa) - (ob === -1 ? 999 : ob) || a.localeCompare(b);
  });
  console.log('\narea                        files     loc   days  tasks');
  for (const a of areas) {
    const v = byArea[a];
    const n = tasks.filter((t) => t.area === a).length;
    console.log(`${a.padEnd(26)} ${String(v.files).padStart(6)} ${String(v.loc).padStart(7)} ${v.days.toFixed(1).padStart(6)} ${String(n).padStart(6)}`);
  }
  console.log(`\ntotal ${totals.files} files, ${totals.loc} loc, ${totals.tasks} tasks, ${totals.days} working days on one track`);
}
