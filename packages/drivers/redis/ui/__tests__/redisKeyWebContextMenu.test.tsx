import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import fs from 'node:fs';
import path from 'node:path';
import { WebContextMenuHost } from '../../../../../src/components/ui/WebContextMenu';
import { showNativeContextMenu } from '../../../../../src/lib/nativeContextMenu';
import { useContextMenuStore } from '../../../../../src/stores/contextMenuStore';
import { buildRedisKeyContextMenuItems } from '../redisKeyContextMenu';

afterEach(() => {
  useContextMenuStore.getState().hide();
  cleanup();
});

describe('Redis key web context menu', () => {
  it('opens a web menu at client coordinates and runs key actions', async () => {
    const onCopyKey = vi.fn();
    const onSetTtl = vi.fn();
    const onRename = vi.fn();
    const onDelete = vi.fn();
    render(<WebContextMenuHost />);

    showNativeContextMenu(
      buildRedisKeyContextMenuItems({
        labels: {
          copyKey: 'Copy name',
          setTtl: 'Set TTL',
          rename: 'Rename',
          delete: 'Delete',
        },
        handlers: { onCopyKey, onSetTtl, onRename, onDelete },
      }),
      { x: 120, y: 40 },
    );

    await screen.findByTestId('web-context-menu');
    fireEvent.click(await screen.findByTestId('web-context-item-copy-key'));
    expect(onCopyKey).toHaveBeenCalledOnce();

    showNativeContextMenu(
      buildRedisKeyContextMenuItems({
        labels: {
          copyKey: 'Copy name',
          setTtl: 'Set TTL',
          rename: 'Rename',
          delete: 'Delete',
        },
        handlers: { onCopyKey, onSetTtl, onRename, onDelete },
      }),
      { x: 10, y: 10 },
    );
    await screen.findByTestId('web-context-item-delete');
    fireEvent.click(screen.getByTestId('web-context-item-delete'));
    expect(onDelete).toHaveBeenCalledOnce();
  });

  it('wires RedisWorkbench key rows to showNativeContextMenu with client coords', () => {
    const src = fs.readFileSync(
      path.resolve(import.meta.dirname, '../RedisWorkbench.tsx'),
      'utf8',
    );
    expect(src).toContain('showNativeContextMenu');
    expect(src).toContain('{ x: e.clientX, y: e.clientY }');
    expect(src).not.toContain('@tauri-apps/api/menu');
  });
});
