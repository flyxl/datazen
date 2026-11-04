/**
 * The key tree's R1 toolbar row and the right panel's tab bar are the same
 * height.
 *
 * These are two separate column headers sitting side by side in 屏 B. Nothing in
 * the layout forces them to agree: the tree row is sized by its tallest control
 * plus the padding on its container, the tab bar by its buttons' `py-*`. The two
 * formulas drifted, and the columns' first rows stopped lining up — a 1px-style
 * detail that is invisible in code review and immediately obvious on screen.
 *
 * A shared constant cannot fix it: Tailwind only extracts class names it can see
 * as literals in a scanned file, so a `COLUMN_HEADER_H` exported from a module
 * would silently stop styling anything. Both sides therefore name `h-10`
 * literally, and *this file* is the thing that keeps the two literals in step —
 * change one height and this test is what fails.
 *
 * jsdom does no layout, so the assertion is on the class contract, not on
 * `getBoundingClientRect`. That is the honest seam: what is pinned is "both
 * authors declared the same height", which is the part a future edit can break.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, render, screen } from '@testing-library/react';
import { create } from 'zustand';
import {
  bindConfirmDialog,
  bindConnectionStore,
  bindSchemaStore,
  bindSettingsStore,
  type ConnectionBridgeState,
  type SchemaStoreState,
  type SettingsBridgeState,
} from '@datazen/driver-sdk';

vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../shared/redisInvoke', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../shared/redisInvoke')>()),
  redisCommandInvoke: vi.fn(async () => undefined),
  invokeScanKeys: vi.fn(async () => ({ cursor: 0, keys: [], done: true })),
  invokeListChildren: vi.fn(async () => ({ children: [], cursor: 0 })),
  invokeGetKey: vi.fn(async () => null),
  invokeGetKeyRaw: vi.fn(async () => null),
  invokeDbSizes: vi.fn(async () => []),
}));

vi.mock('../console/RedisConsole', () => ({ RedisConsole: () => <div /> }));
vi.mock('../observe/SlowlogPanel', () => ({ SlowlogPanel: () => <div /> }));
vi.mock('../observe/PubSubPanel', () => ({ PubSubPanel: () => <div /> }));

import { RedisConnectionView } from '../connection/RedisConnectionView';
import { panelCloseStub } from '../__testing__/panelClose';

// The view registers a close handler on mount; cases fire it explicitly.
const panelClose = panelCloseStub();

bindSettingsStore(
  create<SettingsBridgeState>(() => ({
    settings: { safeMode: false, editorFontFamily: '', driverSettings: {} },
  })),
);
bindConnectionStore(create<ConnectionBridgeState>(() => ({ connections: [] })));
bindConfirmDialog(() => [async () => true, null]);
bindSchemaStore(
  create<SchemaStoreState>(() => ({
    schemas: new Map([
      ['sess-height', { pathItems: {}, databases: ['db0', 'db1'], loading: false }],
    ]),
    loadForConnection: async () => {},
    setLoadedTables: () => {},
    mergeNamespace: () => {},
    registerPathAliases: () => {},
    cachePathItems: () => {},
  })),
);

afterEach(cleanup);

/** The Tailwind height token that decides the row's rendered height, if any. */
function heightToken(el: HTMLElement): string | undefined {
  return el.className.split(/\s+/).find((c) => /^h-/.test(c));
}

describe('column header rows', () => {
  it('the key tree toolbar row and the right panel tab bar declare the same height', async () => {
    render(
      <RedisConnectionView
        panelId="panel-test"
        onPanelClosed={panelClose.onPanelClosed}
        dbSessionId="sess-height"
        connectionId="cfg-height"
        connectionName="local"
        databaseType="redis"
        initialDatabase="db0"
        hideSidebar
        isActive
      />,
    );
    await act(async () => {});

    const treeRow = screen.getByTestId('redis-tree-toolbar-row');
    const tabBar = screen.getByTestId('redis-right-tab-bar');

    // Both pinned explicitly. A row left to be sized by its content is exactly
    // how the two drifted apart the first time.
    expect(heightToken(treeRow)).toBe('h-10');
    expect(heightToken(tabBar)).toBe('h-10');
  });

  it('neither row adds vertical padding on top of its own height', async () => {
    render(
      <RedisConnectionView
        panelId="panel-test"
        onPanelClosed={panelClose.onPanelClosed}
        dbSessionId="sess-height"
        connectionId="cfg-height"
        connectionName="local"
        databaseType="redis"
        initialDatabase="db0"
        hideSidebar
        isActive
      />,
    );
    await act(async () => {});

    // `py-*` on either row would push the rendered box past `h-10` (or leave the
    // height being decided by content, defeating the pin). `px-*` is fine — it
    // is horizontal, and the tab bar is meant to breathe sideways.
    for (const testId of ['redis-tree-toolbar-row', 'redis-right-tab-bar']) {
      const classes = screen.getByTestId(testId).className.split(/\s+/);
      expect(classes.filter((c) => /^py-/.test(c))).toEqual([]);
    }
  });
});
