/**
 * The date/datetime picker dialog of the "Set Value" menu (DBR-05) — the
 * dialog only *chooses* the value: apply hands the composed canonical string
 * to the caller, cancel writes nothing.
 */
import { describe, expect, it, vi, afterEach } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { SetValueDateDialog } from '../SetValueDateDialog';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

afterEach(cleanup);

describe('SetValueDateDialog (DBR-05)', () => {
  it('applies the edited draft through onApply and closes', () => {
    const onApply = vi.fn();
    const onClose = vi.fn();
    render(
      <SetValueDateDialog
        open
        columnName="created_at"
        columnType="date"
        kind="date"
        value="2026-01-01"
        onApply={onApply}
        onClose={onClose}
      />,
    );

    const picker = screen.getByTestId('set-value-date-picker');
    const input = picker.querySelector('input') as HTMLInputElement;
    expect(input).not.toBeNull();
    expect(input.value).toBe('2026-01-01');

    fireEvent.change(input, { target: { value: '2026-03-08' } });
    fireEvent.click(screen.getByTestId('set-value-date-apply'));

    expect(onApply).toHaveBeenCalledTimes(1);
    expect(onApply).toHaveBeenCalledWith('2026-03-08');
    expect(onClose).toHaveBeenCalled();
  });

  it('cancels without applying', () => {
    const onApply = vi.fn();
    render(
      <SetValueDateDialog
        open
        columnName="created_at"
        kind="date"
        value="2026-01-01"
        onApply={onApply}
        onClose={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'common.cancel' }));
    expect(onApply).not.toHaveBeenCalled();
  });

  it('names the column and shows the picker intent', () => {
    render(
      <SetValueDateDialog
        open
        columnName="seen_on"
        columnType="timestamp"
        kind="datetime"
        value=""
        onApply={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    const dialog = screen.getByRole('dialog');
    expect(dialog).toHaveTextContent('seen_on');
    expect(dialog).toHaveTextContent('dataTable.pickDate');
  });
});
