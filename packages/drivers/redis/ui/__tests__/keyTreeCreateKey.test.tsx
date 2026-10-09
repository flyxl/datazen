/**
 * Create key, after the right panel's copy of the button was removed.
 *
 * The action itself is not what moved: the workbench still owns the create-key
 * overlay and still issues `set_string`. What changed is that the button now
 * lives in exactly one place — the key tree's R1 toolbar row — and the right
 * panel's tab bar carries no action controls at all.
 *
 * So the coverage follows the *function*, not the button:
 *
 * 1. The right panel's tab bar stays free of action buttons. It once carried a
 *    Create key copy, and before that the host's 48px KV context band carried
 *    refresh / import / export / overflow. Both were removed on purpose; a
 *    button quietly reappearing in the tab bar would undo that.
 * 2. The surviving button is the key tree's, and it is wired to the workbench's
 *    own overlay — clicking it opens the real dialog and submitting reaches
 *    `set_string`. A button that renders but opens nothing would look like the
 *    feature survived when it did not.
 *
 * Rendered through `RedisConnectionView` because that is the only path where the
 * workbench mounts; the dialog overlay is reached through the same hook
 * (`overlays.setCreateOpen`) from both sides of the split.
 *
 * 只断言 `data-testid` / `data-*`（PRD §7-6：禁英文字面量）; the i18n `t()` is
 * mocked to echo the key.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
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
  useI18n: () => ({
    t: (key: string) => key,
  }),
}));

const execute = vi.fn();
vi.mock('../shared/redisInvoke', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../shared/redisInvoke')>()),
  redisCommandInvoke: (...args: unknown[]) => execute(...(args as [])),
  // The key tree scans and the detail column reads on mount, none of which this
  // suite asserts. Stubbed to empty replies so the run is not buried in
  // unhandled-IPC stderr from a Tauri bridge jsdom does not have.
  invokeScanKeys: vi.fn(async () => ({ cursor: 0, keys: [], done: true })),
  invokeListChildren: vi.fn(async () => ({ children: [], cursor: 0 })),
  invokeGetKey: vi.fn(async () => null),
  invokeGetKeyRaw: vi.fn(async () => null),
  invokeDbSizes: vi.fn(async () => []),
}));

// The console / pubsub / slowlog bodies open their own heavy panels; the tab
// bar is what this suite is about.
vi.mock('../console/RedisConsole', () => ({
  RedisConsole: () => <div data-testid="stub-console" />,
}));
vi.mock('../observe/SlowlogPanel', () => ({
  SlowlogPanel: () => <div data-testid="stub-slowlog" />,
}));
vi.mock('../observe/PubSubPanel', () => ({
  PubSubPanel: () => <div data-testid="stub-pubsub" />,
}));

import { RedisConnectionView } from '../connection/RedisConnectionView';
import { panelCloseStub } from '../__testing__/panelClose';

// The view registers a close handler on mount; cases fire it explicitly.
const panelClose = panelCloseStub();

// The workbench reads the same host bridges a real panel would; jsdom has no
// host, so the minimal stand-ins are bound here. Safe Mode off keeps the guard
// out of the way — it is not what this suite is about.
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
      ['sess-create', { pathItems: {}, databases: ['db0', 'db1'], loading: false }],
    ]),
    loadForConnection: async () => {},
    setLoadedTables: () => {},
    mergeNamespace: () => {},
    registerPathAliases: () => {},
    cachePathItems: () => {},
  })),
);

afterEach(() => {
  cleanup();
  execute.mockReset();
});

function renderView() {
  return render(
    <RedisConnectionView
      panelId="panel-test"
      onPanelClosed={panelClose.onPanelClosed}
      dbSessionId="sess-create"
      connectionId="cfg-create"
      connectionName="local"
      databaseType="redis"
      initialDatabase="db0"
      hideSidebar
      isActive
    />,
  );
}

/** A dialog whose rendered title is this i18n key (identity `t` ⇒ title === key). */
async function dialogByTitle(titleKey: string): Promise<HTMLElement> {
  return waitFor(() => {
    const found = Array.from(document.querySelectorAll('[role="dialog"]')).find(
      (el) => el.textContent?.includes(titleKey) ?? false,
    );
    if (!found) throw new Error(`no dialog for ${titleKey}`);
    return found as HTMLElement;
  });
}

function buttonWithKey(container: HTMLElement, key: string): HTMLButtonElement {
  const found = Array.from(container.querySelectorAll('button')).find((b) => b.textContent === key);
  if (!found) throw new Error(`no button ${key}`);
  return found as HTMLButtonElement;
}

describe('Create key — one button, in the key tree', () => {
  it('is the only create-key affordance on screen', async () => {
    renderView();
    await act(async () => {});

    // A second entry point would mean two different-looking ways to do the same
    // thing in two columns; the user asked for one.
    expect(screen.getAllByTestId('redis-tree-create-key')).toHaveLength(1);
    // It belongs to the key column's own toolbar row, not to a host band.
    expect(
      screen
        .getByTestId('redis-tree-toolbar-row')
        .contains(screen.getByTestId('redis-tree-create-key')),
    ).toBe(true);
  });

  it('leaves the right panel tab bar with no action controls', async () => {
    renderView();
    await act(async () => {});

    // Every `redis-right-*` button in the bar is a tab; the tabs carry
    // `data-active`, so anything left is an action control. The removed ones
    // (Create key, and earlier the band's refresh / import / export / overflow)
    // must not creep back into a row the user asked to be tabs only.
    const bar = screen.getByTestId('redis-right-tab-bar');
    const actions = Array.from(bar.querySelectorAll<HTMLElement>('button')).filter(
      (el) => !el.hasAttribute('data-active'),
    );
    expect(actions).toEqual([]);
  });

  it('opens the workbench create dialog and reaches set_string on submit', async () => {
    renderView();
    await act(async () => {});

    // Enter condition: no dialog before the click.
    expect(screen.queryByRole('dialog')).toBeNull();

    await act(async () => {
      fireEvent.click(screen.getByTestId('redis-tree-create-key'));
    });

    // In-state: the workbench's own overlay opened the real dialog — proof the
    // button still reaches the state machine rather than only rendering.
    const dialog = await dialogByTitle('redis.createKey');
    const nameField = dialog.querySelector('input[placeholder="redis.keyName"]');
    expect(nameField).toBeTruthy();
    fireEvent.change(nameField as HTMLInputElement, { target: { value: 'new:key' } });
    fireEvent.click(buttonWithKey(dialog, 'redis.create'));

    // Exit transition: the command the create-key form always issued.
    await waitFor(() =>
      expect(execute).toHaveBeenCalledWith(
        'redis',
        'set_string',
        expect.objectContaining({ key: 'new:key' }),
      ),
    );
  });
});
