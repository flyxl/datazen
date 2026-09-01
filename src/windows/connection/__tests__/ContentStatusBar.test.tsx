import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ContentStatusBar } from '../ContentStatusBar';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}));

describe('ContentStatusBar', () => {
  it('exposes status semantics and the active database context', () => {
    render(
      <ContentStatusBar
        databaseType="redis"
        connectionName="Redis local"
        currentDatabase="db5"
        tableName=""
        columnCount={0}
        totalRows={0}
      />,
    );

    const status = screen.getByRole('status');
    expect(status).toHaveTextContent('connWin.connected');
    expect(status).toHaveTextContent('Redis local · db5');
    expect(status.querySelector('.bg-success')).not.toBeNull();
  });
});
