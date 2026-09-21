import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import React from 'react';
import { ConsoleResultView, inferResultType } from '../console/consoleResultRenderer';
import type { ConsoleResultItem } from '../console/consoleResultRenderer';

// Components take `useI18n` from the single @datazen/ui runtime; keep the
// assertions locale-independent by overriding only that hook.
vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  useI18n: () => ({
    t: (key: string, params?: Record<string, string>) => {
      const map: Record<string, string> = {
        'redis.common.cancel': 'Cancel',
        'redis.common.confirm': 'Confirm',
      };
      return map[key] ?? key;
    },
  }),
  useLocaleDomains: () => ({ t: (k: string) => k }),
}));

function makeItem(overrides: Partial<ConsoleResultItem> = {}): ConsoleResultItem {
  return {
    command: 'GET foo',
    ok: true,
    resultType: 'scalar',
    ...overrides,
  };
}

describe('ConsoleResultView', () => {
  it('renders error result', () => {
    render(
      <ConsoleResultView
        item={makeItem({
          ok: false,
          error: 'ERR wrong number of arguments',
          resultType: 'error',
        })}
      />,
    );
    expect(screen.getByText('ERR wrong number of arguments')).toBeTruthy();
  });

  it('renders nil result', () => {
    render(<ConsoleResultView item={makeItem({ value: '(nil)', resultType: 'nil' })} />);
    expect(screen.getByText('(nil)')).toBeTruthy();
  });

  it('renders ok result', () => {
    render(<ConsoleResultView item={makeItem({ value: 'OK', resultType: 'ok' })} />);
    expect(screen.getByText('OK')).toBeTruthy();
  });

  it('renders scalar result', () => {
    render(<ConsoleResultView item={makeItem({ value: '42', resultType: 'scalar' })} />);
    expect(screen.getByText('42')).toBeTruthy();
  });

  it('renders array result', () => {
    render(<ConsoleResultView item={makeItem({ value: '[a, b, c]', resultType: 'array' })} />);
    expect(screen.getByText('a')).toBeTruthy();
    expect(screen.getByText('b')).toBeTruthy();
    expect(screen.getByText('c')).toBeTruthy();
  });

  it('renders empty array', () => {
    render(<ConsoleResultView item={makeItem({ value: '[]', resultType: 'array' })} />);
    expect(screen.getByText('(empty array)')).toBeTruthy();
  });

  it('renders map result', () => {
    render(
      <ConsoleResultView
        item={makeItem({ value: '{name => John, age => 30}', resultType: 'map' })}
      />,
    );
    expect(screen.getByText('name')).toBeTruthy();
    expect(screen.getByText('John')).toBeTruthy();
  });
});

describe('inferResultType', () => {
  it('infers nil', () => expect(inferResultType('(nil)')).toBe('nil'));
  it('infers ok', () => expect(inferResultType('OK')).toBe('ok'));
  it('infers array', () => expect(inferResultType('[1, 2, 3]')).toBe('array'));
  it('infers map', () => expect(inferResultType('{a => 1}')).toBe('map'));
  it('infers scalar', () => expect(inferResultType('hello')).toBe('scalar'));
  it('returns ok for undefined', () => expect(inferResultType(undefined)).toBe('ok'));
});
