import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen, waitFor } from '@testing-library/react';
import { PrivilegeView } from '../PrivilegeView';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const getPrivileges = vi.fn();
const executeQuery = vi.fn();

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

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  getPrivileges.mockResolvedValue([
    { grantee: 'alice', objectSchema: 'public', objectName: 'users', privilege: 'SELECT' },
  ]);
  executeQuery.mockResolvedValue({});
});

describe('PrivilegeView', () => {
  it('renders grants and executes SQL', async () => {
    render(<PrivilegeView connectionId="c1" />);
    await screen.findByText('alice');
    expect(screen.getByText('public.users')).toBeInTheDocument();
    expect(screen.getByText('SELECT')).toBeInTheDocument();

    fireEvent.click(screen.getByText('query.execute'));
    await waitFor(() => {
      expect(executeQuery).toHaveBeenCalledWith(
        'c1',
        expect.stringContaining('GRANT SELECT'),
      );
      expect(screen.getByText('privileges.executeOk')).toBeInTheDocument();
    });

    getPrivileges.mockClear();
    fireEvent.click(document.querySelectorAll('button')[0]);
    await waitFor(() => expect(getPrivileges).toHaveBeenCalled());
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
});
