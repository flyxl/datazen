import { describe, expect, it, vi, beforeAll, beforeEach, afterEach } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { SchemaDiffWindow } from '../SchemaDiffWindow';
// Fixture, not production wiring: this suite mocks the HOST i18n hooks (`useI18n`
// echoes keys so assertions can target key strings, `useLocaleDomains` returns
// true), so nothing in the graph ever registers the host dictionaries into the
// shared @datazen/ui registry. `@datazen/ui`'s own components (Dialog's close
// label, LimitationsDialog's bullets) still resolve through the real `t`, so
// they warn "Missing translation" for keys that are perfectly well registered.
// Importing the locales entry point registers the eager packs; the lazy `sync`
// pack is warmed explicitly (the same test-only route locales.test.ts uses).
import '../../../locales';
import { ensureAllLazyDomains } from '../../../locales/lazyPacks';

const { stableT } = vi.hoisted(() => ({
  stableT: (key: string) => key,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

vi.mock('../../../hooks/useThemeListener', () => ({
  useThemeListener: vi.fn(),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: stableT, language: 'en' }),
}));

vi.mock('../../../hooks/useLocaleDomains', () => ({
  useLocaleDomains: () => true,
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: { loadSettings: () => Promise<void> }) => unknown) =>
    sel({ loadSettings: vi.fn().mockResolvedValue(undefined) }),
}));

vi.mock('../../../hooks/useSettings', () => ({
  useSettings: vi.fn(),
}));

vi.mock('../../../lib/windowManager', () => ({
  openDocsWindow: vi.fn(),
}));

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
    getProfiles: vi.fn().mockResolvedValue([]),
    saveProfile: vi.fn().mockResolvedValue(undefined),
    deleteProfile: vi.fn().mockResolvedValue(undefined),
    compareTableSchemas: vi.fn(),
    preparePlan: vi.fn(),
    executeDeploy: vi.fn(),
    listJobs: vi.fn().mockResolvedValue([]),
    getJobDetails: vi.fn(),
    cancelDeploy: vi.fn().mockResolvedValue(true),
    verifyRecovery: vi.fn(),
  },
}));

describe('SchemaDiffWindow', () => {
  beforeAll(async () => {
    // Eager packs land at import above; the lazy `sync` pack needs an explicit
    // await because the mocked `useLocaleDomains` never requests it.
    await ensureAllLazyDomains('en');
  });

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it('renders wizard shell with endpoints step and navigation', () => {
    render(<SchemaDiffWindow />);

    expect(screen.getByTestId('schema-diff-window')).toBeInTheDocument();
    expect(screen.getByTestId('schema-diff-source')).toBeInTheDocument();
    expect(screen.getByTestId('schema-diff-target')).toBeInTheDocument();
    expect(screen.getByTestId('schema-diff-step-endpoints')).toBeInTheDocument();
    expect(screen.getByTestId('schema-diff-step-objects')).toBeInTheDocument();
    expect(screen.getByTestId('schema-diff-next')).toBeInTheDocument();
  });
});
