#!/usr/bin/env node
/**
 * Server 依赖闭包门禁（team-server-and-auth §2.2；web-ready WR-D1 随 server
 * crate 一并新增，与 check-module-layers / check-driver-import-boundaries 并列，
 * 不塞进它们的既有职责）。
 *
 * ## 它断什么
 *
 * `datazen-server` 的依赖闭包里不得出现任何宿主/UI crate：
 *
 * - `tauri`、`tauri-plugin-*`、`tauri-build`、`tauri-codegen`……一切 `tauri` 前缀；
 * - `wry` / `tao`（WebView 与其窗口层）；
 * - `datazen_desktop`（桌面宿主 crate；注意 `datazen`（modules 内核）是**允许**
 *   且预期的——两个 crate 名只差一个词，这是本脚本存在的原因之一：人眼
 *   review 分不清「依赖了内核」和「依赖了桌面宿主」）；
 * - 前端 UI runtime（dioxus / egui / iced / slint / leptos / yew / sycamore / gpui）。
 *
 * ## 它怎么跑
 *
 * 输入是 `cargo tree` 的输出文件（每行一个 crate 条目），CI 里这样接：
 *
 *   cargo tree -p datazen-server --edges normal,build --prefix none > /tmp/server-deps.txt
 *   node scripts/check-server-deps.mjs /tmp/server-deps.txt
 *
 * 解析规则：每行取第一个空白分隔的 token 作为 crate 名（兼容
 * `name v1.2.3 (path+…) 尾缀与 `(*)` 折叠标记）；空行与 `#` 注释跳过。
 * 这样不依赖 cargo tree 的精确格式，升级 cargo 不改这里。
 *
 * 「文件读不到」「一个名字都没解析出来」都是 error 而不是 pass——「没读到」
 * 和「没问题」不允许打印成同一行（仓库门禁的统一纪律）。
 *
 * ## 自测
 *
 * `node scripts/check-server-deps.mjs --self-test` 用内置的三组样本跑判定
 * 逻辑（干净闭包过 / tauri 混入败 / datazen_desktop 混入败 / datazen 内核放行），
 * 供 CI 的变异自证与本脚本的独立回归使用。
 */

import { readFileSync } from 'node:fs';

const PREFIX = '[server-deps]';

/** 禁止出现在闭包里的 crate。前缀匹配用 `prefix:` 标记。 */
const FORBIDDEN = [
  { name: 'tauri', kind: 'prefix' },
  { name: 'wry', kind: 'exact' },
  { name: 'tao', kind: 'exact' },
  { name: 'datazen_desktop', kind: 'exact' },
  { name: 'dioxus', kind: 'exact' },
  { name: 'egui', kind: 'exact' },
  { name: 'iced', kind: 'exact' },
  { name: 'slint', kind: 'exact' },
  { name: 'leptos', kind: 'exact' },
  { name: 'yew', kind: 'exact' },
  { name: 'sycamore', kind: 'exact' },
  { name: 'gpui', kind: 'exact' },
];

/** 一行 `cargo tree` 输出 → crate 名（取首 token）；非条目行返回 null。 */
function crateNameOf(line) {
  const first = line.trim().split(/\s+/)[0];
  if (!first || first.startsWith('#') || first === '(*)') return null;
  return first;
}

/** 判定：返回违规数组（空 = 通过）。`datazen` 是允许的内核依赖。 */
function violationsOf(names) {
  const hits = [];
  for (const name of names) {
    for (const rule of FORBIDDEN) {
      const hit =
        rule.kind === 'prefix' ? name === rule.name || name.startsWith(`${rule.name}-`) : name === rule.name;
      if (hit) hits.push({ name, rule });
    }
  }
  return hits;
}

/** 文本输入 → crate 名列表（去重保序）。 */
function parse(text) {
  const names = [];
  const seen = new Set();
  for (const line of text.split('\n')) {
    const name = crateNameOf(line);
    if (name !== null && !seen.has(name)) {
      seen.add(name);
      names.push(name);
    }
  }
  return names;
}

function fail(message) {
  console.error(`${PREFIX} FAIL ${message}`);
}

function main(argv) {
  if (argv.includes('--self-test')) return selfTest();

  const input = argv.find((arg) => !arg.startsWith('-'));
  if (!input) {
    fail('usage: check-server-deps.mjs <cargo-tree-output-file | --self-test>');
    return 2;
  }

  let text;
  try {
    text = readFileSync(input, 'utf8');
  } catch (error) {
    fail(`cannot read ${input}: ${error.message}`);
    return 2;
  }

  const names = parse(text);
  if (names.length === 0) {
    fail(`no crate entries parsed from ${input} — a gate that read nothing is not a pass`);
    return 2;
  }

  const violations = violationsOf(names);
  if (violations.length > 0) {
    for (const { name, rule } of violations)
      fail(`forbidden crate in the datazen-server dependency closure: ${name} (rule: ${rule.kind} ${rule.name})`);
    fail(
      `${violations.length} forbidden crate(s). The server host must depend on the shared ` +
        `kernel (datazen), never on the desktop host (datazen_desktop) or any UI/WebView layer — ` +
        `see docs/architecture/platform/team-server-and-auth.md §2.2.`,
    );
    return 1;
  }

  console.log(`${PREFIX} ${names.length} crates in closure, 0 forbidden (host/UI-free).`);
  return 0;
}

/** 内置样本回归：四组输入，三组须败一组须过。 */
function selfTest() {
  const cases = [
    {
      what: 'clean closure passes (kernel allowed)',
      text: ['datazen v0.1.0 (path+file:///…/modules)', 'axum v0.8.4', 'tokio v1.47.0', 'serde v1.0.219 (*)'].join('\n'),
      expect: 0,
    },
    {
      what: 'tauri plugin sneaks in',
      text: ['datazen v0.1.0', 'tauri-plugin-dialog v2.7.0', 'axum v0.8.4'].join('\n'),
      expect: 1,
    },
    {
      what: 'desktop host crate sneaks in (the near-miss name)',
      text: ['datazen v0.1.0', 'datazen_desktop v0.1.0 (path+…/src-tauri)'].join('\n'),
      expect: 1,
    },
    {
      what: 'wry WebView sneaks in',
      text: ['axum v0.8.4', 'wry v0.51.2'].join('\n'),
      expect: 1,
    },
  ];
  let failures = 0;
  for (const { what, text, expect } of cases) {
    const violations = violationsOf(parse(text));
    const bad = expect === 0 ? violations.length !== 0 : violations.length === 0;
    if (bad) {
      failures += 1;
      console.error(`${PREFIX} self-test FAIL: ${what} (expected ${expect} violation(s), got ${violations.length})`);
    } else {
      console.log(`${PREFIX} self-test ok: ${what}`);
    }
  }
  // 空输入必须被判为「没读到」而不是通过。
  if (parse('').length !== 0) {
    failures += 1;
    console.error(`${PREFIX} self-test FAIL: empty input must parse to zero names`);
  } else {
    console.log(`${PREFIX} self-test ok: empty input parses to zero names`);
  }
  if (failures > 0) {
    console.error(`${PREFIX} self-test: ${failures} case(s) failed`);
    return 1;
  }
  console.log(`${PREFIX} self-test: all cases green`);
  return 0;
}

process.exitCode = main(process.argv.slice(2));
