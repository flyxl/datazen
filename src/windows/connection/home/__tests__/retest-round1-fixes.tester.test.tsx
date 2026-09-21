/**
 * [tester] Retest-round-1 supplemental tests (track: landing-page-opt).
 *
 * Covers the code paths introduced by the Coder fix commit ae40645b that the
 * round-1 suites did not exercise:
 * - ConnectionCardList row keyboard activation (the new onKeyDown Enter/Space
 *   branch on the div[role=button] row), including the no-double-trigger
 *   canary for the nested real Connect button and the preserved
 *   stopPropagation semantics on its click.
 * - RecentQueriesList useRelativeTimeLabel branches: just-now, hour and day
 *   buckets, the >7d locale-date fallback, and invalid-timestamp omission.
 * - BUG-001 substance: real zh-CN rendering through the real
 *   settingsStore -> localeSync -> @datazen/ui engine -> useI18n chain (no
 *   hook mocks) asserting 「刚刚 / N 分钟前 / N 小时前 / N 天前」, the en
 *   counterparts, and key parity in both locale packs.
 */
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { afterEach, beforeEach, afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { ConnectionCardList } from '../ConnectionCardList';
import { RecentQueriesList } from '../RecentQueriesList';
import { getTranslation } from '../../../../locales';
import { getRelativeTimeParts } from '../../../../lib/relativeTime';
import { useSettingsStore } from '../../../../stores/settingsStore';
import { startLocaleSync } from '../../../../lib/localeSync';
import type { ConnectionConfig, QueryHistoryEntry } from '../../../../types';

vi.mock('../../../../lib/databaseTypes', () => ({
  getDbIcon: () => ({ label: 'Db', bg: 'bg-blue-500' }),
  getDbLabel: (type: string) => type,
  getDriverIconMap: () => ({}),
  getDriverIconParents: () => ({}),
}));

// Passthrough spy on the lib collaborator so the defensively-unreachable
// `!parts` branch of useRelativeTimeLabel can be exercised without changing
// behavior for any other test in this file.
vi.mock('../../../../lib/relativeTime', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../../lib/relativeTime')>();
  return { ...actual, getRelativeTimeParts: vi.fn(actual.getRelativeTimeParts) };
});

afterEach(cleanup);

function setLanguage(language: string): void {
  useSettingsStore.setState({
    settings: { ...useSettingsStore.getState().settings, language },
  });
}

// Real host wiring (main.tsx): settingsStore.language → setLocale(shared engine).
let stopLocaleSync: (() => void) | undefined;

beforeAll(() => {
  stopLocaleSync = startLocaleSync();
});

afterAll(() => {
  stopLocaleSync?.();
});

beforeEach(() => {
  setLanguage('en');
});

const conn: ConnectionConfig = {
  id: 'conn-1',
  name: 'PostgreSQL-Local',
  databaseType: 'postgresql',
  host: 'localhost',
  port: 5432,
  database: 'postgres',
  group: 'Development',
};

function historyEntry(offsetMs: number, id: string): QueryHistoryEntry {
  return {
    id,
    connectionId: 'conn-1',
    database: 'postgres',
    sql: 'SELECT 1;',
    executedAt: new Date(Date.now() - offsetMs).toISOString(),
    executionTimeMs: 12,
    success: true,
  };
}

describe('[tester][retest-1] ConnectionCardList row keyboard activation (BUG-002 regression guard)', () => {
  const onConnect = vi.fn();
  const onNewConnection = vi.fn();

  function renderList() {
    render(
      <ConnectionCardList
        connections={[conn]}
        activeConnections={{}}
        onConnect={onConnect}
        onNewConnection={onNewConnection}
      />,
    );
  }

  beforeEach(() => {
    onConnect.mockClear();
    onNewConnection.mockClear();
  });

  it('[tester] connects when Enter is pressed on the focused row', () => {
    renderList();
    fireEvent.keyDown(screen.getByTestId('home-conn-card-conn-1'), { key: 'Enter' });
    expect(onConnect).toHaveBeenCalledTimes(1);
    expect(onConnect).toHaveBeenCalledWith('conn-1');
  });

  it('[tester] connects when Space is pressed on the focused row', () => {
    renderList();
    fireEvent.keyDown(screen.getByTestId('home-conn-card-conn-1'), { key: ' ' });
    expect(onConnect).toHaveBeenCalledTimes(1);
  });

  it('[tester] ignores non-activation keys on the row', () => {
    renderList();
    fireEvent.keyDown(screen.getByTestId('home-conn-card-conn-1'), { key: 'Tab' });
    fireEvent.keyDown(screen.getByTestId('home-conn-card-conn-1'), { key: 'a' });
    expect(onConnect).not.toHaveBeenCalled();
  });

  it('[tester] fires connect exactly once when Enter is pressed on the nested Connect button (no double trigger)', () => {
    renderList();
    // Keydown bubbles from the real <Button> to the row's onKeyDown; the row
    // handler's preventDefault must yield exactly one activation.
    fireEvent.keyDown(screen.getByTestId('home-conn-connect-conn-1'), { key: 'Enter' });
    expect(onConnect).toHaveBeenCalledTimes(1);
    expect(onConnect).toHaveBeenCalledWith('conn-1');
  });

  it('[tester] keeps inner-button clicks single-fire via stopPropagation', () => {
    renderList();
    fireEvent.click(screen.getByTestId('home-conn-connect-conn-1'));
    expect(onConnect).toHaveBeenCalledTimes(1);
  });
});

describe('[tester][retest-1] RecentQueriesList localized relative-time labels (BUG-001 substance)', () => {
  const onSelectHistoryQuery = vi.fn();
  const onOpenHistory = vi.fn();

  beforeEach(() => {
    onSelectHistoryQuery.mockClear();
    onOpenHistory.mockClear();
  });

  it('[tester] renders zh-CN labels through the real i18n chain', () => {
    setLanguage('zh-CN');
    render(
      <RecentQueriesList
        entries={[
          historyEntry(10_000, 'h-now'),
          historyEntry(2 * 60_000, 'h-min'),
          historyEntry(3 * 3_600_000, 'h-hour'),
          historyEntry(2 * 86_400_000, 'h-day'),
        ]}
        savedConnections={[conn]}
        onSelectHistoryQuery={onSelectHistoryQuery}
        onOpenHistory={onOpenHistory}
      />,
    );
    expect(screen.getByText('刚刚')).toBeInTheDocument();
    expect(screen.getByText('2 分钟前')).toBeInTheDocument();
    expect(screen.getByText('3 小时前')).toBeInTheDocument();
    expect(screen.getByText('2 天前')).toBeInTheDocument();
    // No English leakage in the relative-time segments.
    expect(screen.queryByText('just now')).not.toBeInTheDocument();
    expect(screen.queryByText(/min ago/)).not.toBeInTheDocument();
  });

  it('[tester] renders en labels through the real i18n chain', () => {
    setLanguage('en');
    render(
      <RecentQueriesList
        entries={[
          historyEntry(10_000, 'e-now'),
          historyEntry(2 * 60_000, 'e-min'),
          historyEntry(3 * 3_600_000, 'e-hour'),
          historyEntry(2 * 86_400_000, 'e-day'),
        ]}
        savedConnections={[conn]}
        onSelectHistoryQuery={onSelectHistoryQuery}
        onOpenHistory={onOpenHistory}
      />,
    );
    expect(screen.getByText('just now')).toBeInTheDocument();
    expect(screen.getByText('2 min ago')).toBeInTheDocument();
    expect(screen.getByText('3 hr ago')).toBeInTheDocument();
    expect(screen.getByText('2 d ago')).toBeInTheDocument();
    expect(screen.queryByText('刚刚')).not.toBeInTheDocument();
  });

  it('[tester] falls back to a locale date beyond the 7-day window without i18n key leakage', () => {
    render(
      <RecentQueriesList
        entries={[historyEntry(8 * 86_400_000, 'h-old')]}
        savedConnections={[conn]}
        onSelectHistoryQuery={onSelectHistoryQuery}
        onOpenHistory={onOpenHistory}
      />,
    );
    // Short locale date (contains a numeric year), not a relative label.
    const dateSegment = screen.getByText(/\d{4}/);
    expect(dateSegment.textContent).not.toContain('connWin.');
    expect(screen.queryByText(/min ago|分钟前/)).not.toBeInTheDocument();
  });

  it('[tester] omits the relative segment for invalid timestamps', () => {
    render(
      <RecentQueriesList
        entries={[
          {
            id: 'h-bad',
            connectionId: 'conn-1',
            database: 'postgres',
            sql: 'SELECT 1;',
            executedAt: 'not-a-date',
            executionTimeMs: 5,
            success: true,
          },
        ]}
        savedConnections={[conn]}
        onSelectHistoryQuery={onSelectHistoryQuery}
        onOpenHistory={onOpenHistory}
      />,
    );
    expect(screen.getByTestId('home-query-source-h-bad')).toBeInTheDocument();
    expect(screen.queryByText('just now')).not.toBeInTheDocument();
    expect(screen.queryByText(/min ago/)).not.toBeInTheDocument();
  });

  it('[tester] omits the segment without crashing when getRelativeTimeParts returns null', () => {
    // Defensive `!parts` branch: the lib itself never returns null for a
    // timestamp that passed the component's own guard, so force it via the
    // passthrough spy to lock in the graceful-degradation contract.
    vi.mocked(getRelativeTimeParts).mockReturnValueOnce(null);
    render(
      <RecentQueriesList
        entries={[historyEntry(2 * 60_000, 'h-null')]}
        savedConnections={[conn]}
        onSelectHistoryQuery={onSelectHistoryQuery}
        onOpenHistory={onOpenHistory}
      />,
    );
    expect(screen.getByTestId('home-query-source-h-null')).toBeInTheDocument();
    expect(screen.queryByText(/min ago/)).not.toBeInTheDocument();
    expect(screen.queryByText('just now')).not.toBeInTheDocument();
    vi.mocked(getRelativeTimeParts).mockRestore();
  });
});

describe('[tester][retest-1] relative-time i18n key parity via getTranslation (BUG-001)', () => {
  it('[tester] exposes the four keys with zh-CN copy and {count} interpolation', () => {
    expect(getTranslation('zh-CN', 'connWin.home.queries.justNow')).toBe('刚刚');
    expect(getTranslation('zh-CN', 'connWin.home.queries.minutesAgo', { count: 5 })).toBe(
      '5 分钟前',
    );
    expect(getTranslation('zh-CN', 'connWin.home.queries.hoursAgo', { count: 5 })).toBe('5 小时前');
    expect(getTranslation('zh-CN', 'connWin.home.queries.daysAgo', { count: 5 })).toBe('5 天前');
  });

  it('[tester] exposes the four keys with en copy and {count} interpolation', () => {
    expect(getTranslation('en', 'connWin.home.queries.justNow')).toBe('just now');
    expect(getTranslation('en', 'connWin.home.queries.minutesAgo', { count: 5 })).toBe('5 min ago');
    expect(getTranslation('en', 'connWin.home.queries.hoursAgo', { count: 5 })).toBe('5 hr ago');
    expect(getTranslation('en', 'connWin.home.queries.daysAgo', { count: 5 })).toBe('5 d ago');
  });
});
