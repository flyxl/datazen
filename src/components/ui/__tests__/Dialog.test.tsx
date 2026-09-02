import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup, screen } from '@testing-library/react';
import { Dialog } from '../Dialog';

afterEach(cleanup);

describe('Dialog', () => {
  it('renders title and children when open', () => {
    render(
      <Dialog open title="Test Dialog" onClose={() => {}}>
        <p>Dialog body</p>
      </Dialog>,
    );
    expect(screen.getByText('Test Dialog')).toBeInTheDocument();
    expect(screen.getByText('Dialog body')).toBeInTheDocument();
  });

  it('does not render when closed', () => {
    const { container } = render(
      <Dialog open={false} title="Hidden" onClose={() => {}}>
        <p>Hidden body</p>
      </Dialog>,
    );
    expect(container.innerHTML).toBe('');
  });

  it('does not close when clicking the backdrop', () => {
    const onClose = vi.fn();
    render(
      <Dialog open title="Backdrop test" onClose={onClose}>
        <p>Content</p>
      </Dialog>,
    );
    const backdrop = document.querySelector('[aria-hidden="true"]');
    expect(backdrop).toBeTruthy();
    fireEvent.click(backdrop!);
    expect(onClose).not.toHaveBeenCalled();
  });

  it('closes when the header close button is clicked', () => {
    const onClose = vi.fn();
    render(
      <Dialog open title="Close button" onClose={onClose}>
        <p>Content</p>
      </Dialog>,
    );
    const closeButtons = screen.getAllByRole('button');
    fireEvent.click(closeButtons[0]);
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('closes on Escape', () => {
    const onClose = vi.fn();
    render(
      <Dialog open title="Escape test" onClose={onClose}>
        <p>Content</p>
      </Dialog>,
    );
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('focuses the dialog on open and restores the opener on close', () => {
    const onClose = vi.fn();
    const { rerender } = render(
      <>
        <button type="button">Open</button>
        <Dialog open={false} title="Focus test" onClose={onClose}>
          <input aria-label="Dialog input" />
        </Dialog>
      </>,
    );
    const opener = screen.getByRole('button', { name: 'Open' });
    opener.focus();
    rerender(
      <>
        <button type="button">Open</button>
        <Dialog open title="Focus test" onClose={onClose}>
          <input aria-label="Dialog input" />
        </Dialog>
      </>,
    );
    expect(screen.getByRole('button', { name: 'Close' })).toHaveFocus();
    rerender(
      <>
        <button type="button">Open</button>
        <Dialog open={false} title="Focus test" onClose={onClose}>
          <input aria-label="Dialog input" />
        </Dialog>
      </>,
    );
    expect(screen.getByRole('button', { name: 'Open' })).toHaveFocus();
  });

  it('wraps Tab focus within the dialog', () => {
    render(
      <Dialog open title="Tab test" onClose={() => {}} footer={<button type="button">Last</button>}>
        <p>Content</p>
      </Dialog>,
    );
    const dialog = screen.getByRole('dialog');
    const closeButton = screen.getByRole('button', { name: 'Close' });
    screen.getByRole('button', { name: 'Last' }).focus();
    fireEvent.keyDown(dialog, { key: 'Tab' });
    expect(closeButton).toHaveFocus();
  });
});
