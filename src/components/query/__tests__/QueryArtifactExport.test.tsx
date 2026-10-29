import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { setBackendClient } from '@datazen/backend-client';
import { QueryArtifactExport } from '../QueryArtifactExport';
import type { QueryResultArtifacts } from '@datazen/backend-client';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('../../../hooks/useI18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }));

// Form-split R6: the artifact commands ride the legacy invoke rail, which
// resolves the bound backend facade — bind the mocked invoke behind the
// transport so the wire assertions keep reading the same mock.
setBackendClient('test', {
  transport: {
    call: (command: string, payload?: unknown) =>
      invoke(command as never, payload as never) as never,
  },
} as never);

afterEach(cleanup);
beforeEach(() => vi.mocked(invoke).mockReset());
const artifacts: QueryResultArtifacts = {
  executionId: 'execution-1',
  artifactIds: ['artifact-1'],
  resultCompleteness: 'complete',
};

describe('published result export dialog journey', () => {
  it('passes only the artifact id to IPC; cancellation leaves a usable retry', async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce({ status: 'cancelled' })
      .mockResolvedValueOnce({ status: 'saved', artifactId: 'artifact-1', byteLength: 100 });
    render(<QueryArtifactExport artifacts={artifacts} />);
    fireEvent.click(screen.getByTestId('query-artifact-export-button'));
    await waitFor(() => expect(screen.queryByRole('status')).toBeNull());
    expect(invoke).toHaveBeenCalledWith('export_artifact', {
      request: { artifactId: 'artifact-1' },
    });
    expect(screen.queryByRole('alert')).toBeNull();
    fireEvent.click(screen.getByTestId('query-artifact-export-button'));
    await waitFor(() =>
      expect(screen.getByRole('status').textContent).toBe('query.artifactExport.saved'),
    );
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it('shows partial status and lets Host IPC own lifecycle confirmation', async () => {
    vi.mocked(invoke).mockResolvedValue({ status: 'cancelled' });
    render(<QueryArtifactExport artifacts={{ ...artifacts, resultCompleteness: 'truncated' }} />);
    expect(screen.getByText('query.artifactExport.partial')).toBeTruthy();
    fireEvent.click(screen.getByTestId('query-artifact-export-button'));
    await waitFor(() => expect(screen.queryByRole('status')).toBeNull());
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith('export_artifact', {
      request: { artifactId: 'artifact-1' },
    });
  });

  it('refuses pending artifacts, prevents duplicate dialogs, and recovers from rejection', async () => {
    const { rerender } = render(
      <QueryArtifactExport artifacts={{ ...artifacts, resultCompleteness: 'pending' }} />,
    );
    expect(screen.queryByTestId('query-artifact-export-button')).toBeNull();
    rerender(<QueryArtifactExport artifacts={artifacts} />);
    let reject: ((error: Error) => void) | undefined;
    vi.mocked(invoke).mockImplementationOnce(
      () =>
        new Promise((_resolve, fail) => {
          reject = fail;
        }),
    );
    fireEvent.click(screen.getByTestId('query-artifact-export-button'));
    fireEvent.click(screen.getByTestId('query-artifact-export-button'));
    expect(invoke).toHaveBeenCalledTimes(1);
    reject?.(new Error('Synthetic export failure'));
    await waitFor(() =>
      expect(screen.getByRole('alert').textContent).toBe('Synthetic export failure'),
    );
    vi.mocked(invoke).mockResolvedValueOnce({
      status: 'saved',
      artifactId: 'artifact-1',
      byteLength: 100,
    });
    fireEvent.click(screen.getByTestId('query-artifact-export-button'));
    await waitFor(() =>
      expect(screen.getByRole('status').textContent).toBe('query.artifactExport.saved'),
    );
    expect(screen.queryByRole('alert')).toBeNull();
  });
});
