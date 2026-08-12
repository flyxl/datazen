import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, fireEvent, screen, waitFor } from '@testing-library/react';
import { AddToDashboardDialog } from '../AddToDashboardDialog';

const listDashboardsMock = vi.fn();

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../commands/dashboard', () => ({
  dashboardCommands: {
    listDashboards: (...args: unknown[]) => listDashboardsMock(...args),
  },
}));

beforeEach(() => {
  vi.clearAllMocks();
  listDashboardsMock.mockResolvedValue([
    {
      id: 'd1',
      name: 'Board A',
      createdAt: '',
      updatedAt: '',
      layout: { cols: 12, rowHeight: 80 },
      widgets: [],
      enabled: true,
    },
  ]);
});

afterEach(cleanup);

describe('AddToDashboardDialog', () => {
  it('renders nothing when closed', () => {
    const { container } = render(
      <AddToDashboardDialog open={false} onClose={vi.fn()} onConfirm={vi.fn()} />,
    );
    expect(container.innerHTML).toBe('');
  });

  it('lists dashboards and confirms selection', async () => {
    const onConfirm = vi.fn();
    render(<AddToDashboardDialog open onClose={vi.fn()} onConfirm={onConfirm} />);

    expect(await screen.findByTestId('add-to-dashboard-dialog')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText('Board A')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('add-to-dashboard-confirm'));
    await waitFor(() => expect(onConfirm).toHaveBeenCalledWith('d1'));
  });

  it('confirms new panel with name', async () => {
    const onConfirm = vi.fn();
    render(<AddToDashboardDialog open onClose={vi.fn()} onConfirm={onConfirm} />);

    await screen.findByTestId('add-to-dashboard-dialog');
    await waitFor(() => expect(screen.getByTestId('dashboard-target-new')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('dashboard-target-new'));
    fireEvent.change(screen.getByTestId('dashboard-new-panel-name'), {
      target: { value: 'My Panel' },
    });
    fireEvent.click(screen.getByTestId('add-to-dashboard-confirm'));
    await waitFor(() => expect(onConfirm).toHaveBeenCalledWith('new', 'My Panel'));
  });
});
