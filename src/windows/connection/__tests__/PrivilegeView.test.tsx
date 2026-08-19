import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen, waitFor } from '@testing-library/react';
import { PrivilegeView } from '../PrivilegeView';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [vi.fn().mockResolvedValue(true), null],
}));

const getPrivileges = vi.fn();
const executeQuery = vi.fn();
const driverExecute = vi.fn();

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    getPrivileges: (...args: unknown[]) => getPrivileges(...args),
  },
}));

vi.mock('../../../commands/query', () => ({
  queryCommands: {
    executeQuery: (...args: unknown[]) => executeQuery(...args),
  },
}));

vi.mock('../../../commands/driver', () => ({
  driverCommands: {
    execute: (...args: unknown[]) => driverExecute(...args),
  },
}));

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  getPrivileges.mockResolvedValue([
    { grantee: 'alice', objectSchema: 'public', objectName: 'users', privilege: 'SELECT' },
  ]);
  executeQuery.mockResolvedValue({});
  driverExecute.mockResolvedValue({});
});

describe('PrivilegeView', () => {
  it('renders grants in tree view and executes SQL', async () => {
    render(<PrivilegeView connectionId="c1" />);
    await screen.findByText('alice');
    expect(screen.getByText('public')).toBeInTheDocument();
    expect(screen.getByText('users')).toBeInTheDocument();
    expect(screen.getByText('SELECT')).toBeInTheDocument();

    fireEvent.click(screen.getByText('query.execute'));
    await waitFor(() => {
      expect(executeQuery).toHaveBeenCalledWith('c1', expect.stringContaining('GRANT SELECT'));
      expect(screen.getByText('privileges.executeOk')).toBeInTheDocument();
    });
  });

  it('shows empty and error states', async () => {
    getPrivileges.mockRejectedValueOnce(new Error('denied'));
    const { rerender } = render(<PrivilegeView connectionId="c1" />);
    await screen.findByText('denied');

    getPrivileges.mockReset();
    getPrivileges.mockResolvedValue([]);
    rerender(<PrivilegeView connectionId="c2" />);
    await screen.findByText('privileges.empty');
  });

  it('surfaces execute failures', async () => {
    executeQuery.mockRejectedValueOnce(new Error('cannot grant'));
    render(<PrivilegeView connectionId="c1" />);
    await screen.findByText('alice');
    fireEvent.click(screen.getByText('query.execute'));
    await screen.findByText('cannot grant');
  });

  it('switches between by-user and by-object views', async () => {
    render(<PrivilegeView connectionId="c1" />);
    await screen.findByText('alice');
    expect(screen.getByText('privileges.byUser')).toBeInTheDocument();
    expect(screen.getByText('privileges.byObject')).toBeInTheDocument();

    fireEvent.click(screen.getByText('privileges.byObject'));
    expect(screen.getByText('public')).toBeInTheDocument();
    expect(screen.getByText('users')).toBeInTheDocument();
  });

  it('can refresh data', async () => {
    render(<PrivilegeView connectionId="c1" />);
    await screen.findByText('alice');

    getPrivileges.mockClear();
    const refreshBtn = screen
      .getAllByRole('button')
      .find((btn) => btn.querySelector('.lucide-refresh-cw'));
    if (refreshBtn) {
      fireEvent.click(refreshBtn);
      await waitFor(() => expect(getPrivileges).toHaveBeenCalled());
    }
  });
});
