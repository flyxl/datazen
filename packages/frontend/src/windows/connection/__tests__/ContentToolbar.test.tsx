import { render, screen, cleanup, act, fireEvent } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useSyncExternalStore, type ReactNode } from 'react';
import type { KvContextBarProps, KvSlotState } from '@datazen/driver-sdk';
import { ContentToolbar } from '../ContentToolbar';
import { createKvSlotState } from '../../../lib/kvSlotState';
import type { KvContextBarBinding } from '../useKvWorkspaceSlots';

// useCompactToolbar observes the toolbar width; jsdom has no ResizeObserver.
class MockResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}
// eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
globalThis.ResizeObserver ??= MockResizeObserver as unknown as typeof ResizeObserver;

vi.mock('../../../hooks/useI18n', () => ({
  // Key-style mock: assertions target i18n keys, never visible English copy.
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../lib/windowManager', () => ({
  openDocsWindow: vi.fn(),
}));

vi.mock('../../../components/DataTable/DetailPanelToggle', () => ({
  DetailPanelToggle: ({ children }: { children?: ReactNode }) => (
    <div data-testid="mock-detail-panel-toggle">{children}</div>
  ),
}));

/**
 * Fixture standing in for a driver's KV context bar: it renders the props the host
 * hands over as data attributes, subscribes to the shared selection atom and calls
 * `request` for a host action, which is exactly the contract the host implements
 * against.
 */
function FixtureContextBar({
  connectionId,
  dbSessionId,
  connectionName,
  databaseType,
  database,
  dbIndex,
  compact,
  state,
  request,
}: KvContextBarProps) {
  const selectedKey = useSyncExternalStore(state.subscribe, state.getSelectedKey);
  return (
    <div
      data-testid="fixture-context-bar"
      data-connection-id={connectionId}
      data-db-session-id={dbSessionId}
      data-connection-name={connectionName}
      data-database-type={databaseType}
      data-database={database ?? ''}
      data-db-index={dbIndex ?? ''}
      data-compact={String(compact)}
      data-selected-key={selectedKey ?? ''}
      data-request={typeof request === 'function' ? 'wired' : 'missing'}
    >
      <button
        type="button"
        data-testid="fixture-request-flush"
        onClick={() => request({ type: 'flushDb' })}
      >
        flush
      </button>
    </div>
  );
}

function binding(
  state: KvContextBarProps['state'],
  request: KvContextBarProps['request'] = vi.fn(),
): KvContextBarBinding {
  return {
    Component: FixtureContextBar,
    props: {
      connectionId: 'cfg-1',
      dbSessionId: 'sess-1',
      connectionName: 'KV Local',
      databaseType: 'redis' as never,
      database: 'db7',
      dbIndex: 7,
      state,
      request,
    },
  };
}

/**
 * The full prop set as a *value*, so a journey test can rerender the very same
 * toolbar while the active panel changes underneath it (keep-alive tab switch)
 * instead of only ever testing fresh mounts.
 */
function toolbarProps(
  contextBarSlot?: KvContextBarBinding,
  kvPanelState?: KvSlotState,
): Parameters<typeof ContentToolbar>[0] {
  return {
    showNewQuery: false,
    showNewTable: false,
    showErDiagram: false,
    showObjects: false,
    showBatchExport: false,
    aiChatOpen: false,
    detailPanelApplicable: false,
    detailOpen: false,
    contextBarSlot,
    kvPanelState,
    onNewQuery: vi.fn(),
    onCreateTable: vi.fn(),
    onOpenErDiagram: vi.fn(),
    onOpenObjects: vi.fn(),
    onOpenPrivileges: vi.fn(),
    onBatchExport: vi.fn(),
    onToggleAiChat: vi.fn(),
    onToggleDetail: vi.fn(),
  };
}

function renderToolbar(contextBarSlot?: KvContextBarBinding, kvPanelState?: KvSlotState) {
  return render(<ContentToolbar {...toolbarProps(contextBarSlot, kvPanelState)} />);
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('ContentToolbar KV context bar slot', () => {
  it('keeps the plain spacer when no context bar was contributed', () => {
    const { container } = renderToolbar();
    const toolbar = container.firstElementChild as HTMLElement;

    expect(screen.queryByTestId('conn-toolbar-kv-context-bar')).not.toBeInTheDocument();
    // The pre-track 48px band is unchanged: a bare spacer, nothing else.
    expect(toolbar.querySelector(':scope > div[class="flex-1"]')).not.toBeNull();
  });

  it('renders the driver cluster into the 48px band with the frozen props', () => {
    const request = vi.fn();
    const { container } = renderToolbar(binding(createKvSlotState(), request));
    const toolbar = container.firstElementChild as HTMLElement;

    const bar = screen.getByTestId('conn-toolbar-kv-context-bar');
    expect(bar.getAttribute('data-slot')).toBe('kv-context-bar');
    const fixture = screen.getByTestId('fixture-context-bar');
    expect(fixture.getAttribute('data-connection-id')).toBe('cfg-1');
    expect(fixture.getAttribute('data-db-session-id')).toBe('sess-1');
    expect(fixture.getAttribute('data-db-index')).toBe('7');
    expect(fixture.getAttribute('data-database')).toBe('db7');
    // The toolbar owns `compact`; the driver never has to observe the width itself.
    expect(fixture.getAttribute('data-compact')).toBe('false');
    // The driver cluster takes the free space instead of an empty band.
    expect(bar.className).toContain('flex-1');
    expect(toolbar.querySelector(':scope > div[class="flex-1"]')).toBeNull();
    // The reverse action channel reaches the slot through its props.
    expect(fixture.getAttribute('data-request')).toBe('wired');
    fireEvent.click(screen.getByTestId('fixture-request-flush'));
    expect(request).toHaveBeenCalledTimes(1);
    expect(request).toHaveBeenCalledWith({ type: 'flushDb' });
  });

  it('re-renders the driver cluster when the panel selection changes', () => {
    const state = createKvSlotState();
    renderToolbar(binding(state));
    expect(screen.getByTestId('fixture-context-bar').getAttribute('data-selected-key')).toBe('');

    act(() => {
      state.selectKey('user:42');
    });

    expect(screen.getByTestId('fixture-context-bar').getAttribute('data-selected-key')).toBe(
      'user:42',
    );
  });
});

/**
 * An AI button that opens a chat carrying no Redis fact is the half-broken state
 * this removes. The three cases below are the full state machine of that
 * affordance on a KV panel — hidden while nothing is in scope, shown for a key,
 * and hidden again once the selection is cleared (an entry condition with no exit
 * would be a one-way deadlock).
 */
describe('ContentToolbar AI button on a KV panel', () => {
  function renderKvToolbar() {
    const state = createKvSlotState();
    renderToolbar(binding(state), state);
    return state;
  }

  it('renders no AI affordance while the panel has no key in scope', () => {
    renderKvToolbar();
    expect(screen.queryByTestId('conn-toolbar-ai')).not.toBeInTheDocument();
  });

  it('treats a published blank key as no key', () => {
    const state = renderKvToolbar();
    act(() => {
      state.selectKey('   ');
    });
    expect(screen.queryByTestId('conn-toolbar-ai')).not.toBeInTheDocument();
  });

  it('shows the button when a key is selected and hides it again when cleared', () => {
    const state = renderKvToolbar();

    act(() => {
      state.selectKey('user:42');
    });
    const button = screen.getByTestId('conn-toolbar-ai');
    // The tooltip says what the assistant will be told about (asserted as an
    // i18n key, never as visible English copy).
    expect(button.getAttribute('title')).toBe('redis.ai.context.tooltip');

    act(() => {
      state.selectKey(null);
    });
    expect(screen.queryByTestId('conn-toolbar-ai')).not.toBeInTheDocument();
  });

  it('leaves a relational panel untouched: no relay, button as before', () => {
    renderToolbar();
    const button = screen.getByTestId('conn-toolbar-ai');
    // No KV tooltip is claimed for a panel whose facts are `contextTables`.
    expect(button.getAttribute('title')).not.toBe('redis.ai.context.tooltip');
  });

  // [tester] Panel switching is a rerender, not a fresh mount (keep-alive tabs).
  // An entry condition without its exit transitions would be a one-way deadlock,
  // so the AI-button state machine is driven live here: KV panel with a key →
  // another KV panel without one → a relational panel.
  it('[tester] follows a live panel switch: keyed KV → empty KV → relational', () => {
    const kvA = createKvSlotState();
    kvA.selectKey('user:42');
    const { rerender } = renderToolbar(binding(kvA), kvA);
    expect(screen.getByTestId('conn-toolbar-ai').getAttribute('title')).toBe(
      'redis.ai.context.tooltip',
    );

    // Another KV panel whose tree has nothing selected ⇒ the affordance leaves.
    const kvB = createKvSlotState();
    rerender(<ContentToolbar {...toolbarProps(binding(kvB), kvB)} />);
    expect(screen.queryByTestId('conn-toolbar-ai')).not.toBeInTheDocument();

    // A SQL panel ⇒ no relay at all; the pre-track default entry comes back
    // untouched (default title, no KV tooltip claim, context bar gone).
    rerender(<ContentToolbar {...toolbarProps(undefined, undefined)} />);
    const button = screen.getByTestId('conn-toolbar-ai');
    expect(button).toBeInTheDocument();
    expect(button.getAttribute('title')).not.toBe('redis.ai.context.tooltip');
    expect(screen.queryByTestId('conn-toolbar-kv-context-bar')).not.toBeInTheDocument();
  });
});
