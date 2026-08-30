import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { InstallExtensionDialog } from '../InstallExtensionDialog';
import type { ExtensionManifest, ExtensionSummary } from '../../../types/extension';

const { inspectWithDialogMock, installExtensionMock, fetchMock, onCloseMock } = vi.hoisted(() => ({
  inspectWithDialogMock: vi.fn(),
  installExtensionMock: vi.fn(),
  fetchMock: vi.fn(),
  onCloseMock: vi.fn(),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../commands/extensions', () => ({
  EXTENSIONS_CHANGED_EVENT: 'plugins:changed',
  extensionCommands: {
    inspectExtensionPackageWithDialog: (...args: unknown[]) => inspectWithDialogMock(...args),
    installExtension: (...args: unknown[]) => installExtensionMock(...args),
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

const PREVIEW = {
  pickToken: 'pick-token-1',
  packageLabel: 'acme.zip',
  manifest: REVIEW_MANIFEST,
};

function renderOpen(onInstalled?: (plugin: ExtensionSummary) => void) {
  return render(<InstallExtensionDialog open onClose={onCloseMock} onInstalled={onInstalled} />);
}

/** Clicks folder browse and advances to the review step (inspection must succeed). */
async function gotoReviewViaFolder() {
  fireEvent.click(screen.getByTestId('plugin-install-browse-folder'));
  await screen.findByTestId('plugin-install-review');
}

/** Clicks zip browse and triggers an inspection that rejects. */
async function inspectFailureViaZip() {
  fireEvent.click(screen.getByTestId('plugin-install-browse-zip'));
  await screen.findByTestId('plugin-install-error');
}

beforeEach(() => {
  inspectWithDialogMock.mockReset().mockResolvedValue(PREVIEW);
  installExtensionMock.mockReset();
  fetchMock.mockReset().mockResolvedValue(undefined);
  onCloseMock.mockReset();
});

afterEach(cleanup);

describe('InstallExtensionDialog', () => {
  it('shows native browse actions on the select step', () => {
    renderOpen();

    expect(screen.getByTestId('plugin-install-browse-zip')).toBeEnabled();
    expect(screen.getByTestId('plugin-install-browse-folder')).toBeEnabled();
    expect(screen.queryByTestId('plugin-install-next')).not.toBeInTheDocument();
  });

  it('does not invoke the backend until a browse action is chosen', () => {
    renderOpen();
    expect(inspectWithDialogMock).not.toHaveBeenCalled();
    expect(installExtensionMock).not.toHaveBeenCalled();
  });

  it('walks the two-step flow: inspect → review details/permissions → install', async () => {
    const installed = { id: 'acme.new', name: 'New' } as unknown as ExtensionSummary;
    const onInstalled = vi.fn();
    installExtensionMock.mockResolvedValue(installed);

    renderOpen(onInstalled);

    await gotoReviewViaFolder();
    expect(inspectWithDialogMock).toHaveBeenCalledTimes(1);
    expect(inspectWithDialogMock).toHaveBeenCalledWith('folder');
    expect(installExtensionMock).not.toHaveBeenCalled();

    expect(screen.getByTestId('plugin-install-review')).toHaveTextContent('Demo Plugin');
    expect(screen.getByTestId('plugin-install-review')).toHaveTextContent('v1.2.3');
    expect(screen.getByTestId('plugin-install-review')).toHaveTextContent('Acme');
    expect(screen.getByTestId('plugin-install-package-label')).toHaveTextContent('acme.zip');
    const permissions = screen.getByTestId('plugin-install-permissions');
    expect(permissions).toHaveTextContent('context:connections');
    expect(permissions).toHaveTextContent('command:invoke');

    fireEvent.click(screen.getByTestId('plugin-install-confirm'));
    await waitFor(() => expect(installExtensionMock).toHaveBeenCalledWith('pick-token-1'));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(onCloseMock).toHaveBeenCalledTimes(1));
    expect(onInstalled).toHaveBeenCalledWith(installed);
  });

  it('renders an empty permission list as "no permissions"', async () => {
    inspectWithDialogMock.mockResolvedValue({
      ...PREVIEW,
      manifest: { ...REVIEW_MANIFEST, permissions: [] },
    });

    renderOpen();
    await gotoReviewViaFolder();

    expect(screen.getByTestId('plugin-install-permissions')).toHaveTextContent(
      'plugins.install.noPermissions',
    );
  });

  it('never installs when cancelled from the review step', async () => {
    renderOpen();
    await gotoReviewViaFolder();

    fireEvent.click(screen.getByTestId('plugin-install-back'));
    expect(await screen.findByTestId('plugin-install-browse-folder')).toBeInTheDocument();
    expect(installExtensionMock).not.toHaveBeenCalled();

    fireEvent.click(screen.getByText('common.cancel'));
    expect(onCloseMock).toHaveBeenCalledTimes(1);
    expect(installExtensionMock).not.toHaveBeenCalled();
  });

  it('shows an inspection failure as a copyable error and stays on the select step', async () => {
    inspectWithDialogMock.mockRejectedValue(new Error('manifest invalid'));

    renderOpen();
    await inspectFailureViaZip();

    const message = screen.getByTestId('plugin-install-error');
    expect(message.className).toMatch(/selectable|copyable/);
    expect(message).toHaveAttribute('role', 'alert');
    expect(message).toHaveTextContent('manifest invalid');
    expect(screen.getByTestId('copyable-error-copy')).toBeInTheDocument();

    expect(screen.queryByTestId('plugin-install-review')).not.toBeInTheDocument();
    expect(screen.getByTestId('plugin-install-browse-zip')).toBeEnabled();
    expect(installExtensionMock).not.toHaveBeenCalled();
    expect(screen.getByText('plugins.install.title')).toBeInTheDocument();
  });

  it('copies the raw error text to the clipboard via the copy button', async () => {
    inspectWithDialogMock.mockRejectedValue(new Error('boom: entry missing'));
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    renderOpen();
    await inspectFailureViaZip();

    fireEvent.click(screen.getByTestId('copyable-error-copy'));
    expect(writeText).toHaveBeenCalledWith('boom: entry missing');
  });

  it('returns to the select step with a copyable error when the install fails', async () => {
    installExtensionMock.mockRejectedValue(new Error('disk full'));

    renderOpen();
    await gotoReviewViaFolder();
    fireEvent.click(screen.getByTestId('plugin-install-confirm'));

    expect(await screen.findByTestId('plugin-install-error')).toHaveTextContent('disk full');
    expect(screen.queryByTestId('plugin-install-review')).not.toBeInTheDocument();
    expect(screen.getByTestId('plugin-install-browse-folder')).toBeEnabled();
    expect(onCloseMock).not.toHaveBeenCalled();
  });

  it('ignores a cancelled native picker without surfacing an error', async () => {
    inspectWithDialogMock.mockResolvedValue(null);

    renderOpen();
    fireEvent.click(screen.getByTestId('plugin-install-browse-zip'));

    await waitFor(() => expect(inspectWithDialogMock).toHaveBeenCalledTimes(1));
    expect(screen.queryByTestId('plugin-install-error')).not.toBeInTheDocument();
    expect(screen.queryByTestId('plugin-install-review')).not.toBeInTheDocument();
    expect(screen.getByTestId('plugin-install-browse-zip')).toBeEnabled();
  });
});
