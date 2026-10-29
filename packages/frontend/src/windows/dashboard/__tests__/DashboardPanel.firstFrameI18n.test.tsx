/**
 * First-frame i18n false-positive guard for the Dashboard panel.
 *
 * `t()` in packages/ui/src/i18n.ts reports a dev-only
 * `[i18n] Missing translation` line when it falls back to the raw key. That
 * report is only worth reading if it is *true*, so the panel must not build
 * lazy-domain key constants before the `dashboard` locale pack is registered.
 *
 * No existing suite can see this. ConnectionPage.test.tsx replaces
 * DashboardPanel wholesale with a stub, and the panel's own suites drive it
 * with `useLocaleDomains` mocked to `true` — so the "pack not loaded yet"
 * state, the one that produces the false positive, never exists in the suite
 * at all.
 *
 * This suite therefore mocks NEITHER i18n hook. It lets the real `useI18n` and
 * the real `useLocaleDomains` run, and only the eager domain packs are ever
 * registered (`src/locales/index.ts` pulls in `en/eager` + `zh-CN/eager`;
 * `ensureLocaleDomains` is what loads `dashboard`, and it resolves on a later
 * tick). That is the exact production first frame.
 *
 * The store is seeded *hot* on purpose: the leaking constant is guarded by
 * `{current && (...)}`, so a cold store short-circuits and the defect stays
 * hidden. Reopening a dashboard that is already in the store is the case the
 * first-round probe reproduced, and the only one that exercises the branch.
 */
import { afterEach, beforeEach, describe, expect, it, vi, type MockInstance } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import type { Dashboard, DashboardWidget } from '../../../types/dashboard';

const { dashboardStoreState } = vi.hoisted(() => {
  const widget: DashboardWidget = {
    id: 'w1',
    title: 'W',
    workflowId: 'wf-1',
    viewMode: 'table',
    layout: { x: 0, y: 0, w: 1, h: 1 },
    refresh: { mode: 'manual' },
    enabled: true,
  };
  const dashboard: Dashboard = {
    id: 'd1',
    name: 'Ops',
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    layout: { cols: 12, rowHeight: 40 },
    widgets: [widget],
    enabled: true,
  };
  return {
    dashboardStoreState: {
      dashboardsById: {
        d1: { dashboard, runs: {}, busyWidgets: {}, loading: false, error: null, refCount: 1 },
      },
      list: [dashboard],
      listError: null,
      listLoading: false,
      fetchDashboards: vi.fn().mockResolvedValue(undefined),
      mountDashboard: vi.fn(),
      releaseDashboard: vi.fn(),
      loadDashboard: vi.fn().mockResolvedValue(undefined),
      saveDashboard: vi.fn().mockResolvedValue(dashboard),
      deleteDashboard: vi.fn().mockResolvedValue(undefined),
      refreshWidget: vi.fn().mockResolvedValue({}),
      refreshAllWidgets: vi.fn().mockResolvedValue(undefined),
    },
  };
});

// No `settings.language` → the real useLocaleDomains defaults to 'en' and asks
// for the `dashboard` pack, exactly as the shipping panel does.
vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: unknown) => unknown) =>
    sel({ loadSettings: vi.fn(), updateSettings: vi.fn(), settings: {} }),
}));

vi.mock('../../../stores/dashboardStore', () => {
  const useDashboardStore = Object.assign(
    (sel: (s: typeof dashboardStoreState) => unknown) => sel(dashboardStoreState),
    { getState: () => dashboardStoreState },
  );
  return { useDashboardStore };
});

vi.mock('../../../stores/workspacePanelStateStore', () => {
  const snapshot = { activeDashboardId: 'd1', userWorkflows: [] };
  const useWorkspacePanelStateStore = Object.assign(
    (sel: (s: { dashboard: typeof snapshot }) => unknown) => sel({ dashboard: snapshot }),
    {
      getState: () => ({ saveDashboardSnapshot: vi.fn(), dashboard: snapshot }),
    },
  );
  return { useWorkspacePanelStateStore };
});

vi.mock('../../../commands/dashboard', () => ({
  dashboardCommands: {
    setDashboardRefreshPaused: vi.fn().mockResolvedValue(undefined),
    updateDashboard: vi.fn().mockResolvedValue({}),
  },
}));

vi.mock('../../../commands/ai', () => ({
  aiCommands: { workflowList: vi.fn().mockResolvedValue([]) },
}));

vi.mock('../../../lib/windowManager', () => ({
  openDocsWindow: vi.fn(),
  openWorkflowWindow: vi.fn(),
}));

vi.mock('../ChartWidgetTile', () => ({
  ChartWidgetTile: () => <div data-testid="chart-widget-tile" />,
}));

vi.mock('../RunHistoryDrawer', () => ({ RunHistoryDrawer: () => null }));

vi.mock('../WidgetEditorDrawer', () => ({ WidgetEditorDrawer: () => null }));

/** Every `[i18n] Missing translation` line seen since the spy was installed. */
function i18nWarnings(spy: MockInstance): string[] {
  return spy.mock.calls
    .map((call) => String(call[0]))
    .filter((line) => line.includes('[i18n] Missing translation'));
}

describe('DashboardPanel — first frame emits no i18n false positives', () => {
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

    t('__firstFrameControl.dashboard.neverRegistered');

    expect(i18nWarnings(warn)).toHaveLength(1);
  });

  it('builds nothing lazy-domain-keyed before the dashboard pack is registered', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { DashboardPanel } = await import('../DashboardPanel');

    render(<DashboardPanel initialDashboardId="d1" />);

    // The placeholder is the direct proof that this is the pre-pack frame: if
    // it were absent, the assertion below would be trivially satisfied.
    expect(screen.getByTestId('dashboard-locale-loading')).toBeTruthy();
    expect(i18nWarnings(warn)).toEqual([]);
  });

  it('emits no i18n warnings across the whole mount, pack load included', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { DashboardPanel } = await import('../DashboardPanel');

    render(<DashboardPanel initialDashboardId="d1" />);

    // Once the lazy pack lands the guarded body renders with the hot store,
    // which is the frame the `{current && …}` branch actually needs.
    await waitFor(() => expect(screen.getByTestId('dashboard-main')).toBeTruthy());
    expect(i18nWarnings(warn)).toEqual([]);
  });
});
