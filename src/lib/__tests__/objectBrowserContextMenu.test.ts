import { describe, expect, it, vi } from 'vitest';
import {
  buildObjectBrowserEditorMenuItems,
  buildObjectBrowserListMenuItems,
} from '../objectBrowserContextMenu';

describe('buildObjectBrowserListMenuItems', () => {
  it('builds open / copy / refresh items', () => {
    const onOpen = vi.fn();
    const onCopyName = vi.fn();
    const onCopyDdl = vi.fn();
    const onRefresh = vi.fn();
    const items = buildObjectBrowserListMenuItems({
      labels: { open: 'Open', copyName: 'Copy name', copyDdl: 'Copy DDL', refresh: 'Refresh' },
      handlers: { onOpen, onCopyName, onCopyDdl, onRefresh },
    });
    expect(items.map((i) => (i.kind === 'item' ? i.id : i.kind))).toEqual([
      'refresh',
      'open',
      'copy-name',
      'copy-ddl',
    ]);
    for (const it of items) {
      if (it.kind === 'item') it.action();
    }
    expect(onOpen).toHaveBeenCalledOnce();
    expect(onCopyName).toHaveBeenCalledOnce();
    expect(onCopyDdl).toHaveBeenCalledOnce();
    expect(onRefresh).toHaveBeenCalledOnce();
  });
});

describe('buildObjectBrowserEditorMenuItems', () => {
  it('includes edit actions plus execute / format / comment', () => {
    const items = buildObjectBrowserEditorMenuItems({
      labels: { execute: 'Execute', format: 'Format', comment: 'Comment' },
      handlers: { onExecute: vi.fn(), onFormat: vi.fn(), onComment: vi.fn() },
      sqlText: 'CREATE FUNCTION x()',
      hasSelection: false,
    });
    const ids = items.map((i) =>
      i.kind === 'predefined' ? i.item : i.kind === 'item' ? i.id : i.kind,
    );
    expect(ids).toEqual([
      'Cut',
      'Copy',
      'Paste',
      'SelectAll',
      'separator',
      'execute',
      'format',
      'comment',
    ]);
  });
});
