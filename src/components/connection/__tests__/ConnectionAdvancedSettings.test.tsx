import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { ConnectionAdvancedSettings } from '../ConnectionAdvancedSettings';
import type { ConnectionFormState } from '../useConnectionForm';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}));

afterEach(cleanup);

function createMockForm(overrides: Partial<ConnectionFormState> = {}): ConnectionFormState {
  return {
    showAdvanced: true,
    setShowAdvanced: vi.fn(),
    readOnly: false,
    setReadOnly: vi.fn(),
    driverReadOnly: false,
    supportsSSL: false,
    sslMode: 'prefer',
    sslOptions: [],
    setSslMode: vi.fn(),
    colorTag: '#3b82f6',
    setColorTag: vi.fn(),
    group: '',
    setGroup: vi.fn(),
    ...overrides,
  } as unknown as ConnectionFormState;
}

describe('ConnectionAdvancedSettings', () => {
  it('allows toggling read-only checkbox when driver is not read-only', () => {
    const setReadOnly = vi.fn();
    const form = createMockForm({ readOnly: false, driverReadOnly: false, setReadOnly });

    render(<ConnectionAdvancedSettings form={form} />);

    const checkbox = screen.getByRole('checkbox', { name: /newConn\.readOnly/i });
    expect(checkbox).not.toBeDisabled();
    expect(checkbox).not.toBeChecked();

    fireEvent.click(checkbox);
    expect(setReadOnly).toHaveBeenCalledWith(true);
  });

  it('disables read-only checkbox and shows locked hint when driver is read-only', () => {
    const setReadOnly = vi.fn();
    const form = createMockForm({ readOnly: true, driverReadOnly: true, setReadOnly });

    render(<ConnectionAdvancedSettings form={form} />);

    const checkbox = screen.getByRole('checkbox', { name: /newConn\.readOnly/i });
    expect(checkbox).toBeDisabled();
    expect(checkbox).toBeChecked();
    expect(screen.getByText('newConn.driverReadOnlyLocked')).toBeInTheDocument();

    fireEvent.click(checkbox);
    expect(setReadOnly).not.toHaveBeenCalled();
  });
});
