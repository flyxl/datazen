import { describe, expect, it } from 'vitest';

import {
  DESKTOP_WAPP_ASSET_RESOLVER,
  WAPP_ASSET_CSP,
  WAPP_ASSET_PATH_PREFIX,
  WEB_WAPP_ASSET_RESOLVER,
  buildWappAssetSrc,
  buildWappIconSrc,
  configureWappAssetResolver,
  isSafeWappAssetPath,
  isValidWappId,
  parseWappAssetTarget,
  resetWappAssetResolver,
} from '@datazen/frontend';

/**
 * WR-F4 资产服务协议（web 形态）：`datazen://` 自定义 scheme 的替换形。
 *
 * 约定（协议形状）：`{origin}/wapps/{wapp id}/{package-relative path}?{query}`。
 * 与桌面 `parse_datazen_uri`（modules/src/wapps/protocol.rs）同构的文法与
 * 拒绝语义：publisher 点号 id 文法、解码后路径安全（隐藏分量/穿越/反斜杠/
 * NUL）、宽松 percent-decode。本套件钉 web 侧一半；桌面半侧由
 * apps/desktop `wapps/protocol.rs` 的 Rust 套件钉（两形态同绿）。
 */

describe('wapp asset origin convention (WR-F4)', () => {
  it('web resolver serves origin-relative entry/icon URLs; desktop keeps the datazen:// scheme', () => {
    // web：真实 origin 静态服务（同源相对路径，`?v=` 缓存破坏符与桌面一致）。
    expect(WEB_WAPP_ASSET_RESOLVER.buildSrc('acme.bill-audit', 'index.html', '1.0.0')).toBe(
      '/wapps/acme.bill-audit/index.html?v=1.0.0',
    );
    expect(WEB_WAPP_ASSET_RESOLVER.buildIconSrc('acme.bill-audit', 'assets/icon.svg')).toBe(
      '/wapps/acme.bill-audit/assets/icon.svg',
    );
    // ./ 前缀规整与桌面 buildSrc 相同。
    expect(WEB_WAPP_ASSET_RESOLVER.buildSrc('acme.bill-audit', './main.html', '2.0.0')).toBe(
      '/wapps/acme.bill-audit/main.html?v=2.0.0',
    );
    // 桌面：自定义 scheme，产物逐字节不变（路径不动）。
    expect(DESKTOP_WAPP_ASSET_RESOLVER.buildSrc('acme.bill-audit', 'index.html', '1.0.0')).toBe(
      'datazen://acme.bill-audit/index.html?v=1.0.0',
    );
    expect(DESKTOP_WAPP_ASSET_RESOLVER.buildIconSrc('acme.bill-audit', 'assets/icon.svg')).toBe(
      'datazen://acme.bill-audit/assets/icon.svg',
    );
  });

  it('build helpers resolve through the assembly-time binding (no runtime form probe)', () => {
    // 默认绑定 = 桌面（共享壳层不绑定的形态里行为与旧字面量一致）。
    expect(buildWappAssetSrc('acme.bill-audit', 'index.html', '1.0.0')).toBe(
      'datazen://acme.bill-audit/index.html?v=1.0.0',
    );
    configureWappAssetResolver(WEB_WAPP_ASSET_RESOLVER);
    expect(buildWappAssetSrc('acme.bill-audit', 'index.html', '1.0.0')).toBe(
      '/wapps/acme.bill-audit/index.html?v=1.0.0',
    );
    expect(buildWappIconSrc('acme.bill-audit', 'assets/icon.svg')).toBe(
      '/wapps/acme.bill-audit/assets/icon.svg',
    );
    resetWappAssetResolver();
    expect(buildWappIconSrc('acme.bill-audit', 'assets/icon.svg')).toBe(
      'datazen://acme.bill-audit/assets/icon.svg',
    );
  });

  it('parses the origin-relative form back into (wappId, path, query) with desktop parity', () => {
    const target = parseWappAssetTarget('/wapps/acme.bill-audit/open?page=quota-check&uid=123');
    expect(target.wappId).toBe('acme.bill-audit');
    expect(target.path).toBe('open');
    expect(target.query).toEqual({ page: 'quota-check', uid: '123' });

    // 宽松 percent-decode：`+` 为空格、转义在校验之前解码。
    const decoded = parseWappAssetTarget('/wapps/acme.bill-audit/assets%2Ficon.svg?q=a%20b');
    expect(decoded.path).toBe('assets/icon.svg');
    expect(decoded.query).toEqual({ q: 'a b' });

    // 完整 URL 形（绝对 origin）也收。
    const absolute = parseWappAssetTarget('https://app.example.com/wapps/acme.one/data.json');
    expect(absolute).toEqual({ wappId: 'acme.one', path: 'data.json', query: {} });
  });

  it('rejects the same inputs the desktop parser rejects', () => {
    // 未知前缀（不是本服务的路径）。
    expect(() => parseWappAssetTarget('/plugins/acme.bill-audit/index.html')).toThrowError(
      /unsupported wapp asset path/,
    );
    // host 不是合法 wapp id：缺 publisher 点号 / 大写 / 空段。
    for (const host of ['bill-audit', 'Acme.bill-audit', 'acme.', '.bill']) {
      expect(() => parseWappAssetTarget(`${WAPP_ASSET_PATH_PREFIX}${host}/index.html`)).toThrowError(
        /invalid wapp host/,
      );
    }
    // 空路径。
    expect(() => parseWappAssetTarget('/wapps/acme.bill-audit')).toThrowError(/unsafe path/);
    // 解码后穿越 / 隐藏文件 / 反斜杠 / NUL —— 与桌面 safe_relative_path 同表。
    for (const evil of [
      '/wapps/acme.bill-audit/%2e%2e/%2e%2e/settings.json',
      '/wapps/acme.bill-audit/.storage.json',
      '/wapps/acme.bill-audit/.enabled',
      '/wapps/acme.bill-audit/dir/.secret.png',
      '/wapps/acme.bill-audit/a%5Cb.html',
      '/wapps/acme.bill-audit/%00.html',
    ]) {
      expect(() => parseWappAssetTarget(evil)).toThrowError(/unsafe path/);
    }
    // 纯函数半边与桌面逐条对齐。
    expect(isValidWappId('a0.b-c')).toBe(true);
    expect(isValidWappId('-lead.bill')).toBe(false);
    expect(isSafeWappAssetPath('a/b/c/d.json')).toBe(true);
    expect(isSafeWappAssetPath('')).toBe(false);
    expect(isSafeWappAssetPath('..')).toBe(false);
  });

  it('CSP is single-source: same directive set as the desktop ASSET_CSP minus the datazen: source', () => {
    // 桌面双源（'self' datazen:）随真实 origin 收敛为单源（'self'）。
    expect(WAPP_ASSET_CSP).not.toContain('datazen:');
    for (const directive of [
      "default-src 'self'",
      "script-src 'self'",
      "style-src 'self' 'unsafe-inline'",
      "img-src 'self' data:",
      "font-src 'self'",
      "connect-src 'none'",
    ]) {
      expect(WAPP_ASSET_CSP).toContain(directive);
    }
    // 插件离线语义不变；data: 仍仅限图片指令。
    const img = WAPP_ASSET_CSP.split(';').find((d) => d.trim().startsWith('img-src'));
    expect(img).toContain('data:');
  });
});
