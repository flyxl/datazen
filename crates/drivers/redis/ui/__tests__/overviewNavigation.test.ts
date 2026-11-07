import { describe, expect, it, vi } from 'vitest';
import {
  jumpStateAttribute,
  parseDbIndex,
  planOverviewJump,
  requestOverviewJump,
  typeBadgeClass,
  typeTone,
  type OverviewJumpTarget,
} from '../overview/overviewNavigation';

/**
 * 屏 A → 屏 B 跳转模型（当前为降级态）。
 *
 * The host contract (`ConnectionHomeSlotProps`) carries no panel-open bridge, so
 * a jump without an injected handler must report "not handled" plus a **named**
 * fallback key instead of pretending the panel opened. These cases pin that the
 * degradation is explicit and that a misbehaving bridge cannot take 屏 A down.
 */

const TARGETS: OverviewJumpTarget[] = [
  { kind: 'database', dbIndex: 3 },
  { kind: 'key', dbIndex: 3, key: 'user:1' },
  { kind: 'newKey', dbIndex: 3 },
  { kind: 'console' },
  { kind: 'pubsub' },
  { kind: 'monitor', section: 'memory' },
  { kind: 'importExport' },
];

describe('planOverviewJump', () => {
  it('reports every target as unhandled when the host bridge is absent', () => {
    for (const target of TARGETS) {
      const outcome = planOverviewJump(target, undefined);
      expect(outcome.handled).toBe(false);
      expect(outcome.target).toBe(target);
      expect(outcome.hintKey).not.toBeNull();
    }
  });

  it('names the left-tree entry for db/key targets and the panel entry for the rest', () => {
    expect(planOverviewJump({ kind: 'database', dbIndex: 0 }, undefined).hintKey).toBe(
      'redis.overview.jump.pendingTree',
    );
    expect(planOverviewJump({ kind: 'key', dbIndex: 0, key: 'user:1' }, undefined).hintKey).toBe(
      'redis.overview.jump.pendingTree',
    );
    expect(planOverviewJump({ kind: 'newKey', dbIndex: 0 }, undefined).hintKey).toBe(
      'redis.overview.jump.pendingTree',
    );
    expect(planOverviewJump({ kind: 'console' }, undefined).hintKey).toBe(
      'redis.overview.jump.pendingPanel',
    );
    expect(planOverviewJump({ kind: 'monitor', section: 'slowlog' }, undefined).hintKey).toBe(
      'redis.overview.jump.pendingPanel',
    );
  });

  it('reports handled when a bridge is present', () => {
    const handler = vi.fn();
    const outcome = planOverviewJump({ kind: 'console' }, handler);
    expect(outcome.handled).toBe(true);
    expect(outcome.hintKey).toBeNull();
    // planning must not itself call the bridge
    expect(handler).not.toHaveBeenCalled();
  });
});

describe('requestOverviewJump', () => {
  it('hands the untouched target to the bridge and reports success', () => {
    const handler = vi.fn();
    const target: OverviewJumpTarget = { kind: 'key', dbIndex: 5, key: 'queue:jobs' };
    const outcome = requestOverviewJump(target, handler);
    expect(handler).toHaveBeenCalledWith(target);
    expect(outcome).toEqual({ target, handled: true, hintKey: null });
  });

  it('degrades to a named failure hint when the bridge throws', () => {
    const handler = vi.fn(() => {
      throw new Error('bridge exploded');
    });
    const outcome = requestOverviewJump({ kind: 'database', dbIndex: 1 }, handler);
    expect(handler).toHaveBeenCalled();
    expect(outcome.handled).toBe(false);
    expect(outcome.hintKey).toBe('redis.overview.jump.failed');
  });

  it('never calls anything when there is no bridge', () => {
    const outcome = requestOverviewJump({ kind: 'pubsub' }, undefined);
    expect(outcome.handled).toBe(false);
    expect(outcome.hintKey).toBe('redis.overview.jump.pendingPanel');
  });
});

describe('jumpStateAttribute', () => {
  it('marks affordances unwired vs wired for E2E', () => {
    expect(jumpStateAttribute(undefined)).toBe('unwired');
    expect(jumpStateAttribute(vi.fn())).toBe('wired');
    // A non-function truthy value must not be advertised as wired.
    expect(jumpStateAttribute(undefined as unknown as (() => void) | undefined)).toBe('unwired');
  });
});

describe('typeTone', () => {
  it('maps Redis TYPE tokens to a badge tone', () => {
    expect(typeTone('string')).toBe('accent');
    expect(typeTone('hash')).toBe('success');
    expect(typeTone('list')).toBe('warning');
    expect(typeTone('set')).toBe('danger');
    expect(typeTone('zset')).toBe('danger');
    expect(typeTone('stream')).toBe('accent');
  });

  it('falls back to neutral for unknown or missing types instead of inventing a colour', () => {
    expect(typeTone('tairString')).toBe('neutral');
    expect(typeTone(null)).toBe('neutral');
    expect(typeTone(undefined)).toBe('neutral');
    expect(typeTone('')).toBe('neutral');
    expect(typeTone('HASH')).toBe('success');
  });
});

describe('typeBadgeClass', () => {
  it('renders the typeTone vocabulary as Tailwind classes for the 屏 A badges', () => {
    // BUG-001: the big-key row and the recent-key chip colour themselves here —
    // assert the tone tokens, not translated copy.
    expect(typeBadgeClass('string')).toContain('text-accent');
    expect(typeBadgeClass('hash')).toContain('text-success');
    expect(typeBadgeClass('list')).toContain('text-warning');
    expect(typeBadgeClass('set')).toContain('text-danger');
  });

  it('degrades an unreadable type to the neutral outline instead of a guessed colour', () => {
    expect(typeBadgeClass(null)).toContain('text-fg-muted');
    expect(typeBadgeClass(undefined)).toContain('border-edge');
    expect(typeBadgeClass('tairString')).toBe(typeBadgeClass(null));
  });
});

describe('parseDbIndex', () => {
  it('accepts both dbN and bare index forms', () => {
    expect(parseDbIndex('db0')).toBe(0);
    expect(parseDbIndex('DB15')).toBe(15);
    expect(parseDbIndex(' 7 ')).toBe(7);
    expect(parseDbIndex(4)).toBe(4);
  });

  it('falls back to db0 for anything unparseable', () => {
    expect(parseDbIndex(null)).toBe(0);
    expect(parseDbIndex(undefined)).toBe(0);
    expect(parseDbIndex('')).toBe(0);
    expect(parseDbIndex('default')).toBe(0);
    expect(parseDbIndex('db-1')).toBe(0);
    expect(parseDbIndex(Number.NaN)).toBe(0);
  });

  it('clamps a negative numeric index into the logical range', () => {
    expect(parseDbIndex(-3)).toBe(0);
    expect(parseDbIndex(2.7)).toBe(2);
  });
});
