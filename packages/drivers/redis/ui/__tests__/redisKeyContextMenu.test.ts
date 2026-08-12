import { describe, expect, it, vi } from 'vitest';
import {
  buildRedisKeyContextMenuItems,
  type RedisKeyContextMenuLabels,
} from '../redisKeyContextMenu';

const labels: RedisKeyContextMenuLabels = {
  copyKey: 'Copy Name',
  setTtl: 'Set TTL',
  rename: 'Rename',
  delete: 'Delete',
};

function ids(items: ReturnType<typeof buildRedisKeyContextMenuItems>): string[] {
  return items.map((i) => (i.kind === 'item' ? i.id : i.kind));
}

describe('buildRedisKeyContextMenuItems', () => {
  it('builds copy / set-ttl / rename / delete when handlers are set', () => {
    const onCopyKey = vi.fn();
    const onSetTtl = vi.fn();
    const onRename = vi.fn();
    const onDelete = vi.fn();
    const items = buildRedisKeyContextMenuItems({
      labels,
      handlers: { onCopyKey, onSetTtl, onRename, onDelete },
    });
    expect(ids(items)).toEqual(['copy-key', 'set-ttl', 'rename', 'delete']);
    for (const it of items) {
      if (it.kind === 'item') it.action();
    }
    expect(onCopyKey).toHaveBeenCalledOnce();
    expect(onSetTtl).toHaveBeenCalledOnce();
    expect(onRename).toHaveBeenCalledOnce();
    expect(onDelete).toHaveBeenCalledOnce();
  });

  it('skips items whose handlers are missing', () => {
    const items = buildRedisKeyContextMenuItems({
      labels,
      handlers: { onCopyKey: vi.fn(), onDelete: vi.fn() },
    });
    expect(ids(items)).toEqual(['copy-key', 'delete']);
  });

  it('returns empty when no handlers', () => {
    expect(ids(buildRedisKeyContextMenuItems({ labels, handlers: {} }))).toEqual([]);
  });

  it('uses caller-supplied labels', () => {
    const items = buildRedisKeyContextMenuItems({
      labels: {
        copyKey: '复制名称',
        setTtl: '设置 TTL',
        rename: '重命名',
        delete: '删除',
      },
      handlers: {
        onCopyKey: vi.fn(),
        onSetTtl: vi.fn(),
        onRename: vi.fn(),
        onDelete: vi.fn(),
      },
    });
    const texts = items.map((i) => (i.kind === 'item' ? i.label : ''));
    expect(texts).toEqual(['复制名称', '设置 TTL', '重命名', '删除']);
  });

  it('omits only set-ttl and rename when those handlers are absent', () => {
    const items = buildRedisKeyContextMenuItems({
      labels,
      handlers: { onCopyKey: vi.fn(), onRename: vi.fn() },
    });
    expect(ids(items)).toEqual(['copy-key', 'rename']);
  });
});
