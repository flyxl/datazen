import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup, screen } from '@testing-library/react';
import { ConfirmDialog } from '../ConfirmDialog';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => {
      const map: Record<string, string> = {
        'common.cancel': 'Cancel',
        'common.confirm': 'Confirm',
      };
      return map[key] ?? key;
    },
  }),
}));

afterEach(cleanup);

describe('ConfirmDialog', () => {
  it('renders title and message when open', () => {
    render(
      <ConfirmDialog
        open
        title="Delete Item"
        message="Are you sure?"
        onConfirm={() => {}}
        onCancel={() => {}}
      />,
    );
    expect(screen.getByText('Delete Item')).toBeTruthy();
    expect(screen.getByText('Are you sure?')).toBeTruthy();
  });

  it('does not render when closed', () => {
    const { container } = render(
      <ConfirmDialog
        open={false}
        title="Delete Item"
        message="Are you sure?"
        onConfirm={() => {}}
        onCancel={() => {}}
      />,
    );
    expect(container.innerHTML).toBe('');
  });

  it('calls onConfirm when confirm button is clicked', () => {
    const onConfirm = vi.fn();
    render(
      <ConfirmDialog
        open
        title="Delete"
        message="Sure?"
        onConfirm={onConfirm}
        onCancel={() => {}}
      />,
    );
    fireEvent.click(screen.getByTestId('confirm-dialog-ok'));
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it('calls onCancel when cancel button is clicked', () => {
    const onCancel = vi.fn();
    render(
      <ConfirmDialog
        open
        title="Delete"
        message="Sure?"
        onConfirm={() => {}}
        onCancel={onCancel}
      />,
    );
    fireEvent.click(screen.getByText('Cancel'));
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it('uses custom labels when provided', () => {
    render(
      <ConfirmDialog
        open
        title="Delete"
        message="Sure?"
        confirmLabel="Yes, delete"
        cancelLabel="No, keep it"
        onConfirm={() => {}}
        onCancel={() => {}}
      />,
    );
    expect(screen.getByText('Yes, delete')).toBeTruthy();
    expect(screen.getByText('No, keep it')).toBeTruthy();
  });

  it('shows warning icon for warning kind', () => {
    render(
      <ConfirmDialog
        open
        title="Delete"
        message="Sure?"
        kind="warning"
        onConfirm={() => {}}
        onCancel={() => {}}
      />,
    );
    // Dialog renders via portal to document.body
    const svg = document.body.querySelector('svg');
    expect(svg).toBeTruthy();
  });
});
