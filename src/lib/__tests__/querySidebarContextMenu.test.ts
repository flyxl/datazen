import { describe, expect, it, vi } from 'vitest';
import {
  buildFavoriteSidebarContextMenuItems,
  buildHistorySidebarContextMenuItems,
  buildHistorySidebarHeaderContextMenuItems,
  type QuerySidebarFavoriteLabels,
  type QuerySidebarHistoryHeaderLabels,
  type QuerySidebarHistoryLabels,
} from '../querySidebarContextMenu';

const favoriteLabels: QuerySidebarFavoriteLabels = {
  applySql: 'Apply SQL',
  copySql: 'Copy SQL',
  delete: 'Delete',
};

const historyLabels: QuerySidebarHistoryLabels = {
  applySql: 'Apply SQL',
  copySql: 'Copy SQL',
};

const headerLabels: QuerySidebarHistoryHeaderLabels = {
  clearHistory: 'Clear History',
};

function ids(
  items: ReturnType<
    | typeof buildFavoriteSidebarContextMenuItems
    | typeof buildHistorySidebarContextMenuItems
    | typeof buildHistorySidebarHeaderContextMenuItems
  >,
): string[] {
  return items.map((i) => (i.kind === 'item' ? i.id : i.kind));
}

describe('buildFavoriteSidebarContextMenuItems', () => {
  it('builds apply / copy / delete when handlers are set', () => {
    const onApplySql = vi.fn();
    const onCopySql = vi.fn();
    const onDelete = vi.fn();
    const items = buildFavoriteSidebarContextMenuItems({
      labels: favoriteLabels,
      handlers: { onApplySql, onCopySql, onDelete },
    });
    expect(ids(items)).toEqual(['apply-sql', 'copy-sql', 'delete']);
    for (const it of items) {
      if (it.kind === 'item') it.action();
    }
    expect(onApplySql).toHaveBeenCalledOnce();
    expect(onCopySql).toHaveBeenCalledOnce();
    expect(onDelete).toHaveBeenCalledOnce();
  });

  it('skips items whose handlers are missing', () => {
    const items = buildFavoriteSidebarContextMenuItems({
      labels: favoriteLabels,
      handlers: { onApplySql: vi.fn() },
    });
    expect(ids(items)).toEqual(['apply-sql']);
  });

  it('returns empty when no handlers', () => {
    expect(
      ids(buildFavoriteSidebarContextMenuItems({ labels: favoriteLabels, handlers: {} })),
    ).toEqual([]);
  });

  it('uses caller-supplied labels', () => {
    const items = buildFavoriteSidebarContextMenuItems({
      labels: {
        applySql: '应用 SQL',
        copySql: '复制 SQL',
        delete: '删除',
      },
      handlers: {
        onApplySql: vi.fn(),
        onCopySql: vi.fn(),
        onDelete: vi.fn(),
      },
    });
    const texts = items.map((i) => (i.kind === 'item' ? i.label : ''));
    expect(texts).toEqual(['应用 SQL', '复制 SQL', '删除']);
  });
});

describe('buildHistorySidebarContextMenuItems', () => {
  it('builds apply / copy when handlers are set', () => {
    const onApplySql = vi.fn();
    const onCopySql = vi.fn();
    const items = buildHistorySidebarContextMenuItems({
      labels: historyLabels,
      handlers: { onApplySql, onCopySql },
    });
    expect(ids(items)).toEqual(['apply-sql', 'copy-sql']);
    for (const it of items) {
      if (it.kind === 'item') it.action();
    }
    expect(onApplySql).toHaveBeenCalledOnce();
    expect(onCopySql).toHaveBeenCalledOnce();
  });

  it('skips items whose handlers are missing', () => {
    const items = buildHistorySidebarContextMenuItems({
      labels: historyLabels,
      handlers: { onCopySql: vi.fn() },
    });
    expect(ids(items)).toEqual(['copy-sql']);
  });

  it('returns empty when no handlers', () => {
    expect(
      ids(buildHistorySidebarContextMenuItems({ labels: historyLabels, handlers: {} })),
    ).toEqual([]);
  });

  it('uses caller-supplied labels', () => {
    const items = buildHistorySidebarContextMenuItems({
      labels: {
        applySql: '应用 SQL',
        copySql: '复制 SQL',
      },
      handlers: {
        onApplySql: vi.fn(),
        onCopySql: vi.fn(),
      },
    });
    const texts = items.map((i) => (i.kind === 'item' ? i.label : ''));
    expect(texts).toEqual(['应用 SQL', '复制 SQL']);
  });
});

describe('buildHistorySidebarHeaderContextMenuItems', () => {
  it('builds clear-history when handler is set', () => {
    const onClearHistory = vi.fn();
    const items = buildHistorySidebarHeaderContextMenuItems({
      labels: headerLabels,
      handlers: { onClearHistory },
    });
    expect(ids(items)).toEqual(['clear-history']);
    const first = items[0]!;
    if (first.kind === 'item') first.action();
    expect(onClearHistory).toHaveBeenCalledOnce();
  });

  it('returns empty when clear handler is missing', () => {
    expect(
      ids(buildHistorySidebarHeaderContextMenuItems({ labels: headerLabels, handlers: {} })),
    ).toEqual([]);
  });

  it('uses caller-supplied label', () => {
    const items = buildHistorySidebarHeaderContextMenuItems({
      labels: { clearHistory: '清空历史' },
      handlers: { onClearHistory: vi.fn() },
    });
    expect(items[0]).toMatchObject({ kind: 'item', label: '清空历史' });
  });
});
