/**
 * E2E — First-run / system language behavior (SYS-001~SYS-004)
 *
 * SYS-001  get_system_ui_language is in supported set
 * SYS-002  Persisted language is not overwritten by get_settings
 * SYS-003  Switching to a supported language persists across refresh
 * SYS-004  Unsupported stored language still renders English UI fallback
 */
import { expect, browser, $, $$ } from '@wdio/globals';

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: unknown) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as object)) {
    throw new Error((result as { __error: string }).__error);
  }
  return result as T;
}

interface AppSettings {
  theme: string;
  language: string;
  limitSelectResults: boolean;
  queryResultLimit: number;
  editorFontSize: number;
  editorFontFamily: string;
  confirmOnDelete: boolean;
  autoCommit: boolean;
  safeMode: boolean;
  defaultPageSize: number;
  logLevel?: string;
  logPath?: string;
  mcpServerEnabled?: boolean;
  mcpDisabledTools?: string[];
  contextDir?: string;
}

const SUPPORTED = [
  'en',
  'zh-CN',
  'zh-TW',
  'es',
  'fr',
  'de',
  'ja',
  'pt-BR',
  'ru',
  'ko',
] as const;

async function openMain() {
  await browser.url('tauri://localhost');
  await browser.waitUntil(
    async () => (await $$('input')).length > 0,
    { timeout: 20000, timeoutMsg: 'main window not ready' },
  );
  await browser.pause(400);
}

async function searchPlaceholder(): Promise<string> {
  const inputs = await $$('input');
  for (const input of inputs) {
    const ph = await input.getAttribute('placeholder');
    if (ph) return ph;
  }
  return '';
}

describe('System / first-run language (SYS-001~SYS-004)', () => {
  let original: AppSettings;

  before(async () => {
    await browser.pause(800);
    original = await invokeBackend<AppSettings>('get_settings');
  });

  after(async () => {
    await invokeBackend('save_settings', { settings: original });
    await openMain();
  });

  it('SYS-001: get_system_ui_language returns a supported code', async () => {
    const lang = await invokeBackend<string>('get_system_ui_language');
    expect(SUPPORTED.includes(lang as (typeof SUPPORTED)[number])).toBe(true);
  });

  it('SYS-002: get_settings language stays as previously saved (not forced to system)', async () => {
    const forced = { ...original, language: 'fr' };
    await invokeBackend('save_settings', { settings: forced });
    await browser.pause(300);
    const loaded = await invokeBackend<AppSettings>('get_settings');
    expect(loaded.language).toBe('fr');

    // System language probe must not mutate settings
    await invokeBackend('get_system_ui_language');
    const again = await invokeBackend<AppSettings>('get_settings');
    expect(again.language).toBe('fr');
  });

  it('SYS-003: language persists across refresh', async () => {
    const next = { ...original, language: 'ja' };
    await invokeBackend('save_settings', { settings: next });
    await openMain();
    await browser.refresh();
    await openMain();
    const loaded = await invokeBackend<AppSettings>('get_settings');
    expect(loaded.language).toBe('ja');
    const ph = await searchPlaceholder();
    expect(ph.length).toBeGreaterThan(0);
  });

  it('SYS-004: unsupported language still shows English UI strings', async () => {
    await invokeBackend('save_settings', {
      settings: { ...original, language: 'xx-NOT-REAL' },
    });
    await openMain();
    await browser.refresh();
    await openMain();
    const ph = await searchPlaceholder();
    expect(ph).toContain('Find');
  });
});
