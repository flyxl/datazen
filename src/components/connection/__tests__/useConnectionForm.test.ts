import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { normalizeRedisDatabaseField, DB_REGISTRY } from '../../../lib/databaseTypes';
import { useConnectionForm } from '../useConnectionForm';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}));

vi.mock('../../../stores/connectionStore', () => ({
  useConnectionStore: Object.assign(
    vi.fn((selector: (s: { saveConnection: ReturnType<typeof vi.fn> }) => unknown) =>
      selector({ saveConnection: vi.fn() }),
    ),
    {
      getState: () => ({
        testConnection: vi.fn(),
        saveConnection: vi.fn(),
      }),
    },
  ),
}));

describe('normalizeRedisDatabaseField', () => {
  it('returns 0 for empty or invalid input', () => {
    expect(normalizeRedisDatabaseField('')).toBe('0');
    expect(normalizeRedisDatabaseField('  ')).toBe('0');
    expect(normalizeRedisDatabaseField('abc')).toBe('0');
  });

  it('clamps to 0-15', () => {
    expect(normalizeRedisDatabaseField('0')).toBe('0');
    expect(normalizeRedisDatabaseField('15')).toBe('15');
    expect(normalizeRedisDatabaseField('16')).toBe('15');
    expect(normalizeRedisDatabaseField('-1')).toBe('0');
  });
});

describe('useConnectionForm', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('detects formVariant from DB_REGISTRY', () => {
    const { result, rerender } = renderHook(() => useConnectionForm());

    expect(result.current.formVariant).toBe('standard');

    act(() => result.current.handleDatabaseTypeChange('redis'));
    rerender();
    expect(result.current.formVariant).toBe('index');
    expect(DB_REGISTRY.redis.connectionForm).toBe('index');

    act(() => result.current.handleDatabaseTypeChange('sqlite'));
    rerender();
    expect(result.current.formVariant).toBe('file');
    expect(DB_REGISTRY.sqlite.connectionForm).toBe('file');

    act(() => result.current.handleDatabaseTypeChange('kiwi'));
    rerender();
    expect(result.current.formVariant).toBe('kiwi');
    expect(DB_REGISTRY.kiwi.connectionForm).toBe('kiwi');
  });

  it('includes username in kiwi draft', () => {
    const { result } = renderHook(() => useConnectionForm());

    act(() => {
      result.current.handleDatabaseTypeChange('kiwi');
      result.current.setHost('https://kiwi.example.com');
      result.current.setUsername('kiwi-user');
      result.current.setPassword('secret');
      result.current.setDatabase('instance.example.com');
    });

    expect(result.current.draft.databaseType).toBe('kiwi');
    expect(result.current.draft.username).toBe('kiwi-user');
    expect(result.current.hasUsername).toBe(true);
  });
});
