import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { SchemaDiffWindow } from '../SchemaDiffWindow';

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
  schemaDiffCommands: {
    compareTableSchemas: vi.fn(),
    preparePlan: vi.fn(),
    executeDeploy: vi.fn(),
  },
}));

describe('SchemaDiffWindow', () => {
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
