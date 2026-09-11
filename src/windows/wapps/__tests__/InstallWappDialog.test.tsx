import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { InstallWappDialog } from '../InstallWappDialog';
import type { WappManifest, WappSummary } from '../../../types/wapp';

const { inspectWithDialogMock, installWappMock, fetchMock, onCloseMock } = vi.hoisted(() => ({
  inspectWithDialogMock: vi.fn(),
  installWappMock: vi.fn(),
  fetchMock: vi.fn(),
  onCloseMock: vi.fn(),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../commands/wapps', () => ({
  WAPPS_CHANGED_EVENT: 'wapps:changed',
  wappCommands: {
    inspectWappPackageWithDialog: (...args: unknown[]) => inspectWithDialogMock(...args),
    installWapp: (...args: unknown[]) => installWappMock(...args),
  },
}));

vi.mock('../../../stores/wappStore', () => ({
  useWappStore: {
    getState: () => ({ fetch: fetchMock }),
  },
}));

const REVIEW_MANIFEST: WappManifest = {
  id: 'acme.demo',
  name: 'Demo Wapp',
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

function renderOpen(onInstalled?: (plugin: WappSummary) => void) {
  return render(<InstallWappDialog open onClose={onCloseMock} onInstalled={onInstalled} />);
}

/** Clicks folder browse and advances to the review step (inspection must succeed). */
async function gotoReviewViaFolder() {
  fireEvent.click(screen.getByTestId('extension-install-browse-folder'));
  await screen.findByTestId('extension-install-review');
}

/** Clicks zip browse and triggers an inspection that rejects. */
async function inspectFailureViaZip() {
  fireEvent.click(screen.getByTestId('extension-install-browse-zip'));
  await screen.findByTestId('extension-install-error');
}

beforeEach(() => {
  inspectWithDialogMock.mockReset().mockResolvedValue(PREVIEW);
  installWappMock.mockReset();
  fetchMock.mockReset().mockResolvedValue(undefined);
  onCloseMock.mockReset();
});

afterEach(cleanup);

describe('InstallWappDialog', () => {
  it('shows native browse actions on the select step', () => {
    renderOpen();

    expect(screen.getByTestId('extension-install-browse-zip')).toBeEnabled();
    expect(screen.getByTestId('extension-install-browse-folder')).toBeEnabled();
    expect(screen.queryByTestId('extension-install-next')).not.toBeInTheDocument();
  });

  it('does not invoke the backend until a browse action is chosen', () => {
    renderOpen();
    expect(inspectWithDialogMock).not.toHaveBeenCalled();
    expect(installWappMock).not.toHaveBeenCalled();
  });

  it('walks the two-step flow: inspect → review details/permissions → install', async () => {
    const installed = { id: 'acme.new', name: 'New' } as unknown as WappSummary;
    const onInstalled = vi.fn();
    installWappMock.mockResolvedValue(installed);

    renderOpen(onInstalled);

    await gotoReviewViaFolder();
    expect(inspectWithDialogMock).toHaveBeenCalledTimes(1);
    expect(inspectWithDialogMock).toHaveBeenCalledWith('folder');
    expect(installWappMock).not.toHaveBeenCalled();

    expect(screen.getByTestId('extension-install-review')).toHaveTextContent('Demo Wapp');
    expect(screen.getByTestId('extension-install-review')).toHaveTextContent('v1.2.3');
    expect(screen.getByTestId('extension-install-review')).toHaveTextContent('Acme');
    expect(screen.getByTestId('extension-install-package-label')).toHaveTextContent('acme.zip');
    const permissions = screen.getByTestId('extension-install-permissions');
    expect(permissions).toHaveTextContent('context:connections');
    expect(permissions).toHaveTextContent('command:invoke');

    fireEvent.click(screen.getByTestId('extension-install-confirm'));
    await waitFor(() => expect(installWappMock).toHaveBeenCalledWith('pick-token-1'));
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

    expect(screen.getByTestId('extension-install-permissions')).toHaveTextContent(
      'extensions.install.noPermissions',
    );
  });

  it('never installs when cancelled from the review step', async () => {
    renderOpen();
    await gotoReviewViaFolder();

    fireEvent.click(screen.getByTestId('extension-install-back'));
    expect(await screen.findByTestId('extension-install-browse-folder')).toBeInTheDocument();
    expect(installWappMock).not.toHaveBeenCalled();

    fireEvent.click(screen.getByText('common.cancel'));
    expect(onCloseMock).toHaveBeenCalledTimes(1);
    expect(installWappMock).not.toHaveBeenCalled();
  });

  it('shows an inspection failure as a copyable error and stays on the select step', async () => {
    inspectWithDialogMock.mockRejectedValue(new Error('manifest invalid'));

    renderOpen();
    await inspectFailureViaZip();

    const message = screen.getByTestId('extension-install-error');
    expect(message.className).toMatch(/selectable|copyable/);
    expect(message).toHaveAttribute('role', 'alert');
    expect(message).toHaveTextContent('manifest invalid');
    expect(screen.getByTestId('copyable-error-copy')).toBeInTheDocument();

    expect(screen.queryByTestId('extension-install-review')).not.toBeInTheDocument();
    expect(screen.getByTestId('extension-install-browse-zip')).toBeEnabled();
    expect(installWappMock).not.toHaveBeenCalled();
    expect(screen.getByText('extensions.install.title')).toBeInTheDocument();
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
    installWappMock.mockRejectedValue(new Error('disk full'));

    renderOpen();
    await gotoReviewViaFolder();
    fireEvent.click(screen.getByTestId('extension-install-confirm'));

    expect(await screen.findByTestId('extension-install-error')).toHaveTextContent('disk full');
    expect(screen.queryByTestId('extension-install-review')).not.toBeInTheDocument();
    expect(screen.getByTestId('extension-install-browse-folder')).toBeEnabled();
    expect(onCloseMock).not.toHaveBeenCalled();
  });

  it('ignores a cancelled native picker without surfacing an error', async () => {
    inspectWithDialogMock.mockResolvedValue(null);

    renderOpen();
    fireEvent.click(screen.getByTestId('extension-install-browse-zip'));

    await waitFor(() => expect(inspectWithDialogMock).toHaveBeenCalledTimes(1));
    expect(screen.queryByTestId('extension-install-error')).not.toBeInTheDocument();
    expect(screen.queryByTestId('extension-install-review')).not.toBeInTheDocument();
    expect(screen.getByTestId('extension-install-browse-zip')).toBeEnabled();
  });
});
