/**
 * ErrorBanner extraction parity — "save as tunnel" dialog.
 *
 * The banner's content arrives as a prop from the parent form, so the risk
 * here is not plumbing but presentation: losing the box, the role, or the
 * margin that separates it from the name field.
 */
import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { SaveTunnelDialog } from '../SaveTunnelDialog';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

afterEach(cleanup);

describe('SaveTunnelDialog error banner', () => {
  it('announces the form error in the boxed variant', () => {
    render(
      <SaveTunnelDialog
        open
        onClose={vi.fn()}
        onSubmit={vi.fn().mockResolvedValue({ id: 'tun_new', name: 'B', kind: 'ssh' })}
        error="tunnel name already taken"
      />,
    );

    const banner = screen.getByTestId('save-tunnel-error');
    expect(banner.tagName).toBe('DIV');
    expect(banner).toHaveAttribute('role', 'alert');
    expect(banner.textContent).toBe('tunnel name already taken');
    // Boxed surface on the `danger` token, plus the call site's own top margin.
    // The call site restates no colour, so all of it comes from the variant.
    expect(banner).toHaveClass('rounded-md', 'border', 'bg-danger/10', 'mt-3');
    expect(banner).toHaveClass('text-xs', 'text-danger');
    expect(banner.className).not.toMatch(/red-\d/);
  });

  it('is not rendered when the form has no error', () => {
    render(
      <SaveTunnelDialog
        open
        onClose={vi.fn()}
        onSubmit={vi.fn().mockResolvedValue({ id: 'tun_new', name: 'B', kind: 'ssh' })}
      />,
    );

    expect(screen.queryByTestId('save-tunnel-error')).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('leaves the name field usable so the error is actionable', () => {
    render(
      <SaveTunnelDialog
        open
        onClose={vi.fn()}
        onSubmit={vi.fn().mockResolvedValue({ id: 'tun_new', name: 'B', kind: 'ssh' })}
        error="tunnel name already taken"
      />,
    );

    fireEvent.change(screen.getByTestId('save-tunnel-name'), { target: { value: 'Bastion 2' } });
    expect(screen.getByTestId('save-tunnel-name')).toHaveValue('Bastion 2');
  });
});
