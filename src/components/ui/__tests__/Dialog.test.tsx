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

  it('does not close on Escape', () => {
    const onClose = vi.fn();
    render(
      <Dialog open title="Escape test" onClose={onClose}>
        <p>Content</p>
      </Dialog>,
    );
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();
  });
});
