import { describe, expect, it, vi } from 'vitest';
import { buildErNodeContextMenuItems, type ErNodeContextMenuLabels } from '../erNodeContextMenu';

const labels: ErNodeContextMenuLabels = {
  openTable: 'Open Table',
  copyName: 'Copy Name',
  focusTable: 'Focus this table',
};

function ids(items: ReturnType<typeof buildErNodeContextMenuItems>): string[] {
  return items.map((i) => (i.kind === 'item' ? i.id : i.kind));
}

describe('buildErNodeContextMenuItems', () => {
  it('builds open / copy / focus when all handlers are set', () => {
    const onOpenTable = vi.fn();
    const onCopyName = vi.fn();
    const onFocusTable = vi.fn();
    const items = buildErNodeContextMenuItems({
      labels,
      handlers: { onOpenTable, onCopyName, onFocusTable },
    });
    expect(ids(items)).toEqual(['open-table', 'copy-name', 'focus-table']);
    for (const it of items) {
      if (it.kind === 'item') it.action();
    }
    expect(onOpenTable).toHaveBeenCalledOnce();
    expect(onCopyName).toHaveBeenCalledOnce();
    expect(onFocusTable).toHaveBeenCalledOnce();
  });

  it('omits focus when onFocusTable is missing', () => {
    const items = buildErNodeContextMenuItems({
      labels,
      handlers: { onOpenTable: vi.fn(), onCopyName: vi.fn() },
    });
    expect(ids(items)).toEqual(['open-table', 'copy-name']);
  });

  it('includes only copy when that is the only handler', () => {
    const onCopyName = vi.fn();
    const items = buildErNodeContextMenuItems({
      labels,
      handlers: { onCopyName },
    });
    expect(ids(items)).toEqual(['copy-name']);
    const first = items[0]!;
    if (first.kind === 'item') first.action();
    expect(onCopyName).toHaveBeenCalledOnce();
  });

  it('skips items whose handlers are missing', () => {
    const items = buildErNodeContextMenuItems({
      labels,
      handlers: { onOpenTable: vi.fn() },
    });
    expect(ids(items)).toEqual(['open-table']);
  });

  it('returns empty when no handlers', () => {
    expect(ids(buildErNodeContextMenuItems({ labels, handlers: {} }))).toEqual([]);
  });

  it('uses caller-supplied labels', () => {
    const items = buildErNodeContextMenuItems({
      labels: {
        openTable: '打开表',
        copyName: '复制名称',
        focusTable: '聚焦此表',
      },
      handlers: {
        onOpenTable: vi.fn(),
        onCopyName: vi.fn(),
        onFocusTable: vi.fn(),
      },
    });
    const texts = items.map((i) => (i.kind === 'item' ? i.label : ''));
    expect(texts).toEqual(['打开表', '复制名称', '聚焦此表']);
  });

  it('includes focus alone when only onFocusTable is set', () => {
    const onFocusTable = vi.fn();
    const items = buildErNodeContextMenuItems({
      labels,
      handlers: { onFocusTable },
    });
    expect(ids(items)).toEqual(['focus-table']);
    const first = items[0]!;
    if (first.kind === 'item') first.action();
    expect(onFocusTable).toHaveBeenCalledOnce();
  });
});
