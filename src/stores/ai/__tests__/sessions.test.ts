import { describe, it, expect, beforeEach, vi } from 'vitest';
import { computeSessionKey, loadSessions, saveSessions, touchSession } from '../sessions';

describe('computeSessionKey', () => {
  it('uses connectionId when provided', () => {
    expect(computeSessionKey('conn-1', 'sess-1', 'mydb')).toBe('conn-1::mydb');
  });

  it('falls back to dbSessionId when connectionId is absent', () => {
    expect(computeSessionKey(undefined, 'sess-1', 'mydb')).toBe('sess-1::mydb');
  });

  it('falls back to default when both are absent', () => {
    expect(computeSessionKey(undefined, undefined, 'mydb')).toBe('default::mydb');
  });

  it('handles empty database', () => {
    expect(computeSessionKey('conn-1', 'sess-1', undefined)).toBe('conn-1::');
    expect(computeSessionKey('conn-1', 'sess-1', '')).toBe('conn-1::');
  });

  it('returns stable key for same inputs', () => {
    const k1 = computeSessionKey('a', 'b', 'c');
    const k2 = computeSessionKey('a', 'b', 'c');
    expect(k1).toBe(k2);
  });
});

describe('loadSessions / saveSessions', () => {
  beforeEach(() => {
    vi.stubGlobal('localStorage', {
      store: {} as Record<string, string>,
      getItem: vi.fn(function (this: { store: Record<string, string> }, key: string) {
        return this.store[key] ?? null;
      }),
      setItem: vi.fn(function (this: { store: Record<string, string> }, key: string, val: string) {
        this.store[key] = val;
      }),
      removeItem: vi.fn(function (this: { store: Record<string, string> }, key: string) {
        delete this.store[key];
      }),
      clear: vi.fn(function (this: { store: Record<string, string> }) {
        this.store = {};
      }),
      get length() {
        return Object.keys(this.store).length;
      },
      key: vi.fn(function (this: { store: Record<string, string> }, i: number) {
        return Object.keys(this.store)[i] ?? null;
      }),
    } as unknown as Storage);
  });

  it('loads empty when nothing stored', () => {
    expect(loadSessions()).toEqual({});
  });

  it('round-trips sessions through save/load', () => {
    const sessions = {
      'conn-1::db': {
        messages: [{ role: 'user' as const, content: 'hello' }],
        lastAccess: 1000,
      },
    };
    saveSessions(sessions);
    const loaded = loadSessions();
    expect(loaded['conn-1::db']).toBeDefined();
    expect(loaded['conn-1::db'].messages).toHaveLength(1);
    expect(loaded['conn-1::db'].messages[0].content).toBe('hello');
  });

  it('truncates to MAX_MESSAGES_PER_SESSION (50)', () => {
    const msgs = Array.from({ length: 60 }, (_, i) => ({
      role: 'user' as const,
      content: `msg-${i}`,
    }));
    const sessions = {
      key: { messages: msgs, lastAccess: 1000 },
    };
    saveSessions(sessions);
    const loaded = loadSessions();
    expect(loaded['key'].messages).toHaveLength(50);
    // Should keep the last 50
    expect(loaded['key'].messages[0].content).toBe('msg-10');
  });

  it('evicts oldest LRU when exceeding MAX_SESSIONS (20)', () => {
    const sessions: Record<
      string,
      { messages: { role: 'user'; content: string }[]; lastAccess: number }
    > = {};
    for (let i = 0; i < 22; i++) {
      sessions[`key-${i}`] = {
        messages: [{ role: 'user', content: `msg-${i}` }],
        lastAccess: i * 100,
      };
    }
    saveSessions(sessions);
    const loaded = loadSessions();
    const keys = Object.keys(loaded);
    expect(keys.length).toBe(20);
    // oldest (0, 1) should be evicted
    expect(loaded['key-0']).toBeUndefined();
    expect(loaded['key-1']).toBeUndefined();
    // newest should survive
    expect(loaded['key-21']).toBeDefined();
  });
});

describe('touchSession', () => {
  it('updates lastAccess timestamp', () => {
    const sessions = {
      k: { messages: [], lastAccess: 100 },
    };
    const updated = touchSession(sessions, 'k');
    expect(updated['k'].lastAccess).toBeGreaterThan(100);
  });
});
