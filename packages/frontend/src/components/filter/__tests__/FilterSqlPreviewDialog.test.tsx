import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, screen, cleanup } from '@testing-library/react';
import { FilterSqlPreviewDialog } from '../FilterSqlPreviewDialog';
import type { TableReadPreviewSql } from '@datazen/backend-client';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}));

const preview: TableReadPreviewSql = {
  dataSql: 'SELECT "id", "name" FROM "users" WHERE "status" = \'active\' LIMIT 50 OFFSET 0;',
  countSql: 'SELECT COUNT(*) FROM "users" WHERE "status" = \'active\';',
  exactCountSql: 'SELECT COUNT(*) FROM "users" WHERE "status" = \'active\';',
  countKind: 'exact',
};

afterEach(cleanup);

describe('FilterSqlPreviewDialog (DB-15)', () => {
  it('shows the data statement, the count statement and the exact count', () => {
    render(<FilterSqlPreviewDialog open onClose={vi.fn()} preview={preview} />);

    expect(screen.getByTestId('filter-sql-preview-data')).toHaveTextContent(
      'SELECT "id", "name" FROM "users" WHERE "status" = \'active\' LIMIT 50 OFFSET 0;',
    );
    expect(screen.getByTestId('filter-sql-preview-count')).toHaveTextContent(
      'SELECT COUNT(*) FROM "users"',
    );
    // The count row names its kind: an estimate and an exact count read differently.
    expect(
      screen.getByText('filter.previewSqlCount · filter.previewSqlKind.exact'),
    ).toBeInTheDocument();
    expect(screen.getByTestId('filter-sql-preview-exact-count')).toBeInTheDocument();
    expect(screen.queryByTestId('filter-sql-preview-error')).toBeNull();
  });

  it('hides the count statement when the driver would not run one', () => {
    render(
      <FilterSqlPreviewDialog
        open
        onClose={vi.fn()}
        preview={{ ...preview, countSql: null, countKind: 'unsupported' }}
      />,
    );

    expect(screen.queryByTestId('filter-sql-preview-count')).toBeNull();
    // The exact count stays: the rescue statement is a fact even without a plan.
    expect(screen.getByTestId('filter-sql-preview-exact-count')).toBeInTheDocument();
  });

  it('copies every statement, each labelled, in one write', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(window.navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });
    render(<FilterSqlPreviewDialog open onClose={vi.fn()} preview={preview} />);

    fireEvent.click(screen.getByTestId('filter-sql-preview-copy'));

    await vi.waitFor(() => expect(writeText).toHaveBeenCalledOnce());
    const text = writeText.mock.calls[0][0] as string;
    expect(text).toContain(preview.dataSql);
    expect(text).toContain(preview.countSql!);
    expect(text).toContain('-- filter.previewSqlData');
    expect(text).toContain('-- filter.previewSqlCount');
  });

  it('shows the error and no statements when the preview failed', () => {
    render(
      <FilterSqlPreviewDialog open onClose={vi.fn()} preview={null} error="connection refused" />,
    );

    expect(screen.getByTestId('filter-sql-preview-error')).toHaveTextContent('connection refused');
    expect(screen.queryByTestId('filter-sql-preview-data')).toBeNull();
  });

  it('shows the loading placeholder while the statements are in flight', () => {
    render(<FilterSqlPreviewDialog open onClose={vi.fn()} preview={null} error={null} />);

    expect(screen.getByTestId('filter-sql-preview-loading')).toBeInTheDocument();
  });
});
