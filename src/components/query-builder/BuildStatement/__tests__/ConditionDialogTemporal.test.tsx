/**
 * ConditionDialog temporal value behaviour: temporal columns get the themed
 * in-app picker (never the native `datetime-local` widget), an empty or
 * malformed literal blocks OK, and a valid datetime is normalised to the
 * space-separated SQL form before it reaches the store.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { ConditionDialog } from '../ConditionDialog';
import type { ConditionDraft } from '../ConditionDialog';

vi.mock('../../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

// Render the dialog inline; the shared Dialog portals to document.body.
vi.mock('../../../ui/Dialog', () => ({
  Dialog: ({ open, children, footer }: Record<string, unknown> & { children: React.ReactNode }) =>
    open ? (
      <div data-testid="dialog-shell">
        {children}
        {footer}
      </div>
    ) : null,
}));

const draft: ConditionDraft = {
  id: null,
  groupId: 'root',
  isFirstInGroup: true,
  condition: {
    id: 'c1',
    table: 'er_orders',
    column: 'ordered_at',
    operator: '>',
    value: '',
    conjunction: 'AND',
  },
};

function renderDialog(onApply = vi.fn()) {
  render(
    <ConditionDialog
      draft={draft}
      allowAggregate={false}
      allTables={['er_orders']}
      allColumns={{ er_orders: ['ordered_at', 'id'] }}
      allColumnTypes={{ er_orders: { ordered_at: 'timestamp without time zone', id: 'integer' } }}
      tableAliases={{}}
      onApply={onApply}
      onRemove={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  return onApply;
}

afterEach(cleanup);

describe('ConditionDialog temporal value control', () => {
  it('renders the themed picker (calendar toggle) for a timestamp column', () => {
    renderDialog();
    expect(screen.getByTestId('qb-cond-value-toggle')).toBeTruthy();
    expect(screen.queryByTestId('qb-cond-value-popover')).toBeNull();
  });

  it('blocks OK while the temporal value is empty, without an error message', () => {
    renderDialog();
    expect((screen.getByTestId('qb-cond-apply') as HTMLButtonElement).disabled).toBe(true);
    expect(screen.queryByTestId('qb-cond-value-error')).toBeNull();
  });

  it('blocks OK and shows the error while the literal is invalid', () => {
    renderDialog();
    const input = screen.getByTestId('qb-cond-value-text') as HTMLInputElement;
    fireEvent.change(input, { target: { value: '2026-13-45' } });
    expect(screen.getByTestId('qb-cond-value-error')).toBeTruthy();
    expect((screen.getByTestId('qb-cond-apply') as HTMLButtonElement).disabled).toBe(true);
  });

  it('accepts a valid literal, normalises the T separator, and applies', () => {
    const onApply = renderDialog();
    const input = screen.getByTestId('qb-cond-value-text') as HTMLInputElement;
    fireEvent.change(input, { target: { value: '2026-09-20T01:13' } });
    const apply = screen.getByTestId('qb-cond-apply') as HTMLButtonElement;
    expect(apply.disabled).toBe(false);
    fireEvent.click(apply);
    expect(onApply).toHaveBeenCalledTimes(1);
    expect(onApply.mock.calls[0]![0].condition.value).toBe('2026-09-20 01:13');
  });

  it('keeps a plain text input without a toggle for non-temporal columns', () => {
    render(
      <ConditionDialog
        draft={{ ...draft, condition: { ...draft.condition, column: 'id' } }}
        allowAggregate={false}
        allTables={['er_orders']}
        allColumns={{ er_orders: ['ordered_at', 'id'] }}
        allColumnTypes={{ er_orders: { id: 'integer' } }}
        tableAliases={{}}
        onApply={vi.fn()}
        onRemove={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.queryByTestId('qb-cond-value-toggle')).toBeNull();
    expect((screen.getByTestId('qb-cond-value') as HTMLInputElement).type).toBe('text');
  });
});
