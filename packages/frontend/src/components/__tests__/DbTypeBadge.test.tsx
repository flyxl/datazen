import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { DbTypeBadge } from '../DbTypeBadge';
import type { IconResolver } from '../../lib/iconResolver';

vi.mock('../../lib/databaseTypes', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../lib/databaseTypes')>();
  return {
    ...actual,
    getDriverIconParents: () => ({ doris: 'mysql', questdb: 'postgresql' }),
    getDbIcon: (dbType: string) => {
      if (dbType === 'doris') return { label: 'Do', bg: 'bg-cyan-700' };
      if (dbType === 'questdb') return { label: 'Qd', bg: 'bg-rose-700' };
      return { label: 'DB', bg: 'bg-slate-600' };
    },
  };
});

function resolver(map: Record<string, string>): IconResolver {
  return {
    resolve(id) {
      const href = map[id];
      if (href) return { kind: 'url', href };
      return { kind: 'placeholder', label: '?', bgClass: 'bg-slate-600' };
    },
  };
}

describe('DbTypeBadge', () => {
  it('renders own brand url without shortLabel overlay', () => {
    const { container } = render(
      <DbTypeBadge
        databaseType="doris"
        resolver={resolver({ 'db.doris': '/doris.svg', 'db.mysql': '/mysql.svg' })}
      />,
    );
    expect(container.querySelector('img')?.getAttribute('src')).toBe('/doris.svg');
    expect(screen.queryByText('Do')).toBeNull();
  });

  it('composites parent icon with shortLabel when own url missing', () => {
    const { container } = render(
      <DbTypeBadge
        databaseType="doris"
        resolver={resolver({ 'db.mysql': '/mysql.svg' })}
      />,
    );
    expect(container.querySelector('img')?.getAttribute('src')).toBe('/mysql.svg');
    expect(screen.getByText('Do')).toBeTruthy();
  });

  it('falls back to placeholder when neither own nor parent url exists', () => {
    render(<DbTypeBadge databaseType="unknown" resolver={resolver({})} />);
    expect(screen.getByText('?')).toBeTruthy();
  });
});
