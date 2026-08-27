import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { InstallExtensionDialog } from '../InstallExtensionDialog';
import type { ExtensionManifest, ExtensionSummary } from '../../../types/extension';

const { inspectPackageMock, installFromPathMock, fetchMock, onCloseMock } = vi.hoisted(() => ({
  inspectPackageMock: vi.fn(),
  installFromPathMock: vi.fn(),
  fetchMock: vi.fn(),
  onCloseMock: vi.fn(),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../commands/extensions', () => ({
  EXTENSIONS_CHANGED_EVENT: 'plugins:changed',
  extensionCommands: {
    inspectExtensionPackage: (...args: unknown[]) => inspectPackageMock(...args),
    installExtensionFromPath: (...args: unknown[]) => installFromPathMock(...args),
  },
}));

vi.mock('../../../stores/extensionStore', () => ({
  useExtensionStore: {
    getState: () => ({ fetch: fetchMock }),
  },
}));

const REVIEW_MANIFEST: ExtensionManifest = {
  id: 'acme.demo',
  name: 'Demo Plugin',
  version: '1.2.3',
  apiVersion: 2,
  author: 'Acme',
  description: 'Does demo things',
  entry: 'index.html',
  contributes: { pages: [], themes: [] },
  permissions: ['context:connections', 'command:invoke'],
};

function renderOpen(onInstalled?: (plugin: ExtensionSummary) => void) {
  return render(<InstallExtensionDialog open onClose={onCloseMock} onInstalled={onInstalled} />);
}

/** Types a path and advances to the review step (inspection must succeed). */
async function gotoReview(path = '/tmp/acme.zip') {
  fireEvent.change(screen.getByPlaceholderText('plugins.install.pathPlaceholder'), {
    target: { value: path },
  });
  fireEvent.click(screen.getByTestId('plugin-install-next'));
  await screen.findByTestId('plugin-install-review');
}

/** Types a path and triggers an inspection that rejects. */
async function inspectFailure(path: string) {
  fireEvent.change(screen.getByPlaceholderText('plugins.install.pathPlaceholder'), {
    target: { value: path },
  });
  fireEvent.click(screen.getByTestId('plugin-install-next'));
  await screen.findByTestId('plugin-install-error');
}

beforeEach(() => {
  inspectPackageMock.mockReset().mockResolvedValue(REVIEW_MANIFEST);
  installFromPathMock.mockReset();
  fetchMock.mockReset().mockResolvedValue(undefined);
  onCloseMock.mockReset();
});

afterEach(cleanup);

describe('InstallExtensionDialog', () => {
  it('keeps continue disabled until a non-blank path is entered', () => {
    renderOpen();

    const next = screen.getByTestId('plugin-install-next');
    expect(next).toBeDisabled();

    fireEvent.change(screen.getByPlaceholderText('plugins.install.pathPlaceholder'), {
      target: { value: '   ' },
    });
    expect(next).toBeDisabled();

    fireEvent.change(screen.getByPlaceholderText('plugins.install.pathPlaceholder'), {
      target: { value: '/tmp/acme.zip' },
    });
    expect(next).toBeEnabled();
  });

  it('does not invoke the backend for a blank path', () => {
    renderOpen();
    fireEvent.click(screen.getByTestId('plugin-install-next'));
    expect(inspectPackageMock).not.toHaveBeenCalled();
    expect(installFromPathMock).not.toHaveBeenCalled();
  });

  it('walks the two-step flow: inspect → review details/permissions → install', async () => {
    const installed = { id: 'acme.new', name: 'New' } as unknown as ExtensionSummary;
    const onInstalled = vi.fn();
    installFromPathMock.mockResolvedValue(installed);

    renderOpen(onInstalled);

    // Step 1 → 2: validation-only inspection of the trimmed path.
    await gotoReview('  /tmp/acme.zip  ');
    expect(inspectPackageMock).toHaveBeenCalledTimes(1);
    expect(inspectPackageMock).toHaveBeenCalledWith('/tmp/acme.zip');
    expect(installFromPathMock).not.toHaveBeenCalled();

    // Step 2 shows name/version/author plus the permission badge list.
    expect(screen.getByTestId('plugin-install-review')).toHaveTextContent('Demo Plugin');
    expect(screen.getByTestId('plugin-install-review')).toHaveTextContent('v1.2.3');
    expect(screen.getByTestId('plugin-install-review')).toHaveTextContent('Acme');
    const permissions = screen.getByTestId('plugin-install-permissions');
    expect(permissions).toHaveTextContent('context:connections');
    expect(permissions).toHaveTextContent('command:invoke');

    // Confirmation performs the actual install.
    fireEvent.click(screen.getByTestId('plugin-install-confirm'));
    await waitFor(() => expect(installFromPathMock).toHaveBeenCalledWith('/tmp/acme.zip'));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(onCloseMock).toHaveBeenCalledTimes(1));
    expect(onInstalled).toHaveBeenCalledWith(installed);
  });

  it('renders an empty permission list as "no permissions"', async () => {
    inspectPackageMock.mockResolvedValue({ ...REVIEW_MANIFEST, permissions: [] });

    renderOpen();
    await gotoReview();

    expect(screen.getByTestId('plugin-install-permissions')).toHaveTextContent(
      'plugins.install.noPermissions',
    );
  });

  it('never installs when cancelled from the review step', async () => {
    renderOpen();
    await gotoReview();

    // Back returns to the path-entry step without touching the backend.
    fireEvent.click(screen.getByTestId('plugin-install-back'));
    expect(await screen.findByTestId('plugin-install-next')).toBeInTheDocument();
    expect(installFromPathMock).not.toHaveBeenCalled();

    // Cancelling from the select step closes without installing either.
    fireEvent.click(screen.getByText('common.cancel'));
    expect(onCloseMock).toHaveBeenCalledTimes(1);
    expect(installFromPathMock).not.toHaveBeenCalled();
  });

  it('shows an inspection failure as a copyable error and stays on the select step', async () => {
    inspectPackageMock.mockRejectedValue(new Error('manifest invalid'));

    renderOpen();
    await inspectFailure('/tmp/broken.zip');

    const message = screen.getByTestId('plugin-install-error');
    // Copyable contract per PRD §4.3: selectable text + explicit copy action +
    // alert semantics.
    expect(message.className).toMatch(/selectable|copyable/);
    expect(message).toHaveAttribute('role', 'alert');
    expect(message).toHaveTextContent('manifest invalid');
    expect(screen.getByTestId('copyable-error-copy')).toBeInTheDocument();

    // Still on the select step; nothing was installed.
    expect(screen.queryByTestId('plugin-install-review')).not.toBeInTheDocument();
    expect(screen.getByTestId('plugin-install-next')).toBeEnabled();
    expect(installFromPathMock).not.toHaveBeenCalled();
    expect(screen.getByText('plugins.install.title')).toBeInTheDocument();
  });

  it('copies the raw error text to the clipboard via the copy button', async () => {
    inspectPackageMock.mockRejectedValue(new Error('boom: entry missing'));
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    renderOpen();
    await inspectFailure('/tmp/broken.zip');

    fireEvent.click(screen.getByTestId('copyable-error-copy'));
    expect(writeText).toHaveBeenCalledWith('boom: entry missing');
  });

  it('returns to the select step with a copyable error when the install fails', async () => {
    installFromPathMock.mockRejectedValue(new Error('disk full'));

    renderOpen();
    await gotoReview();
    fireEvent.click(screen.getByTestId('plugin-install-confirm'));

    expect(await screen.findByTestId('plugin-install-error')).toHaveTextContent('disk full');
    // Back on the path step so the user can retry; dialog stays open.
    expect(screen.queryByTestId('plugin-install-review')).not.toBeInTheDocument();
    expect(screen.getByTestId('plugin-install-next')).toBeEnabled();
    expect(onCloseMock).not.toHaveBeenCalled();
  });

  it('clears a previous error when the path changes again', async () => {
    inspectPackageMock.mockRejectedValue(new Error('first failure'));

    renderOpen();
    const input = screen.getByPlaceholderText('plugins.install.pathPlaceholder');
    fireEvent.change(input, { target: { value: '/tmp/broken.zip' } });
    fireEvent.click(screen.getByTestId('plugin-install-next'));
    await screen.findByTestId('plugin-install-error');

    fireEvent.change(input, { target: { value: '/tmp/other.zip' } });
    expect(screen.queryByTestId('plugin-install-error')).not.toBeInTheDocument();
  });
});
