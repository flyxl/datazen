import { describe, expect, it, vi } from 'vitest';
import {
  buildWorkflowHistoryContextMenuItems,
  buildWorkflowListContextMenuItems,
  type WorkflowHistoryContextMenuLabels,
  type WorkflowListContextMenuLabels,
} from '../workflowListContextMenu';

const listLabels: WorkflowListContextMenuLabels = {
  open: 'Open',
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
  it('builds open / delete / copy-name and never includes run', () => {
    const onOpen = vi.fn();
    const onDelete = vi.fn();
    const onCopyName = vi.fn();
    const items = buildWorkflowListContextMenuItems({
      labels: listLabels,
      handlers: { onOpen, onDelete, onCopyName },
    });
    expect(ids(items)).toEqual(['open', 'delete', 'copy-name']);
    expect(ids(items)).not.toContain('run');
    for (const it of items) {
      if (it.kind === 'item') it.action();
    }
    expect(onOpen).toHaveBeenCalledOnce();
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
  });
});
