import { describe, expect, it, vi } from 'vitest';
import {
  buildConnectionTabContextMenuItems,
  type ConnectionTabContextMenuLabels,
} from '../connectionTabContextMenu';

const labels: ConnectionTabContextMenuLabels = {
  close: 'Close',
  closeOthers: 'Close Others',
  closeAll: 'Close All',
  closeToTheRight: 'Close to the Right',
  closeToTheLeft: 'Close to the Left',
};

function ids(items: ReturnType<typeof buildConnectionTabContextMenuItems>): string[] {
  return items.map((i) => (i.kind === 'item' ? i.id : i.kind));
}

describe('buildConnectionTabContextMenuItems', () => {
  it('builds close / others / to-right / to-left / close-all when handlers are set', () => {
    const onClose = vi.fn();
    const onCloseOthers = vi.fn();
    const onCloseAll = vi.fn();
    const onCloseToTheRight = vi.fn();
    const onCloseToTheLeft = vi.fn();
    const items = buildConnectionTabContextMenuItems({
      labels,
      handlers: { onClose, onCloseOthers, onCloseAll, onCloseToTheRight, onCloseToTheLeft },
      hasTabsToRight: true,
      hasTabsToLeft: true,
    });
    expect(ids(items)).toEqual([
      'close',
      'close-others',
      'close-to-the-right',
      'close-to-the-left',
      'close-all',
    ]);
    for (const it of items) {
      if (it.kind === 'item') {
        expect(it.enabled).not.toBe(false);
        it.action();
      }
    }
    expect(onClose).toHaveBeenCalledOnce();
    expect(onCloseOthers).toHaveBeenCalledOnce();
    expect(onCloseAll).toHaveBeenCalledOnce();
    expect(onCloseToTheRight).toHaveBeenCalledOnce();
    expect(onCloseToTheLeft).toHaveBeenCalledOnce();
  });

  it('disables close-others / to-right / to-left when flags say so', () => {
    const items = buildConnectionTabContextMenuItems({
      labels,
      handlers: {
        onClose: vi.fn(),
        onCloseOthers: vi.fn(),
        onCloseAll: vi.fn(),
        onCloseToTheRight: vi.fn(),
        onCloseToTheLeft: vi.fn(),
      },
      onlyOneTab: true,
      hasTabsToRight: false,
      hasTabsToLeft: false,
    });
    expect(ids(items)).toEqual([
      'close',
      'close-others',
      'close-to-the-right',
      'close-to-the-left',
      'close-all',
    ]);
    expect(items.find((i) => i.kind === 'item' && i.id === 'close-others')).toMatchObject({
      enabled: false,
    });
    expect(items.find((i) => i.kind === 'item' && i.id === 'close-to-the-right')).toMatchObject({
      enabled: false,
    });
    expect(items.find((i) => i.kind === 'item' && i.id === 'close-to-the-left')).toMatchObject({
      enabled: false,
    });
  });

  it('enables close-others when onlyOneTab is false or omitted', () => {
    const withFalse = buildConnectionTabContextMenuItems({
      labels,
      handlers: {
        onClose: vi.fn(),
        onCloseOthers: vi.fn(),
        onCloseAll: vi.fn(),
      },
      onlyOneTab: false,
    });
    const omitted = buildConnectionTabContextMenuItems({
      labels,
      handlers: {
        onClose: vi.fn(),
        onCloseOthers: vi.fn(),
        onCloseAll: vi.fn(),
      },
    });
    for (const items of [withFalse, omitted]) {
      const closeOthers = items.find((i) => i.kind === 'item' && i.id === 'close-others');
      expect(closeOthers).toMatchObject({ kind: 'item', enabled: true });
    }
  });

  it('skips items whose handlers are missing', () => {
    const items = buildConnectionTabContextMenuItems({
      labels,
      handlers: { onClose: vi.fn() },
      onlyOneTab: true,
    });
    expect(ids(items)).toEqual(['close']);
  });

  it('returns empty when no handlers', () => {
    expect(ids(buildConnectionTabContextMenuItems({ labels, handlers: {} }))).toEqual([]);
  });

  it('uses caller-supplied labels', () => {
    const items = buildConnectionTabContextMenuItems({
      labels: {
        close: '关闭',
        closeOthers: '关闭其他',
        closeAll: '关闭全部',
        closeToTheRight: '关闭右侧',
        closeToTheLeft: '关闭左侧',
      },
      handlers: {
        onClose: vi.fn(),
        onCloseOthers: vi.fn(),
        onCloseAll: vi.fn(),
        onCloseToTheRight: vi.fn(),
        onCloseToTheLeft: vi.fn(),
      },
      hasTabsToRight: true,
      hasTabsToLeft: true,
    });
    const texts = items.map((i) => (i.kind === 'item' ? i.label : ''));
    expect(texts).toEqual(['关闭', '关闭其他', '关闭右侧', '关闭左侧', '关闭全部']);
  });
});
