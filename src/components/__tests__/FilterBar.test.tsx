import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render } from '@testing-library/react';
import { FilterBar } from '../FilterBar';

vi.mock('../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

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

    expect(container.firstElementChild).toHaveAttribute('aria-busy', 'true');
    fireEvent.click(getByLabelText('filter.remove'));
    fireEvent.click(getByText('filter.clear'));
    expect(onRemove).not.toHaveBeenCalled();
    expect(onClear).not.toHaveBeenCalled();
  });
});
