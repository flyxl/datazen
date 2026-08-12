import { describe, expect, it, vi } from 'vitest';
import {
  buildWorkflowHistoryContextMenuItems,
  buildWorkflowListContextMenuItems,
  type WorkflowHistoryContextMenuLabels,
  type WorkflowListContextMenuLabels,
} from '../workflowListContextMenu';

const listLabels: WorkflowListContextMenuLabels = {
  open: 'Open',
  run: 'Run',
  delete: 'Delete',
  copyName: 'Copy Name',
};

const historyLabels: WorkflowHistoryContextMenuLabels = {
  openDetail: 'Open Details',
  delete: 'Delete',
};

function ids(
  items: ReturnType<
    typeof buildWorkflowListContextMenuItems | typeof buildWorkflowHistoryContextMenuItems
  >,
): string[] {
  return items.map((i) => (i.kind === 'item' ? i.id : i.kind));
}

describe('buildWorkflowListContextMenuItems', () => {
  it('builds open / run / delete / copy-name when handlers are set', () => {
    const onOpen = vi.fn();
    const onRun = vi.fn();
    const onDelete = vi.fn();
    const onCopyName = vi.fn();
    const items = buildWorkflowListContextMenuItems({
      labels: listLabels,
      handlers: { onOpen, onRun, onDelete, onCopyName },
    });
    expect(ids(items)).toEqual(['open', 'run', 'delete', 'copy-name']);
    for (const it of items) {
      if (it.kind === 'item') it.action();
    }
    expect(onOpen).toHaveBeenCalledOnce();
    expect(onRun).toHaveBeenCalledOnce();
    expect(onDelete).toHaveBeenCalledOnce();
    expect(onCopyName).toHaveBeenCalledOnce();
  });

  it('skips items whose handlers are missing', () => {
    const items = buildWorkflowListContextMenuItems({
      labels: listLabels,
      handlers: { onOpen: vi.fn(), onCopyName: vi.fn() },
    });
    expect(ids(items)).toEqual(['open', 'copy-name']);
  });

  it('returns empty when no handlers', () => {
    expect(ids(buildWorkflowListContextMenuItems({ labels: listLabels, handlers: {} }))).toEqual(
      [],
    );
  });

  it('uses caller-supplied labels', () => {
    const items = buildWorkflowListContextMenuItems({
      labels: {
        open: '打开',
        run: '运行',
        delete: '删除',
        copyName: '复制名称',
      },
      handlers: {
        onOpen: vi.fn(),
        onRun: vi.fn(),
        onDelete: vi.fn(),
        onCopyName: vi.fn(),
      },
    });
    const texts = items.map((i) => (i.kind === 'item' ? i.label : ''));
    expect(texts).toEqual(['打开', '运行', '删除', '复制名称']);
  });
});

describe('buildWorkflowHistoryContextMenuItems', () => {
  it('builds open-detail when handler is set', () => {
    const onOpenDetail = vi.fn();
    const items = buildWorkflowHistoryContextMenuItems({
      labels: { openDetail: 'Open Details' },
      handlers: { onOpenDetail },
    });
    expect(ids(items)).toEqual(['open-detail']);
    const first = items[0]!;
    if (first.kind === 'item') first.action();
    expect(onOpenDetail).toHaveBeenCalledOnce();
  });

  it('includes delete only when onDelete is provided', () => {
    const onOpenDetail = vi.fn();
    const onDelete = vi.fn();
    const items = buildWorkflowHistoryContextMenuItems({
      labels: historyLabels,
      handlers: { onOpenDetail, onDelete },
    });
    expect(ids(items)).toEqual(['open-detail', 'delete']);
    for (const it of items) {
      if (it.kind === 'item') it.action();
    }
    expect(onOpenDetail).toHaveBeenCalledOnce();
    expect(onDelete).toHaveBeenCalledOnce();
  });

  it('omits delete when handler is missing even if label is set', () => {
    const items = buildWorkflowHistoryContextMenuItems({
      labels: historyLabels,
      handlers: { onOpenDetail: vi.fn() },
    });
    expect(ids(items)).toEqual(['open-detail']);
  });

  it('returns empty when no handlers', () => {
    expect(
      ids(buildWorkflowHistoryContextMenuItems({ labels: historyLabels, handlers: {} })),
    ).toEqual([]);
  });

  it('uses caller-supplied labels', () => {
    const items = buildWorkflowHistoryContextMenuItems({
      labels: { openDetail: '打开详情', delete: '删除' },
      handlers: { onOpenDetail: vi.fn(), onDelete: vi.fn() },
    });
    const texts = items.map((i) => (i.kind === 'item' ? i.label : ''));
    expect(texts).toEqual(['打开详情', '删除']);
  });
});
