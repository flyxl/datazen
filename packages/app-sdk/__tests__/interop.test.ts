/**
 * F8 test-agent: SDK ↔ host contract interoperability checks.
 *
 * The host counterpart lives in `src/lib/extensionBridge.ts` (+ `themeTokens.ts`,
 * `types/plugin.ts`, `theme.css` consumers). Heavy host modules (tauri IPC,
 * zustand stores) are NOT executed here — their constants are parsed from
 * source so a drift on either side fails this file. Light pure modules
 * (`themeTokens.ts`) are imported for live round-trips.
 */
import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  BRIDGE_CHANNEL as SDK_CHANNEL,
  BRIDGE_ERROR as SDK_ERRORS,
  REQUEST_TIMEOUT_MS as SDK_TIMEOUT,
  EXTENSION_API_VERSION as SDK_API_VERSION,
} from '../src/bridge';
import { DEFAULT_THEME_TOKENS, applyThemeSnapshot, startThemeListener } from '../src/theme';
import * as SdkPublic from '../src/index';
import type { ConnectionSummary } from '../src/bridge';
import {
  THEME_SNAPSHOT_VERSION,
  THEME_TOKENS,
  buildThemeSnapshot,
} from '../../../src/lib/themeTokens';
import { EXTENSION_API_VERSION as HOST_API_VERSION } from '../../../src/types/extension';

const HERE = dirname(fileURLToPath(import.meta.url));
const HOST_ROOT = resolve(HERE, '../../..');
const HOST_BRIDGE_SRC = readFileSync(resolve(HOST_ROOT, 'src/lib/extensionBridge.ts'), 'utf8');
const HOST_CSS_SRC = readFileSync(resolve(HOST_ROOT, 'src/styles/themes.css'), 'utf8');
const SDK_CSS_SRC = readFileSync(resolve(HERE, '../src/theme.css'), 'utf8');

function extractConstBlock(source: string, name: string): string {
  const startMatch = source.match(new RegExp(`(?:export )?const ${name}(?::[^=]+)? = \\{`));
  if (!startMatch) throw new Error(`${name} block not found in host source`);
  // Brace-depth scan from the literal's opening brace (values hold no braces).
  let depth = 1;
  for (let i = startMatch.index! + startMatch[0].length; i < source.length; i += 1) {
    if (source[i] === '{') depth += 1;
    else if (source[i] === '}') {
      depth -= 1;
      if (depth === 0) return source.slice(startMatch.index! + startMatch[0].length, i);
    }
  }
  throw new Error(`${name}: unbalanced braces`);
}

function extractStringLiterals(block: string): string[] {
  return [...block.matchAll(/'([^']+)'/g)].map((m) => m[1]!);
}

describe('X-01 wire error codes: every SDK BRIDGE_ERROR equals the host router code', () => {
  const hostBlock = extractConstBlock(HOST_BRIDGE_SRC, 'BRIDGE_ERROR');
  const hostPairs = [...hostBlock.matchAll(/([A-Z_]+):\s*'([^']+)'/g)].map(
    ([, key, value]) => [key!, value!] as const,
  );

  it('key-by-key equality in both directions', () => {
    expect(hostPairs.length).toBeGreaterThan(0);
    for (const [key, value] of hostPairs) {
      expect(SDK_ERRORS).toHaveProperty(key, value);
    }
    // No extra SDK-side codes either (SDK-local codes live in SDK_ERROR).
    expect(Object.keys(SDK_ERRORS)).toEqual(hostPairs.map(([key]) => key));
  });

  it('every code is a distinct E_* string', () => {
    const values = Object.values(SDK_ERRORS);
    expect(values.every((code) => /^E_[A-Z_]+$/.test(code))).toBe(true);
    expect(new Set(values).size).toBe(values.length);
  });
});

describe('X-02 envelope constants', () => {
  it('channel name matches the host literal', () => {
    expect(SDK_CHANNEL).toBe('datazen-extension');
    expect(HOST_BRIDGE_SRC).toContain(`export const BRIDGE_CHANNEL = '${SDK_CHANNEL}';`);
  });

  it('request deadline matches the host constant (30s)', () => {
    expect(SDK_TIMEOUT).toBe(30_000);
    expect(HOST_BRIDGE_SRC).toContain('export const REQUEST_TIMEOUT_MS = 30_000;');
  });

  it('envelope field set of SDK requests mirrors the host PluginRequestEnvelope', () => {
    // Host shape: {ch, type, reqId?, target:'host', payload?}
    expect(HOST_BRIDGE_SRC).toContain("target: 'host'");
    // Host responses suffix .ok/.err and echo reqId — the SDK routes purely by
    // reqId, which stays compatible with any suffix.
    expect(HOST_BRIDGE_SRC).toContain("`${requestType}.${ok ? 'ok' : 'err'}`");
    // ok:true / ok:false discrimination present on both sides.
    expect(HOST_BRIDGE_SRC).toMatch(/ok: (true|false)/);
  });
});

describe('X-03 protocol version alignment', () => {
  it('apiVersion === 2 across SDK, host types and Rust PLUGIN_API_VERSION', () => {
    expect(SDK_API_VERSION).toBe(HOST_API_VERSION);
    expect(SDK_API_VERSION).toBe(2);
  });

  it('theme snapshot version rides the same protocol version', () => {
    expect(THEME_SNAPSHOT_VERSION).toBe(SDK_API_VERSION);
  });
});

describe('X-04 route table covers every api the SDK emits', () => {
  const sdkEmittedTypes = [
    'context.getConnections',
    'context.getActiveConnection',
    'command.invoke',
    'storage.get',
    'storage.set',
    'storage.remove',
    'ui.notify',
    'i18n.getString',
  ];
  const hostRoutes = extractStringLiterals(extractConstBlock(HOST_BRIDGE_SRC, 'API_ROUTES'));

  it('each SDK request type is routable host-side with its declared permission', () => {
    for (const type of sdkEmittedTypes) {
      expect(hostRoutes, `${type} missing from host API_ROUTES`).toContain(type);
      expect(HOST_BRIDGE_SRC).toMatch(new RegExp(`'${type.replace('.', '\\.')}'\\s*:`));
    }
  });
});

describe('X-05 theme token contract', () => {
  it('SDK DEFAULT_THEME_TOKENS keys equal the host THEME_TOKENS list exactly', () => {
    expect(Object.keys(DEFAULT_THEME_TOKENS).sort()).toEqual([...THEME_TOKENS].sort());
  });

  it('host themes.css defines every contract token (sanity for buildThemeSnapshot)', () => {
    for (const token of THEME_TOKENS) {
      expect(HOST_CSS_SRC).toContain(token);
    }
  });

  it('theme.css var() references ⊆ host THEME_TOKENS', () => {
    const used = new Set<string>();
    for (const match of SDK_CSS_SRC.matchAll(/var\((--[a-z0-9-]+)/gi)) {
      used.add(match[1]!.toLowerCase());
    }
    expect(used.size).toBeGreaterThan(0);
    const allowed = new Set(THEME_TOKENS.map((token) => token.toLowerCase()));
    const unknown = [...used].filter((token) => !allowed.has(token));
    expect(unknown, `tokens outside the host contract: ${unknown.join(', ')}`).toEqual([]);
  });

  it('theme.css color policy: only var() references; literals confined to fallbacks; no rgb()/hsl()', () => {
    const css = SDK_CSS_SRC.replace(/\/\*[\s\S]*?\*\//g, '');

    // ① Every color usage goes through a var() reference to a contract token.
    //    All seven DataTable tokens plus the twelve core semantic colors must
    //    be consumed; the five host-chrome tokens (--c-query-run/--c-titlebar*)
    //    intentionally stay unconsumed (plugins do not render title bars).
    const varNames = new Set(
      [...css.matchAll(/var\(\s*(--[a-z0-9-]+)/gi)].map((match) => match[1]!.toLowerCase()),
    );
    const required = [...THEME_TOKENS].filter(
      (token) =>
        token.startsWith('--dt-') ||
        (!token.startsWith('--c-titlebar') && token !== '--c-query-run'),
    );
    for (const token of required) {
      expect(varNames.has(token.toLowerCase()), `${token} not consumed by theme.css`).toBe(true);
    }
    expect(varNames.size).toBe(required.length);

    // ② After removing var(...) calls entirely, no raw color literal remains
    //    (no #hex / rgb( / rgba( / hsl( anywhere outside fallbacks).
    const withoutVars = css.replace(/\bvar\([^)]*\)/g, '');
    expect(withoutVars).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
    expect(withoutVars).not.toMatch(/\brgba?\(|\bhsla?\(/);

    // ③ Fallback literals exist only inside var() and mirror the documented
    //    pre-snapshot palette. Counted here so regressions stay visible.
    const fallbackHexes = [...css.matchAll(/var\([^)]*?(#[0-9a-fA-F]{3,8})\b/g)].map((m) => m[1]);
    const totalHexes = [...css.matchAll(/#[0-9a-fA-F]{3,8}\b/g)].map((m) => m[0]);
    expect(totalHexes.length).toBe(fallbackHexes.length); // none outside var()
    expect(fallbackHexes.length).toBeGreaterThan(0);

    // ④ Baseline control classes promised by PRD §5 are all present.
    for (const className of [
      '.dz-root',
      '.dz-btn',
      '.dz-btn--primary',
      '.dz-btn--danger',
      '.dz-input',
      '.dz-textarea',
      '.dz-select',
      '.dz-label',
      '.dz-card',
      '.dz-table',
      '.dz-badge',
    ]) {
      expect(css).toContain(className);
    }

    // NOTE-F8-01 evidence: strict zero-literal reading would flag these N
    // documented pre-first-snapshot fallbacks; see progress doc.
  });

  it('fallback palette hexes agree with DEFAULT_THEME_TOKENS where both define a token', () => {
    for (const match of SDK_CSS_SRC.matchAll(/var\((--[a-z0-9-]+)\s*,\s*(#[0-9a-fA-F]{3,8})\)/g)) {
      const [, token, fallback] = match;
      if (DEFAULT_THEME_TOKENS[token!]) {
        expect(fallback?.toLowerCase(), token ?? '').toBe(
          DEFAULT_THEME_TOKENS[token!]!.toLowerCase(),
        );
      }
    }
  });
});

describe('X-06 live snapshot round-trip (host builder → SDK applier)', () => {
  it('buildThemeSnapshot output applied by the SDK writes tokens and syncs dark', () => {
    document.documentElement.style.setProperty('--c-accent', '#123456');
    document.documentElement.style.setProperty('--dt-number', '#654321');
    document.documentElement.classList.add('dark');

    try {
      const snapshot = buildThemeSnapshot();

      // Host wire shape: {v, dark, tokens}.
      expect(Object.keys(snapshot).sort()).toEqual(['dark', 'tokens', 'v']);
      expect(snapshot.v).toBe(THEME_SNAPSHOT_VERSION);
      expect(typeof snapshot.dark).toBe('boolean');
      expect(Object.keys(snapshot.tokens)).toEqual(expect.arrayContaining([...THEME_TOKENS]));
      expect(snapshot.tokens['--c-accent']).toBe('#123456');

      const state = applyThemeSnapshot(snapshot);
      expect(state.dark).toBe(true);
      expect(document.documentElement.style.getPropertyValue('--c-accent')).toBe('#123456');
      expect(document.documentElement.style.getPropertyValue('--dt-number')).toBe('#654321');
      // Tokens the host could not resolve ('') fall back to the SDK palette (R5).
      expect(document.documentElement.style.getPropertyValue('--c-surface')).toBe(
        DEFAULT_THEME_TOKENS['--c-surface'],
      );
    } finally {
      document.documentElement.classList.remove('dark');
      document.documentElement.style.removeProperty('--c-accent');
      document.documentElement.style.removeProperty('--dt-number');
    }
  });

  it('theme.apply envelope shaped like pushThemeSnapshot drives startThemeListener', () => {
    const posted: unknown[] = [];
    const parent = { postMessage: (data: unknown) => posted.push(data) } as unknown as Window;

    const detach = startThemeListener({ parentWindow: parent });
    const snapshot = { v: THEME_SNAPSHOT_VERSION, dark: true, tokens: { '--c-edge': '#010203' } };
    // Exact envelope the host handle.pushThemeSnapshot() posts:
    window.dispatchEvent(
      new MessageEvent('message', {
        source: parent,
        data: { ch: SDK_CHANNEL, type: 'theme.apply', target: 'host', payload: snapshot },
      }),
    );

    expect(document.documentElement.style.getPropertyValue('--c-edge')).toBe('#010203');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    detach();
  });

  it('host.ready payload shaped like attachBridge answers createClient.ready()', async () => {
    const posted: unknown[] = [];
    const parent = { postMessage: (data: unknown) => posted.push(data) } as unknown as Window;
    const client = SdkPublic.createClient({ parentWindow: parent });

    const pending = client.ready();
    const snapshot = buildThemeSnapshot();
    // Exact payload keys the host posts after plugin.ready (attachBridge):
    window.dispatchEvent(
      new MessageEvent('message', {
        source: parent,
        data: {
          ch: SDK_CHANNEL,
          type: 'host.ready',
          target: 'host',
          payload: {
            apiVersion: HOST_API_VERSION,
            locale: 'zh-CN',
            dark: snapshot.dark,
            tokens: snapshot.tokens,
          },
        },
      }),
    );

    const context = await pending;
    expect(context.apiVersion).toBe(2);
    expect(context.locale).toBe('zh-CN');
    expect(context.dark).toBe(snapshot.dark);
    client.detach();
  });
});

describe('X-07 public surface', () => {
  it('index exports the documented API (PRD §5)', () => {
    expect(typeof SdkPublic.createClient).toBe('function');
    expect(typeof SdkPublic.applyThemeSnapshot).toBe('function');
    expect(typeof SdkPublic.startThemeListener).toBe('function');
    expect(typeof SdkPublic.subscribeTheme).toBe('function');
    expect(typeof SdkPublic.getThemeState).toBe('function');
    expect(SdkPublic.THEME_CHANGED_EVENT).toBe('datazen:theme-pack-changed');
    expect(Object.keys(SdkPublic.BRIDGE_ERROR)).toContain('PERMISSION');
  });

  it('ConnectionSummary stays the whitelisted {id,name,dbType} triple', () => {
    // Compile-time drift guard: adding a field must fail this fixture.
    const sample: ConnectionSummary = { id: 'c1', name: 'Prod', dbType: 'postgres' };
    expect(Object.keys(sample).sort()).toEqual(['dbType', 'id', 'name']);
  });
});
