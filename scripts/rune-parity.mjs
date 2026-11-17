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

const sorted = [...rows].sort(
  (a, b) => (a.order === -1 ? 999 : a.order) - (b.order === -1 ? 999 : b.order) || a.react.localeCompare(b.react),
);

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

const tasks = bucket(sorted);
const args = process.argv.slice(2);
const flag = (name, fallback = null) => {
  const i = args.indexOf(name);
  return i >= 0 ? (args[i + 1] ?? true) : fallback;
};

const totals = {
  files: rows.length,
  loc: rows.reduce((n, r) => n + r.loc, 0),
  days: Math.round(rows.reduce((n, r) => n + r.days, 0) * 10) / 10,
  tasks: tasks.length,
};

if (flag('--json')) {
  writeFileSync(flag('--json'), JSON.stringify({ generatedAt: new Date().toISOString(), totals, tasks, files: rows }, null, 2));
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
  for (const r of rows) {
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
  for (const r of rows) {
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
