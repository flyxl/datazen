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
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  WAPP_ASSET_CSP,
  isSafeWappAssetPath,
  parseWappAssetTarget,
  WAPP_ASSET_PATH_PREFIX,
  attachBridge,
  BRIDGE_CHANNEL,
  BRIDGE_ERROR,
  type WappBridgeHandle,
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
    // %c0%ae（overlong `.`）与 %c0%af（overlong `/`）有损替换后仍不得绕过
    // 分量校验：替换产物要么保留非法字节形（拒绝），要么落到被拒分量。
    for (const evil of [
      `${WAPP_ASSET_PATH_PREFIX}acme.bill-audit/%c0%ae%c0%ae/settings.json`,
      `${WAPP_ASSET_PATH_PREFIX}acme.bill-audit/dir/%c0%ae.storage.json`,
      `${WAPP_ASSET_PATH_PREFIX}acme.bill-audit/%c0%afsecret.html`,
    ]) {
      expect(() => parseWappAssetTarget(evil)).toThrowError(/unsafe path|invalid wapp host/);
    }
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
    expect(find('connect-src')).toBe('connect-src none');
    // default-src 单源兜底。
    expect(find('default-src')).toBe("default-src 'self'");
    // 任何指令都不得出现 datazen: 源（web 形态没有自定义 scheme）。
    expect(WAPP_ASSET_CSP).not.toContain('datazen:');
  });
});

// ---------------------------------------------------------------------------
// 3. iframe 握手故障：跨源/伪装信封的「不回答」面
// ---------------------------------------------------------------------------

type SentEnvelope = Record<string, unknown>;

interface FakeFrame {
  iframe: HTMLIFrameElement;
  sent: SentEnvelope[];
}

function makeFakeIframe(): FakeFrame {
  const sent: SentEnvelope[] = [];
  const fakeWindow = {
    postMessage: vi.fn((data: unknown) => {
      sent.push(data as SentEnvelope);
    }),
  };
  const iframe = document.createElement('iframe');
  Object.defineProperty(iframe, 'contentWindow', {
    configurable: true,
    get: () => fakeWindow,
  });
  return { iframe, sent };
}

function receive(data: unknown, source: unknown): void {
  window.dispatchEvent(
    new MessageEvent('message', { data, source: source as Window | null }),
  );
}

const flush = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 0));

async function waitUntil(cond: () => boolean, tries = 40): Promise<boolean> {
  for (let i = 0; i < tries && !cond(); i += 1) {
    await flush();
  }
  return cond();
}

describe('wapp bridge freeze: handshake fault injections', () => {
  let frame: FakeFrame;
  let handle: WappBridgeHandle | null = null;

  beforeEach(() => {
    vi.clearAllMocks();
    frame = makeFakeIframe();
  });

  afterEach(() => {
    handle?.detach();
    handle = null;
  });

  function attachAll(): WappBridgeHandle {
    return attachBridge(frame.iframe, {
      wappId: 'acme.bill-audit',
      permissions: ['context:connections', 'command:invoke', 'storage:local'],
    });
  }

  it('a foreign-window source gets no answer at all (source isolation is freeze-grade)', async () => {
    // 既有测试钉「兄弟 iframe 不能串话」；冻结级补钉：任意**非 contentWindow
    // 的源**（父窗口伪装/恶意 helper）同样零回答——不做任何路由、不发任何帧。
    handle = attachAll();
    const foreign = window; // 最具迷惑性的源：宿主自己
    receive(
      { ch: BRIDGE_CHANNEL, target: 'host', type: 'context.getConnections', reqId: 'fx-1' },
      foreign,
    );
    receive(
      { ch: BRIDGE_CHANNEL, target: 'wapp', type: 'context.getConnections', reqId: 'fx-2' },
      frame.iframe.contentWindow,
    );
    // fx-2：target 错（不是 'host'）→ 同样不回答。
    await waitUntil(() => frame.sent.length > 0, 12);
    expect(frame.sent).toHaveLength(0);
  });

  it('a malformed channel field is ignored without answering (ch mismatch)', async () => {
    handle = attachAll();
    // ch 不是 BRIDGE_CHANNEL、或整个 ch 缺失 → 信封不是桥词汇，零回答。
    receive(
      { ch: 'datazen-wapp-evil', target: 'host', type: 'storage.get', reqId: 'fm-1' },
      frame.iframe.contentWindow,
    );
    receive({ target: 'host', type: 'storage.get', reqId: 'fm-2' }, frame.iframe.contentWindow);
    await waitUntil(() => frame.sent.length > 0, 12);
    expect(frame.sent).toHaveLength(0);
  });

  it('the malformed envelope that IS answered: routed type + good channel + missing payload', async () => {
    // 冻结对照：合法信封 + 路由内类型缺 payload → 有回答（E_BAD_REQUEST），
    // 证明上面的「零回答」是路由门语义，不是桥整体失效。
    handle = attachAll();
    receive(
      { ch: BRIDGE_CHANNEL, target: 'host', type: 'storage.get', reqId: 'fp-1' },
      frame.iframe.contentWindow,
    );
    const answered = await waitUntil(() => frame.sent.some((m) => m.reqId === 'fp-1'), 30);
    expect(answered).toBe(true);
    const sent = frame.sent.find((m) => m.reqId === 'fp-1');
    expect((sent!.payload as { code: string }).code).toBe(BRIDGE_ERROR.BAD_REQUEST);
  });
});
