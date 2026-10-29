import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { ExecutionStrategySelect } from '../toolbar/ExecutionStrategySelect';
import { useSettingsStore } from '../../../../stores/settingsStore';
import type { AppSettings } from '../../../../types';
import { showNativeContextMenu, type NativeMenuItemDef } from '../../../../lib/nativeContextMenu';

vi.mock('../../../../lib/nativeContextMenu', () => ({ showNativeContextMenu: vi.fn() }));
vi.mock('../../../../hooks/useI18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }));

const previousState = useSettingsStore.getState();
afterEach(() => {
  cleanup();
  useSettingsStore.setState(previousState);
  vi.clearAllMocks();
});

describe('Execution strategy toolbar journey', () => {
  it('shows current statement when unset, saves whole script choice, and reflects it on reopen', async () => {
    const updateSettings = vi.fn(async (partial: Partial<AppSettings>) => {
      useSettingsStore.setState((state) => ({ settings: { ...state.settings, ...partial } }));
    });
    useSettingsStore.setState({
      settings: { ...previousState.settings, sqlExecutionStrategy: undefined },
      updateSettings,
    });
    render(<ExecutionStrategySelect />);
    const title = 'query.executionStrategy.title: query.executionStrategy.currentStatement';
    const button = screen.getByTitle(title);
    fireEvent.click(button);
    let items = vi.mocked(showNativeContextMenu).mock.lastCall?.[0] ?? [];
    const currentItem = items.find(
      (item) => 'id' in item && item.id === 'strategy-current_statement',
    );
    expect(currentItem).toMatchObject({ label: '✓ query.executionStrategy.currentStatement' });
    const entireItem = items.find(
      (item): item is Extract<NativeMenuItemDef, { kind: 'item' }> =>
        item.kind === 'item' && item.id === 'strategy-entire_script',
    );
    if (!entireItem) throw new Error('Missing whole script choice');
    await act(async () => {
      await entireItem.action();
    });
    expect(updateSettings).toHaveBeenCalledWith({ sqlExecutionStrategy: 'entire_script' });
    fireEvent.click(
      screen.getByTitle('query.executionStrategy.title: query.executionStrategy.entireScript'),
    );
    items = vi.mocked(showNativeContextMenu).mock.lastCall?.[0] ?? [];
    expect(
      items.find((item) => 'id' in item && item.id === 'strategy-entire_script'),
    ).toMatchObject({ label: '✓ query.executionStrategy.entireScript' });
  });
});
