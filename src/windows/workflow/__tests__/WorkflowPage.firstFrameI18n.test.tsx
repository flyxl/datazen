/**
 * First-frame i18n false-positive guard for the Workflow page.
 *
 * `t()` in packages/ui/src/i18n.ts reports a dev-only
 * `[i18n] Missing translation` line when it falls back to the raw key. That
 * report is only worth reading if it is *true*, so the page must not build
 * lazy-domain key constants before the `workflows` locale pack is registered.
 *
 * WorkflowPage.test.tsx cannot catch this: it mocks `useI18n` with
 * `t: (key) => key` and `useLocaleDomains` with `() => true`. A `t` that
 * echoes its argument never consults the registry, and a hook that hard-codes
 * `true` removes the "pack not loaded yet" state entirely — so both the
 * mechanism and the window it lives in are switched off.
 *
 * This suite therefore mocks NEITHER hook. It lets the real `useI18n` and the
 * real `useLocaleDomains` run, and only the eager domain packs are ever
 * registered (`src/locales/index.ts` pulls in `en/eager` + `zh-CN/eager`;
 * `ensureLocaleDomains` is what loads `workflows`, and it resolves on a later
 * tick). That is the exact production first frame.
 */
import { afterEach, beforeEach, describe, expect, it, vi, type MockInstance } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import type { WorkflowExecutionResult, WorkflowListItem } from '../../../types';

const { aiStoreState } = vi.hoisted(() => {
  const state = {
    workflows: [{ id: 'wf-1', name: 'Demo', description: '', variables: [] }] as WorkflowListItem[],
    workflowsLoading: false,
    loadWorkflows: vi.fn().mockResolvedValue(undefined),
    loadConfig: vi.fn().mockResolvedValue(undefined),
    executeWorkflow: vi.fn().mockResolvedValue(undefined),
    workflowError: null as string | null,
    clearWorkflowResult: vi.fn(),
    setupEventListeners: vi.fn().mockResolvedValue(() => {}),
    workflowExecutionResult: null as WorkflowExecutionResult | null,
  };
  return { aiStoreState: state };
});

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock('../../../hooks/useSettings', () => ({ useSettings: () => {} }));

vi.mock('../../../hooks/useResizable', () => ({
  useResizable: () => ({ size: 256, handleRef: { current: null } }),
}));

// No `settings.language` → the real useLocaleDomains defaults to 'en' and asks
// for the `workflows` pack, exactly as the shipping page does.
vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: unknown) => unknown) =>
    sel({ loadSettings: vi.fn(), updateSettings: vi.fn(), settings: {} }),
}));

vi.mock('../../../stores/aiStore', () => {
  const useAiStore = Object.assign(
    (sel: (s: typeof aiStoreState) => unknown) => sel(aiStoreState),
    { getState: () => aiStoreState },
  );
  return { useAiStore };
});

vi.mock('../../../commands/ai', () => ({
  aiCommands: {
    workflowGetDir: vi.fn().mockResolvedValue('/tmp/workflows'),
    workflowHistoryList: vi.fn().mockResolvedValue([]),
    workflowHistoryGet: vi.fn(),
    workflowHistoryClear: vi.fn().mockResolvedValue(undefined),
    workflowSave: vi.fn().mockResolvedValue(undefined),
    workflowDelete: vi.fn().mockResolvedValue(undefined),
    workflowReload: vi.fn().mockResolvedValue(undefined),
    workflowGet: vi.fn(),
  },
}));

vi.mock('../../../commands/connection', () => ({
  connectionCommands: { getConnections: vi.fn().mockResolvedValue([]) },
}));

vi.mock('../../../commands/settings', () => ({
  settingsCommands: { openWorkflowsDir: vi.fn().mockResolvedValue(undefined) },
}));

vi.mock('../../../lib/windowManager', () => ({ openDocsWindow: vi.fn() }));

vi.mock('@datazen/extension-points', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/extension-points')>()),
  WindowChromeSlot: ({ title }: { title?: unknown }) => <div>{String(title ?? '')}</div>,
}));

vi.mock('../../../components/StatusBar', () => ({ StatusBar: () => <div /> }));

vi.mock('../../../components/DataTable/DataTable', () => ({
  DataTable: () => <div data-testid="data-table" />,
}));

vi.mock('../../../components/chart/ChartView', () => ({
  ChartView: () => <div data-testid="chart-view" />,
}));

vi.mock('../../../components/ai/WorkflowChatPanel', () => ({
  WorkflowChatPanel: () => <div data-testid="wf-chat" />,
}));

/** Every `[i18n] Missing translation` line seen since the spy was installed. */
function i18nWarnings(spy: MockInstance): string[] {
  return spy.mock.calls
    .map((call) => String(call[0]))
    .filter((line) => line.includes('[i18n] Missing translation'));
}

describe('WorkflowPage — first frame emits no i18n false positives', () => {
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

    t('__firstFrameControl.workflow.neverRegistered');

    expect(i18nWarnings(warn)).toHaveLength(1);
  });

  it('builds nothing lazy-domain-keyed before the workflows pack is registered', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { WorkflowPage } = await import('../WorkflowPage');

    render(<WorkflowPage />);

    // The placeholder is the direct proof that this is the pre-pack frame: if
    // it were absent, the assertion below would be trivially satisfied.
    expect(screen.getByTestId('workflow-locale-loading')).toBeTruthy();
    expect(i18nWarnings(warn)).toEqual([]);
  });

  it('emits no i18n warnings across the whole mount, pack load included', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { WorkflowPage } = await import('../WorkflowPage');

    render(<WorkflowPage />);

    // Once the lazy pack lands the guarded body renders; a constant built
    // above the gate would have been evaluated on the frames before this.
    await waitFor(() => expect(screen.queryByTestId('workflow-locale-loading')).toBeNull());
    expect(i18nWarnings(warn)).toEqual([]);
  });
});
