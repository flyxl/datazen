/**
 * TemporalValueInput: free typing, themed popover calendar, and canonical
 * ISO emission (`YYYY-MM-DD`, `HH:MM:SS`, `YYYY-MM-DDTHH:MM:SS`) matching the
 * data grid's display format.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { TemporalValueInput } from '../TemporalValueInput';

afterEach(cleanup);

function renderInput(
  props: Partial<Parameters<typeof TemporalValueInput>[0]> = {},
  onChange = vi.fn(),
) {
  render(
    <TemporalValueInput
      kind="datetime"
      value=""
      onChange={onChange}
      placeholder="Value"
      data-testid="tvi"
      {...props}
    />,
  );
  return onChange;
}

describe('TemporalValueInput', () => {
  it('propagates free typing from the text field unchanged', () => {
    const onChange = renderInput();
    fireEvent.change(screen.getByTestId('tvi-text'), { target: { value: '2026-09-' } });
    expect(onChange).toHaveBeenCalledWith('2026-09-');
  });

  it('opens the popover from the toggle and closes it on outside pointer down', () => {
    renderInput();
    fireEvent.click(screen.getByTestId('tvi-toggle'));
    const popover = screen.getByTestId('tvi-popover');
    expect(popover).toBeTruthy();
    // Regression: the dialog body scrolls — an in-flow popover is clipped by
    // it and drags scrollbars along. The popover must live on document.body.
    expect(popover.parentElement).toBe(document.body);
    fireEvent.pointerDown(document.body);
    expect(screen.queryByTestId('tvi-popover')).toBeNull();
  });

  it('picking a day keeps the typed time and emits ISO with the T separator', () => {
    const onChange = renderInput({ value: '2026-09-20T12:30:00' });
    fireEvent.click(screen.getByTestId('tvi-toggle'));
    fireEvent.click(screen.getByTestId('qb-cal-day-5'));
    expect(onChange).toHaveBeenCalledWith('2026-09-05T12:30:00');
  });

  it('wraps to December of the previous year when paging before January', () => {
    const onChange = renderInput({ kind: 'date', value: '2026-01-15' });
    fireEvent.click(screen.getByTestId('tvi-toggle'));
    fireEvent.click(screen.getByTestId('qb-cal-prev'));
    fireEvent.click(screen.getByTestId('qb-cal-day-31'));
    expect(onChange).toHaveBeenCalledWith('2025-12-31');
  });

  it('emits date-only form for the date kind', () => {
    const onChange = renderInput({ kind: 'date', value: '2026-09-20' });
    fireEvent.click(screen.getByTestId('tvi-toggle'));
    fireEvent.click(screen.getByTestId('qb-cal-day-8'));
    expect(onChange).toHaveBeenCalledWith('2026-09-08');
  });

  it('editing a time segment preserves the other parts', () => {
    const onChange = renderInput({ value: '2026-09-20T12:30:00' });
    fireEvent.click(screen.getByTestId('tvi-toggle'));
    fireEvent.change(screen.getByTestId('tvi-h'), { target: { value: '08' } });
    expect(onChange).toHaveBeenCalledWith('2026-09-20T08:30:00');
  });

  it('emits time-only form for the time kind', () => {
    const onChange = renderInput({ kind: 'time', value: '12:30:00' });
    fireEvent.click(screen.getByTestId('tvi-toggle'));
    expect(screen.queryByTestId('qb-cal-day-1')).toBeNull();
    fireEvent.change(screen.getByTestId('tvi-mi'), { target: { value: '45' } });
    expect(onChange).toHaveBeenCalledWith('12:45:00');
  });

  it('defaults the date to today when a datetime time is typed first', () => {
    const onChange = renderInput({ value: '' });
    fireEvent.click(screen.getByTestId('tvi-toggle'));
    fireEvent.change(screen.getByTestId('tvi-h'), { target: { value: '7' } });
    const today = new Date();
    const ymd = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, '0')}-${String(
      today.getDate(),
    ).padStart(2, '0')}`;
    expect(onChange).toHaveBeenCalledWith(`${ymd}T07:00:00`);
  });
});
