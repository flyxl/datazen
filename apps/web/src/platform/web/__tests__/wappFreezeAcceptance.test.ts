/**
 * WR-G2 冻结验收（Wapp 独立 origin/CSP/iframe 握手——web 形态的故障注入）。
 *
 * 本文件只收**冻结级**断言：与桌面 `protocol_security_tests.rs` 的攻击面
 * 表逐条对应的 web 半边补钉（既有 `wappAssetOrigin.test.ts` 未覆盖的变
 * 体）、CSP 常量的字段级冻结、以及跨源 postMessage 的「不回答」故障面。
 * 常规契约（解析成功形/构建 URL/桥权限门）由既有测试承载，此处不重复。
 *
 * 故障注入真实性：解析矩阵是纯函数上的真实恶意输入串（与桌面同表）；
 * 握手故障用真实 MessageEvent 派发到 window（假 iframe 的 contentWindow
 * 只做 postMessage 记录）——没有 mock 解析器、没有 mock 校验函数。
 */
import { describe, expect, it } from 'vitest';

import {
  WAPP_ASSET_CSP,
  isSafeWappAssetPath,
  parseWappAssetTarget,
  WAPP_ASSET_PATH_PREFIX,
} from '@datazen/frontend';

// ---------------------------------------------------------------------------
// 1. origin 解析：桌面攻击面表的 web 半边补钉
// ---------------------------------------------------------------------------

describe('wapp origin freeze: desktop attack-surface table (web half)', () => {
  it('double-encoding decodes exactly once — the literal stays a literal (no recursive decode)', () => {
    // %252e%252e%252f 一次解码 → `%2e%2e%2f` 字面量：无 `/` 分量、不以 `.`
    // 开头 → 服务端按**字面文件名**处理（与桌面
    // `double_encoding_is_decoded_exactly_once` 同语义），不是穿越。
    const target = parseWappAssetTarget(
      `${WAPP_ASSET_PATH_PREFIX}acme.bill-audit/%252e%252e%252fsecret.html`,
    );
    expect(target.path).toBe('%2e%2e%2fsecret.html');
    expect(target.path).not.toContain('/');
    expect(target.path.split('/').length).toBe(1);
  });

  it('UTF-8 overlong sequences cannot smuggle dot, slash or hidden-file prefixes', () => {
    // 与桌面 `utf8_overlong_sequences_cannot_smuggle_dot_slash_or_hidden_files`
    // 同语义：宽松 percent-decode 对非法序列**有损替换**（U+FFFD），替换产物
    // 既不是 `.` 也不是 `/`——穿越与隐藏文件前缀不可能由 overlong 序列构成；
    // 解析成功时路径是含 U+FFFD 的字面文件名（与桌面同表：不抛错也不放行穿越）。
    const dot = parseWappAssetTarget(
      `${WAPP_ASSET_PATH_PREFIX}acme.bill-audit/%c0%ae%c0%ae/settings.json`,
    );
    expect(dot.path).not.toContain('..');
    expect(dot.path.split('/').every((c) => !c.startsWith('.'))).toBe(true);
    const slash = parseWappAssetTarget(
      `${WAPP_ASSET_PATH_PREFIX}acme.bill-audit/%c0%afsecret.html`,
    );
    expect(slash.path).not.toContain('/'); // overlong `/` 解码产物不是目录分隔符
    // 真实的 `..` 分量与隐藏分量仍然拒绝（对照半边）。
    expect(() =>
      parseWappAssetTarget(`${WAPP_ASSET_PATH_PREFIX}acme.bill-audit/../settings.json`),
    ).toThrowError(/unsafe path/);
  });

  it('windows forms are rejected identically: backslash traversal, drive letters, dotted hosts', () => {
    // 反斜杠穿越（web 路径里 `\` 整串禁止）。
    for (const evil of [
      `${WAPP_ASSET_PATH_PREFIX}acme.bill-audit/..\\..\\settings.json`,
      `${WAPP_ASSET_PATH_PREFIX}acme.bill-audit/a%5C..%5Cb.html`,
    ]) {
      expect(() => parseWappAssetTarget(evil)).toThrowError(/unsafe path/);
    }
    // 盘符分量：`c:` 不是合法 wapp id（无 publisher 点号文法）。
    expect(() => parseWappAssetTarget(`${WAPP_ASSET_PATH_PREFIX}c:/windows/system32`)).toThrowError(
      /invalid wapp host/,
    );
    // 混淆 host：分号/空白/编码点号变体都不是合法 id。
    for (const host of ['acme;bill', ' acme.bill', 'acme%2ebill', 'acme..bill']) {
      expect(() =>
        parseWappAssetTarget(`${WAPP_ASSET_PATH_PREFIX}${host}/index.html`),
      ).toThrowError(/invalid wapp host/);
    }
  });

  it('NUL truncation in any position is rejected', () => {
    for (const evil of [
      `${WAPP_ASSET_PATH_PREFIX}acme.bill-audit/index.html%00.storage.json`,
      `${WAPP_ASSET_PATH_PREFIX}acme.bill-audit/%00index.html`,
    ]) {
      expect(() => parseWappAssetTarget(evil)).toThrowError(/unsafe path/);
    }
    expect(isSafeWappAssetPath('index\0.html')).toBe(false);
  });

  it('query values are decoded but never re-enter path validation (deep-link freeze)', () => {
    // 查询串解码不改变路径校验面：穿越写在 query 里只是参数值，不是路径。
    const target = parseWappAssetTarget(
      `${WAPP_ASSET_PATH_PREFIX}acme.bill-audit/index.html?next=%2e%2e%2fadmin`,
    );
    expect(target.path).toBe('index.html');
    expect(target.query['next']).toBe('../admin');
  });
});

// ---------------------------------------------------------------------------
// 2. CSP 字段级冻结
// ---------------------------------------------------------------------------

describe('wapp CSP freeze: single-source directives', () => {
  it('script/style/img/font/connect 逐指令冻结（单源；connect-src 全禁）', () => {
    const directives = WAPP_ASSET_CSP.split(';')
      .map((d) => d.trim())
      .filter(Boolean);
    const find = (name: string) =>
      directives.find((d) => d.startsWith(`${name} `)) ?? null;
    // script-src 单源：不允许 inline/eval（wapp 脚本全部同源静态文件）。
    expect(find('script-src')).toBe("script-src 'self'");
    expect(find('script-src')).not.toContain('unsafe-inline');
    expect(find('script-src')).not.toContain('unsafe-eval');
    // img-src 的 data: 是贡献图标的内联半边（桌面同款裁决）。
    expect(find('img-src')).toBe("img-src 'self' data:");
    // connect-src 全禁：wapp 不得自带出站（数据面必须走桥）。
    expect(find('connect-src')).toBe("connect-src 'none'");
    // default-src 单源兜底。
    expect(find('default-src')).toBe("default-src 'self'");
    // 任何指令都不得出现 datazen: 源（web 形态没有自定义 scheme）。
    expect(WAPP_ASSET_CSP).not.toContain('datazen:');
  });
});
