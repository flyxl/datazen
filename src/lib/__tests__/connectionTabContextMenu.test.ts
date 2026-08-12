import { describe, expect, it, vi } from 'vitest';
import {
  buildConnectionTabContextMenuItems,
  type ConnectionTabContextMenuLabels,
} from '../connectionTabContextMenu';

const labels: ConnectionTabContextMenuLabels = {
  close: 'Close',
  closeOthers: 'Close Others',
  closeAll: 'Close All',
};

function ids(items: ReturnType<typeof buildConnectionTabContextMenuItems>): string[] {
  return items.map((i) => (i.kind === 'item' ? i.id : i.kind));
}

describe('buildConnectionTabContextMenuItems', () => {
  it('builds close / close-others / close-all when handlers are set', () => {
    const onClose = vi.fn();
    const onCloseOthers = vi.fn();
    const onCloseAll = vi.fn();
    const items = buildConnectionTabContextMenuItems({
      labels,
      handlers: { onClose, onCloseOthers, onCloseAll },
    });
    expect(ids(items)).toEqual(['close', 'close-others', 'close-all']);
    for (const it of items) {
      if (it.kind === 'item') {
        expect(it.enabled).not.toBe(false);
        it.action();
      }
    }
    expect(onClose).toHaveBeenCalledOnce();
    expect(onCloseOthers).toHaveBeenCalledOnce();
    expect(onCloseAll).toHaveBeenCalledOnce();
  });

  it('disables close-others when onlyOneTab', () => {
    const onCloseOthers = vi.fn();
    const items = buildConnectionTabContextMenuItems({
      labels,
      handlers: {
        onClose: vi.fn(),
        onCloseOthers,
        onCloseAll: vi.fn(),
      },
      onlyOneTab: true,
    });
    expect(ids(items)).toEqual(['close', 'close-others', 'close-all']);
    const closeOthers = items.find((i) => i.kind === 'item' && i.id === 'close-others');
    expect(closeOthers).toMatchObject({ kind: 'item', enabled: false });
    const close = items.find((i) => i.kind === 'item' && i.id === 'close');
    const closeAll = items.find((i) => i.kind === 'item' && i.id === 'close-all');
    expect(close).toMatchObject({ kind: 'item', enabled: true });
    expect(closeAll).toMatchObject({ kind: 'item', enabled: true });
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
      },
      handlers: {
        onClose: vi.fn(),
        onCloseOthers: vi.fn(),
        onCloseAll: vi.fn(),
      },
    });
    const texts = items.map((i) => (i.kind === 'item' ? i.label : ''));
    expect(texts).toEqual(['关闭', '关闭其他', '关闭全部']);
  });
});
