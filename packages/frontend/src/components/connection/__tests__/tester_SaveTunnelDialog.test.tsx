/**
 * [tester] Validation / error-path coverage for the "save as tunnel" dialog. The
 * happy path is covered elsewhere; these tests pin the empty-name, whitespace,
 * retry and overlong-name behaviour plus the rollback guarantee on failure.
 */
import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { SaveTunnelDialog } from '../SaveTunnelDialog';
import type { SavedTunnel } from '../../../types';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

afterEach(cleanup);

const CREATED: SavedTunnel = { id: 'tun_new', name: 'Bastion', kind: 'ssh' };

function setup(props: Partial<React.ComponentProps<typeof SaveTunnelDialog>> = {}) {
  const onSubmit = vi.fn().mockResolvedValue(CREATED);
  const onClose = vi.fn();
  const view = render(<SaveTunnelDialog open onClose={onClose} onSubmit={onSubmit} {...props} />);
  return { onSubmit, onClose, view };
}

function typeName(value: string) {
  fireEvent.change(screen.getByTestId('save-tunnel-name'), { target: { value } });
}

describe('[tester] SaveTunnelDialog', () => {
  it('keeps confirm disabled for an empty or whitespace-only name', () => {
    const { onSubmit } = setup();
    const confirm = screen.getByTestId('save-tunnel-confirm');

    expect(confirm).toBeDisabled();

    typeName('   ');
    expect(confirm).toBeDisabled();

    fireEvent.click(confirm);
    expect(onSubmit).not.toHaveBeenCalled();

    typeName('  Bastion  ');
    expect(confirm).not.toBeDisabled();
  });

  it('submits the trimmed name and closes on success', async () => {
    const { onSubmit, onClose } = setup();
    typeName('  Bastion  ');
    fireEvent.click(screen.getByTestId('save-tunnel-confirm'));

    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith('Bastion'));
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
  });

  it('stays open and stays retryable when persistence fails', async () => {
    // `saveAsTunnel` resolves to null on failure (the hook surfaces the error).
    const onSubmit = vi.fn().mockResolvedValue(null);
    const onClose = vi.fn();
    render(
      <SaveTunnelDialog
        open
        onClose={onClose}
        onSubmit={onSubmit}
        error="newConn.tunnelSaveFailed"
      />,
    );

    typeName('Nope');
    fireEvent.click(screen.getByTestId('save-tunnel-confirm'));

    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith('Nope'));
    // No half-finished state: the dialog must not close and the error is shown.
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByTestId('save-tunnel-error')).toHaveTextContent('newConn.tunnelSaveFailed');
    await waitFor(() => expect(screen.getByTestId('save-tunnel-confirm')).not.toBeDisabled());

    // Retrying with the same name is allowed and re-invokes the callback.
    fireEvent.click(screen.getByTestId('save-tunnel-confirm'));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(2));
  });

  it('ignores clicks and disables both actions while a save is in flight', () => {
    const { onSubmit } = setup({ busy: true });
    typeName('Bastion');

    expect(screen.getByTestId('save-tunnel-confirm')).toBeDisabled();
    expect(screen.getByTestId('save-tunnel-cancel')).toBeDisabled();
    expect(screen.getByTestId('save-tunnel-confirm')).toHaveTextContent('newConn.tunnelSaving');

    fireEvent.click(screen.getByTestId('save-tunnel-confirm'));
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it('resets the name field every time the dialog is reopened', () => {
    const onSubmit = vi.fn().mockResolvedValue(CREATED);
    const onClose = vi.fn();
    const { rerender } = render(<SaveTunnelDialog open onClose={onClose} onSubmit={onSubmit} />);
    typeName('Bastion');
    expect(screen.getByTestId('save-tunnel-name')).toHaveValue('Bastion');

    rerender(<SaveTunnelDialog open={false} onClose={onClose} onSubmit={onSubmit} />);
    rerender(<SaveTunnelDialog open onClose={onClose} onSubmit={onSubmit} />);

    expect(screen.getByTestId('save-tunnel-name')).toHaveValue('');
  });

  it('accepts an arbitrarily long name — no max-length guard exists (reported as an improvement)', async () => {
    const { onSubmit } = setup();
    const longName = 'B'.repeat(512);
    typeName(longName);
    fireEvent.click(screen.getByTestId('save-tunnel-confirm'));

    // Current behaviour: length is never validated, here or in `tunnelStore.create`
    // (which only mints a fresh `tun_*` id and therefore also allows duplicates).
    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith(longName));
  });
});
