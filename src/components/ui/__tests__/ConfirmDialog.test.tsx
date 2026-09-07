import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup, screen } from '@testing-library/react';
import { ConfirmDialog } from '../ConfirmDialog';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => {
      const map: Record<string, string> = {
        'common.cancel': 'Cancel',
        'common.confirm': 'Confirm',
        'query.editor.executionConfirm.previewTruncated': 'Showing first {lines} lines',
        'query.editor.executionConfirm.copySql': 'Copy SQL',
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

  it('renders badge when provided', () => {
    render(
      <ConfirmDialog
        open
        title="Confirm"
        message="Are you sure?"
        badge="Production"
        onConfirm={() => {}}
        onCancel={() => {}}
      />,
    );
    expect(screen.getByText('Production')).toBeTruthy();
  });

  it('renders description when provided', () => {
    render(
      <ConfirmDialog
        open
        title="Confirm"
        message="Are you sure?"
        description="This is a longer description with more details."
        onConfirm={() => {}}
        onCancel={() => {}}
      />,
    );
    expect(screen.getByText('This is a longer description with more details.')).toBeTruthy();
  });

  it('renders code preview section when codePreview is provided', () => {
    render(
      <ConfirmDialog
        open
        title="Confirm"
        message="Review SQL"
        codePreview="SELECT * FROM users WHERE id = 1"
        onConfirm={() => {}}
        onCancel={() => {}}
      />,
    );
    expect(screen.getByText('SELECT * FROM users WHERE id = 1')).toBeTruthy();
    expect(screen.getByTestId('confirm-dialog-copy-sql')).toBeTruthy();
  });

  it('truncates code preview to max lines', () => {
    const longSql = Array.from({ length: 20 }, (_, i) => `SELECT ${i} FROM t`).join('\n');
    render(
      <ConfirmDialog
        open
        title="Confirm"
        message="Review SQL"
        codePreview={longSql}
        onConfirm={() => {}}
        onCancel={() => {}}
      />,
    );
    // Should show the truncated indicator (i18n mock returns the key literally)
    expect(screen.getByText(/Showing first \{lines\} lines/)).toBeTruthy();
    // Should not show the last line
    expect(screen.queryByText('SELECT 19 FROM t')).toBeNull();
  });

  it('renders without badge when badge is not provided', () => {
    const { container } = render(
      <ConfirmDialog
        open
        title="Delete"
        message="Sure?"
        onConfirm={() => {}}
        onCancel={() => {}}
      />,
    );
    // No badge element should be present
    const badges = container.querySelectorAll('[class*="bg-amber-500"]');
    expect(badges.length).toBe(0);
  });
});
