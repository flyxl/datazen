import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useBindParameters } from '../useBindParameters';

describe('useBindParameters', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.restoreAllMocks();
  });

  it('parses params from SQL', () => {
    const { result } = renderHook(() => useBindParameters('SELECT :uid, $1'));
    expect(result.current.params).toHaveLength(2);
    expect(result.current.params[0].stableId).toBe('named:uid');
    expect(result.current.params[1].stableId).toBe('dollar:1');
  });

  it('produces labels from param syntax', () => {
    const { result } = renderHook(() => useBindParameters('SELECT :uid'));
    expect(result.current.labels).toEqual({ 'named:uid': ':uid' });
  });

  it('setValue updates values keyed by stableId', () => {
    const { result } = renderHook(() => useBindParameters('SELECT :uid'));
    act(() => result.current.setValue('named:uid', '42'));
    expect(result.current.values).toEqual({ 'named:uid': '42' });
  });

  it('reconciles values when SQL changes — keeps surviving params', () => {
    const { result, rerender } = renderHook(({ sql }) => useBindParameters(sql), {
      initialProps: { sql: 'SELECT :uid, :name' },
    });
    act(() => result.current.setValue('named:uid', '7'));
    act(() => result.current.setValue('named:name', 'Alice'));

    // SQL changes to only have :uid — :name should be dropped
    rerender({ sql: 'SELECT :uid' });
    expect(result.current.values).toEqual({ 'named:uid': '7' });
  });

  it('reconciles values when SQL changes — restores old values if param reappears', () => {
    const { result, rerender } = renderHook(({ sql }) => useBindParameters(sql), {
      initialProps: { sql: 'SELECT :uid, :name' },
    });
    act(() => result.current.setValue('named:uid', '7'));
    act(() => result.current.setValue('named:name', 'Alice'));

    // SQL changes to only have :uid
    rerender({ sql: 'SELECT :uid' });
    expect(result.current.values).toEqual({ 'named:uid': '7' });

    // :name reappears — value should be restored to empty (not 'Alice' since it was dropped)
    rerender({ sql: 'SELECT :uid, :name' });
    expect(result.current.values['named:uid']).toBe('7');
  });

  it('buildPayloadForTarget generates v2 payload for a different SQL', () => {
    const { result } = renderHook(() => useBindParameters('SELECT :uid'));
    act(() => result.current.setValue('named:uid', '42'));

    const payload = result.current.buildPayloadForTarget('SELECT :uid WHERE id = :uid');
    expect(payload.version).toBe(2);
    expect(payload.values['named:uid']).toBe(42);
    expect(payload.occurrences).toHaveLength(2);
  });

  it('fingerprint is comma-joined stable IDs', () => {
    const { result } = renderHook(() => useBindParameters('SELECT :uid, $1'));
    expect(result.current.fingerprint).toBe('named:uid,dollar:1');
  });

  it('fingerprint is empty for no params', () => {
    const { result } = renderHook(() => useBindParameters('SELECT 1'));
    expect(result.current.fingerprint).toBe('');
  });
});

describe('useBindParameters history', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('records history on markSubmitted', () => {
    const { result } = renderHook(() => useBindParameters('SELECT :uid'));
    act(() => result.current.setValue('named:uid', '42'));

    act(() => {
      result.current.markSubmitted({ 'named:uid': '42' });
    });

    const history = result.current.getHistory('named:uid');
    expect(history).toHaveLength(1);
    expect(history[0].value).toBe('42');
  });

  it('deduplicates history entries with same value', () => {
    const { result } = renderHook(() => useBindParameters('SELECT :uid'));

    act(() => {
      result.current.markSubmitted({ 'named:uid': '42' });
    });
    act(() => {
      result.current.markSubmitted({ 'named:uid': '42' });
    });

    const history = result.current.getHistory('named:uid');
    expect(history).toHaveLength(1);
  });

  it('limits history to 5 entries', () => {
    const { result } = renderHook(() => useBindParameters('SELECT :uid'));

    for (let i = 0; i < 7; i++) {
      act(() => {
        result.current.markSubmitted({ 'named:uid': String(i) });
      });
    }

    const history = result.current.getHistory('named:uid');
    expect(history).toHaveLength(5);
    // Most recent first
    expect(history[0].value).toBe('6');
  });

  it('does not record empty values', () => {
    const { result } = renderHook(() => useBindParameters('SELECT :uid'));

    act(() => {
      result.current.markSubmitted({ 'named:uid': '' });
    });

    expect(result.current.getHistory('named:uid')).toHaveLength(0);
  });

  it('does not record sensitive param names', () => {
    const { result } = renderHook(() => useBindParameters('SELECT :password'));

    act(() => {
      result.current.markSubmitted({ 'named:password': 'secret123' });
    });

    expect(result.current.getHistory('named:password')).toHaveLength(0);
  });

  it('does not record token/secret/key params', () => {
    const sensitiveSql = 'SELECT :token, :secret, :key, :credential';
    const { result } = renderHook(() => useBindParameters(sensitiveSql));

    act(() => {
      result.current.markSubmitted({
        'named:token': 'abc',
        'named:secret': 'def',
        'named:key': 'ghi',
        'named:credential': 'jkl',
      });
    });

    expect(result.current.getHistory('named:token')).toHaveLength(0);
    expect(result.current.getHistory('named:secret')).toHaveLength(0);
    expect(result.current.getHistory('named:key')).toHaveLength(0);
    expect(result.current.getHistory('named:credential')).toHaveLength(0);
  });

  it('clearHistory removes entries for a specific stableId', () => {
    const { result } = renderHook(() => useBindParameters('SELECT :uid, :name'));

    act(() => {
      result.current.markSubmitted({ 'named:uid': '42', 'named:name': 'Alice' });
    });

    act(() => result.current.clearHistory('named:uid'));

    expect(result.current.getHistory('named:uid')).toHaveLength(0);
    expect(result.current.getHistory('named:name')).toHaveLength(1);
  });

  it('clearAllHistory removes all entries', () => {
    const { result } = renderHook(() => useBindParameters('SELECT :uid, :name'));

    act(() => {
      result.current.markSubmitted({ 'named:uid': '42', 'named:name': 'Alice' });
    });

    act(() => result.current.clearAllHistory());

    expect(result.current.getHistory('named:uid')).toHaveLength(0);
    expect(result.current.getHistory('named:name')).toHaveLength(0);
  });

  it('persists history to localStorage', () => {
    const { result } = renderHook(() => useBindParameters('SELECT :uid'));

    act(() => {
      result.current.markSubmitted({ 'named:uid': '42' });
    });

    const raw = localStorage.getItem('datazen-bind-param-history');
    expect(raw).toBeTruthy();
    const parsed = JSON.parse(raw!);
    expect(parsed['named:uid']).toHaveLength(1);
    expect(parsed['named:uid'][0].value).toBe('42');
  });

  it('loads history from localStorage on mount', () => {
    const existing = {
      'named:uid': [{ value: '99', timestamp: 5000 }],
    };
    localStorage.setItem('datazen-bind-param-history', JSON.stringify(existing));

    const { result } = renderHook(() => useBindParameters('SELECT :uid'));
    expect(result.current.getHistory('named:uid')).toHaveLength(1);
    expect(result.current.getHistory('named:uid')[0].value).toBe('99');
  });

  it('silently degrades when localStorage is unavailable', () => {
    // Mock localStorage.getItem to throw
    const origGetItem = localStorage.getItem;
    vi.spyOn(localStorage, 'getItem').mockImplementation(() => {
      throw new Error('quota exceeded');
    });

    const { result } = renderHook(() => useBindParameters('SELECT :uid'));
    // Should not throw — history is just empty
    expect(result.current.getHistory('named:uid')).toHaveLength(0);

    localStorage.getItem = origGetItem;
  });

  it('silently degrades when localStorage JSON is corrupted', () => {
    localStorage.setItem('datazen-bind-param-history', 'NOT-JSON{{{');

    const { result } = renderHook(() => useBindParameters('SELECT :uid'));
    expect(result.current.getHistory('named:uid')).toHaveLength(0);
  });
});

describe('useBindParameters execution target switch', () => {
  it('keeps values when target SQL still uses same params', () => {
    const { result, rerender } = renderHook(({ sql }) => useBindParameters(sql), {
      initialProps: { sql: 'SELECT :uid FROM t WHERE id = :uid' },
    });
    act(() => result.current.setValue('named:uid', '42'));

    // Target changes but still has :uid
    rerender({ sql: 'SELECT :uid FROM t2 WHERE id = :uid' });
    expect(result.current.values['named:uid']).toBe('42');
  });

  it('handles parameter set change on target switch', () => {
    const { result, rerender } = renderHook(({ sql }) => useBindParameters(sql), {
      initialProps: { sql: 'SELECT :uid' },
    });
    act(() => result.current.setValue('named:uid', '42'));

    // SQL now has :uid and :name
    rerender({ sql: 'SELECT :uid, :name' });
    expect(result.current.values['named:uid']).toBe('42');
    expect(result.current.values['named:name']).toBe('');
  });
});

describe('useBindParameters with dialect policy', () => {
  it('parses @-style params when enableAt is true', () => {
    const { result } = renderHook(() => useBindParameters('SELECT @user', { enableAt: true }));
    expect(result.current.params).toHaveLength(1);
    expect(result.current.params[0].stableId).toBe('named:user');
  });

  it('parses ?-style params when enableQuestion is true', () => {
    const { result } = renderHook(() => useBindParameters('SELECT ?, ?', { enableQuestion: true }));
    expect(result.current.params).toHaveLength(2);
    expect(result.current.params[0].stableId).toBe('question:1');
    expect(result.current.params[1].stableId).toBe('question:2');
  });
});
