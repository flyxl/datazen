/**
 * First-frame i18n false-positive guard for the Data Transfer window.
 *
 * `t()` in packages/ui/src/i18n.ts reports a dev-only
 * `[i18n] Missing translation` line when it falls back to the raw key. That
 * report is only worth reading if it is *true*, so the window must not build
 * lazy-domain key constants before the `sync` locale pack is registered.
 *
 * The sibling suite (DataTransferWindow.test.tsx) cannot catch this: it mocks
 * `useI18n` with `t: (key) => key` and `useLocaleDomains` with `() => true`.
 * A `t` that echoes its argument never consults the registry, and a hook that
 * hard-codes `true` removes the "pack not loaded yet" state entirely — so
 * both the mechanism and the window it lives in are switched off.
 *
 * This suite therefore mocks NEITHER hook. It lets the real `useI18n` and the
 * real `useLocaleDomains` run, and only the eager domain packs are ever
 * registered (`src/locales/index.ts` pulls in `en/eager` + `zh-CN/eager`;
 * `ensureLocaleDomains` is what loads `sync`, and it resolves on a later tick).
 * That is the exact production first frame.
 */
import { afterEach, beforeEach, describe, expect, it, vi, type MockInstance } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock('../../../hooks/useSettings', () => ({
  useSettings: () => undefined,
}));

vi.mock('../../../stores/settingsStore', () => ({
  // No `settings.language` → the real useLocaleDomains defaults to 'en' and
  // asks for the `sync` pack, exactly as the shipping window does.
  useSettingsStore: (sel: (s: { loadSettings: () => void }) => unknown) =>
    sel({ loadSettings: vi.fn() }),
}));

vi.mock('../../../lib/windowKind', () => ({
  getUrlParam: () => null,
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: { getDatabases: vi.fn().mockResolvedValue([]) },
}));

vi.mock('../../../commands/transfer', () => ({
  DEFAULT_TRANSFER_OPTIONS: { batchSize: 500, stopOnError: true, confirmedDestructive: false },
  transferCommands: {
    getProfiles: vi.fn().mockResolvedValue([]),
    inspect: vi.fn().mockResolvedValue([]),
    preview: vi.fn(),
    execute: vi.fn(),
    cancel: vi.fn(),
    classifyPair: vi.fn(),
  },
}));

vi.mock('@datazen/extension-points', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/extension-points')>()),
  WindowChromeSlot: ({ title }: { title?: unknown }) => <div>{String(title ?? '')}</div>,
}));

vi.mock('../../../components/StatusBar', () => ({ StatusBar: () => <div /> }));

/** Every `[i18n] Missing translation` line seen since the spy was installed. */
function i18nWarnings(spy: MockInstance): string[] {
  return spy.mock.calls
    .map((call) => String(call[0]))
    .filter((line) => line.includes('[i18n] Missing translation'));
}

describe('DataTransferWindow — first frame emits no i18n false positives', () => {
  beforeEach(() => {
    // packages/ui/src/i18n.ts keeps `reportedMissingKeys` in a module-private
    // Set that is never cleared, so a shared module instance would let an
    // earlier test case report a key and make this one pass vacuously. A fresh
    // module graph restores an empty Set. React is externalised by Vite, so
    // resetting the registry does not split React identity.
    vi.resetModules();
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('reports a genuinely missing key (proves the DEV gate and the spy are live)', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { t } = await import('@datazen/ui');

    t('__firstFrameControl.dataTransfer.neverRegistered');

    expect(i18nWarnings(warn)).toHaveLength(1);
  });

  it('builds nothing lazy-domain-keyed before the sync pack is registered', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { DataTransferWindow } = await import('../DataTransferWindow');

    render(<DataTransferWindow />);

    // The placeholder is the direct proof that this is the pre-pack frame: if
    // it were absent, the assertion below would be trivially satisfied.
    expect(screen.getByTestId('data-transfer-locale-loading')).toBeTruthy();
    expect(i18nWarnings(warn)).toEqual([]);
  });

  it('emits no i18n warnings across the whole mount, pack load included', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { DataTransferWindow } = await import('../DataTransferWindow');

    render(<DataTransferWindow />);

    // Once the lazy pack lands the guarded body renders; a constant built
    // above the gate would have been evaluated on the frames before this.
    await waitFor(() => expect(screen.getByTestId('data-transfer-window')).toBeTruthy());
    expect(i18nWarnings(warn)).toEqual([]);
  });
});
