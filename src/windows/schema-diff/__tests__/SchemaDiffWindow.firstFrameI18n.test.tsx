/**
 * Pins a DEFENSIVE contract for the Schema Diff window — not a live bug.
 *
 * `endpointsCrossDialectNote` is a ternary that short-circuits, so it only
 * evaluates `t('schemaDiff.crossDialectNote')` once `endpoints.isCrossDialect`
 * turns true. The `localesReady` gate stops rendering, not evaluation, so if
 * that ever happened before the `sync` pack registered, the window would emit a
 * dev-only `[i18n] Missing translation` line for a key that is perfectly well
 * registered. Moving the constant below the gate closes that permanently.
 *
 * READ THIS BEFORE TRUSTING IT AS A GUARD FOR A LIVE BUG: today it cannot
 * happen. `isCrossDialect` only turns true once the user picks endpoints
 * (MigrationEndpointsBar's Select, or applyImportedConfig), and that is always
 * after the lazy pack has loaded. This suite gets to the frame anyway by mocking
 * `getUrlParam` to return sourceId/targetId, which is the one route that *could*
 * set the endpoints without interaction — so the test is pinning the guard
 * against that route and a changed first-frame, not recording a defect. That is
 * why it stays: if a caller ever does pass ids, the test is already there.
 *
 * SchemaDiffWindow.test.tsx cannot see this class of problem at all: it mocks
 * `useI18n` with `t: (key) => key` and `useLocaleDomains` with `() => true`. A
 * `t` that echoes its argument never consults the registry, and a hook that
 * hard-codes `true` removes the "pack not loaded yet" state entirely.
 *
 * This suite therefore mocks NEITHER i18n hook, and the only eager domain packs
 * are ever registered (`src/locales/index.ts` pulls in `en/eager` +
 * `zh-CN/eager`). One deliberate deviation from the other three suites in this
 * family: `lazyPacks.ensureLocaleDomains` is held open instead of resolving on
 * the next tick, because "isCrossDialect turns true before the pack lands" is
 * otherwise a sub-tick race rather than a reproducible state. Holding the pack
 * makes the pre-pack frame deterministic. `useI18n` and `useLocaleDomains` are
 * the real ones.
 */
import { afterEach, beforeEach, describe, expect, it, vi, type MockInstance } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import type { ConnectionConfig } from '../../../types';

const { invokeMock, urlParams } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  urlParams: {} as Record<string, string>,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...a: unknown[]) => invokeMock(...a),
}));

vi.mock('../../../hooks/useThemeListener', () => ({ useThemeListener: vi.fn() }));

// No `settings.language` → the real useLocaleDomains defaults to 'en'.
vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: { loadSettings: () => Promise<void> }) => unknown) =>
    sel({ loadSettings: vi.fn().mockResolvedValue(undefined) }),
}));

vi.mock('../../../hooks/useSettings', () => ({ useSettings: vi.fn() }));

vi.mock('../../../lib/windowManager', () => ({ openDocsWindow: vi.fn() }));

vi.mock('../../../lib/crossWindowBus', () => ({
  listenCrossWindow: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock('../../../lib/schemaDiffLimitationsPrefs', () => ({
  isSchemaDiffLimitationsDismissed: vi.fn().mockReturnValue(true),
  setSchemaDiffLimitationsDismissed: vi.fn(),
}));

vi.mock('../../../lib/dedicatedDbSession', () => ({
  listDatabasesDedicated: vi.fn().mockResolvedValue({ databases: [] }),
  ensureDedicatedSession: vi.fn().mockResolvedValue(null),
  releaseDedicatedSession: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../../commands/schemaDiff', () => ({
  dialectSupportsTransactionalDdl: vi.fn().mockReturnValue(true),
  exportPlanSql: vi.fn().mockReturnValue(''),
  planHasDestructive: vi.fn().mockReturnValue(false),
  subscribeSchemaDiffJobUpdates: vi.fn().mockReturnValue(() => {}),
  schemaDiffCommands: {
    compareTableSchemas: vi.fn(),
    preparePlan: vi.fn(),
    executeDeploy: vi.fn(),
    listJobs: vi.fn().mockResolvedValue([]),
    getJobDetails: vi.fn(),
    cancelDeploy: vi.fn().mockResolvedValue(true),
    verifyRecovery: vi.fn(),
  },
}));

// URL prefill. NO production caller emits these ids today (see the file
// header) — this mock is what makes the pre-pack `isCrossDialect` frame
// reproducible, and it is also the only route that can produce it.
vi.mock('../../../lib/windowKind', () => ({
  getUrlParam: (name: string) => urlParams[name] ?? null,
}));

// Keep the `sync` pack in flight for the whole test (see the file header).
vi.mock('../../../locales/lazyPacks', () => ({
  ensureLocaleDomains: () => new Promise<void>(() => {}),
  ensureAllLazyDomains: () => new Promise<void>(() => {}),
  isDomainLoaded: () => false,
}));

const pgSource: ConnectionConfig = {
  id: 'pg-1',
  name: 'PG Source',
  databaseType: 'postgresql',
  host: '127.0.0.1',
  port: 5432,
  database: 'app',
  username: 'postgres',
  password: '',
  sslMode: 'disable',
};
const myTarget: ConnectionConfig = {
  ...pgSource,
  id: 'my-1',
  name: 'MySQL Target',
  databaseType: 'mysql',
  port: 3306,
};

/** Every `[i18n] Missing translation` line seen since the spy was installed. */
function i18nWarnings(spy: MockInstance): string[] {
  return spy.mock.calls
    .map((call) => String(call[0]))
    .filter((line) => line.includes('[i18n] Missing translation'));
}

describe('SchemaDiffWindow — first frame emits no i18n false positives', () => {
  beforeEach(() => {
    // packages/ui/src/i18n.ts keeps `reportedMissingKeys` in a module-private
    // Set that is never cleared, so a shared module instance would let an
    // earlier test case report a key and make this one pass vacuously. A fresh
    // module graph restores an empty Set. React is externalised by Vite, so
    // resetting the registry does not split React identity.
    vi.resetModules();
    urlParams.sourceId = 'pg-1';
    urlParams.targetId = 'my-1';
    invokeMock.mockImplementation((cmd: string) =>
      Promise.resolve(cmd === 'get_connections' ? [pgSource, myTarget] : []),
    );
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('reports a genuinely missing key (proves the DEV gate and the spy are live)', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { t } = await import('@datazen/ui');

    t('__firstFrameControl.schemaDiff.neverRegistered');

    expect(i18nWarnings(warn)).toHaveLength(1);
  });

  it('emits no warning once a cross-dialect prefill lands on a pre-pack frame', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { SchemaDiffWindow } = await import('../SchemaDiffWindow');

    render(<SchemaDiffWindow />);

    // Still the pre-pack frame — the guard is holding the body back.
    expect(screen.getByTestId('schema-diff-locale-loading')).toBeTruthy();

    // Let `get_connections` resolve and `useMigrationEndpointPrefill` apply
    // the cross-dialect pair from the URL ids this suite injects above. If
    // the constant were built before the gate, the note would be evaluated
    // right here and emit a bogus line.
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await new Promise((resolve) => setTimeout(resolve, 0));
    await new Promise((resolve) => setTimeout(resolve, 0));

    // The guard is still up, so the body — and any constant built above it —
    // has not been rendered. The note must not have been evaluated either.
    expect(screen.getByTestId('schema-diff-locale-loading')).toBeTruthy();
    expect(i18nWarnings(warn)).toEqual([]);
  });
});
