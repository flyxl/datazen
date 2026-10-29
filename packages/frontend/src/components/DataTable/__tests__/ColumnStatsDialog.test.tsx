/**
 * `ColumnStatsDialog` (DB-12): the three states of the stats request —
 * loading, error, ready — plus the "no average for this type" reading.
 *
 * The dialog owns no fetching (that is `useColumnStats`), so the tests hand
 * it states directly and lock what each one renders.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, screen } from '@testing-library/react';
import { render } from '@testing-library/react';
import type { ColumnStatsState } from '../../../hooks/useColumnStats';
import { ColumnStatsDialog } from '../ColumnStatsDialog';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

afterEach(cleanup);

function stateOf(partial: Partial<ColumnStatsState>): ColumnStatsState {
  return {
    status: 'ready',
    column: 'price',
    stats: null,
    error: null,
    ...partial,
  } as ColumnStatsState;
}

const NUMERIC_STATS = {
  rowCount: 1_000_000,
  nonNullCount: 999_994,
  nullCount: 6,
  distinctCount: 7,
  min: '0.5',
  max: '99.5',
  avg: 19.5,
  supportsAvg: true,
};

describe('ColumnStatsDialog (DB-12)', () => {
  it('renders nothing while idle', () => {
    const { container } = render(
      <ColumnStatsDialog state={stateOf({ status: 'idle' })} onClose={() => {}} />,
    );
    expect(container.innerHTML).toBe('');
  });

  it('shows the loading copy and the column while computing', () => {
    render(
      <ColumnStatsDialog state={stateOf({ status: 'loading', stats: null })} onClose={() => {}} />,
    );
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByText('columnStats.loading')).toBeInTheDocument();
  });

  it('renders the six aggregates with formatted counts', () => {
    render(<ColumnStatsDialog state={stateOf({ stats: NUMERIC_STATS })} onClose={() => {}} />);
    // The title names the column the numbers belong to.
    expect(screen.getByText('columnStats.title — price')).toBeInTheDocument();
    expect(screen.getByTestId('column-stats-rows').textContent).toContain('1,000,000');
    expect(screen.getByTestId('column-stats-nulls').textContent).toContain('6');
    expect(screen.getByTestId('column-stats-min').textContent).toContain('0.5');
    expect(screen.getByTestId('column-stats-max').textContent).toContain('99.5');
    expect(screen.getByTestId('column-stats-avg').textContent).toContain('19.5');
  });

  it('omits the average row for a column type that cannot be averaged', () => {
    render(
      <ColumnStatsDialog
        state={stateOf({
          stats: { ...NUMERIC_STATS, avg: null, supportsAvg: false },
        })}
        onClose={() => {}}
      />,
    );
    expect(screen.queryByTestId('column-stats-avg')).toBeNull();
    expect(screen.getByText('columnStats.avgUnsupported')).toBeInTheDocument();
  });

  it('a value the server did not report reads as an em dash, not a zero', () => {
    render(
      <ColumnStatsDialog
        state={stateOf({
          stats: {
            rowCount: 10,
            nonNullCount: 10,
            nullCount: 0,
            distinctCount: null,
            min: null,
            max: null,
            avg: null,
            supportsAvg: true,
          },
        })}
        onClose={() => {}}
      />,
    );
    expect(screen.getByTestId('column-stats-distinct').textContent).toContain('—');
    expect(screen.getByTestId('column-stats-min').textContent).toContain('—');
  });

  it('shows the error message of a failed aggregate', () => {
    render(
      <ColumnStatsDialog
        state={stateOf({ status: 'error', error: 'function avg(text) does not exist' })}
        onClose={() => {}}
      />,
    );
    expect(screen.getByText(/avg\(text\)/)).toBeInTheDocument();
  });

  it('the close button hands the dismissal back to the host', () => {
    const onClose = vi.fn();
    render(<ColumnStatsDialog state={stateOf({ stats: NUMERIC_STATS })} onClose={onClose} />);
    fireEvent.click(screen.getByRole('button', { name: 'common.close' }));
    expect(onClose).toHaveBeenCalledOnce();
  });
});
