import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render } from '@testing-library/react';
import { FilterBar } from '../FilterBar';

vi.mock('../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

afterEach(cleanup);

describe('FilterBar', () => {
  it('exposes loading state and prevents remove/clear mutations while busy', () => {
    const onRemove = vi.fn();
    const onClear = vi.fn();
    const { container, getByLabelText, getByText } = render(
      <FilterBar
        filters={[{ column: 'status', operator: 'eq', value: 'paid' }]}
        onRemove={onRemove}
        onClear={onClear}
        loading
      />,
    );

    const removeButton = getByLabelText('filter.remove');
    const clearButton = getByText('filter.clear');

    expect(container.firstElementChild).toHaveAttribute('aria-busy', 'true');
    expect(container.firstElementChild).toHaveAttribute('aria-disabled', 'true');
    expect(removeButton).toBeDisabled();
    expect(removeButton).toHaveAttribute('aria-disabled', 'true');
    expect(clearButton).toBeDisabled();
    expect(clearButton).toHaveAttribute('aria-disabled', 'true');
    fireEvent.click(removeButton);
    fireEvent.click(clearButton);
    expect(onRemove).not.toHaveBeenCalled();
    expect(onClear).not.toHaveBeenCalled();
  });

  it('keeps filter mutations available when not loading', () => {
    const onRemove = vi.fn();
    const onClear = vi.fn();
    const { container, getByLabelText, getByText } = render(
      <FilterBar
        filters={[{ column: 'status', operator: 'eq', value: 'paid' }]}
        onRemove={onRemove}
        onClear={onClear}
      />,
    );

    const removeButton = getByLabelText('filter.remove');
    const clearButton = getByText('filter.clear');
    expect(container.firstElementChild).toHaveAttribute('aria-busy', 'false');
    expect(container.firstElementChild).not.toHaveAttribute('aria-disabled');
    expect(removeButton).not.toBeDisabled();
    expect(clearButton).not.toBeDisabled();

    fireEvent.click(removeButton);
    fireEvent.click(clearButton);
    expect(onRemove).toHaveBeenCalledWith(0);
    expect(onClear).toHaveBeenCalledTimes(1);
  });
});
