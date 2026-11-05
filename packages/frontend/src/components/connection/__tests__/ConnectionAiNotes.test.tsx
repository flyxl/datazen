import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { ConnectionAiNotes } from '../ConnectionAiNotes';

const notesGet = vi.fn();
const notesSet = vi.fn();

vi.mock('../../../commands/ai', () => ({
  aiCommands: {
    connectionNotesGet: (...args: unknown[]) => notesGet(...args),
    connectionNotesSet: (...args: unknown[]) => notesSet(...args),
  },
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

describe('ConnectionAiNotes', () => {
  beforeEach(() => {
    notesGet.mockReset().mockResolvedValue('read-only replica');
    notesSet.mockReset().mockResolvedValue(undefined);
  });

  afterEach(cleanup);

  it('loads and saves notes through the AI IPC', async () => {
    render(<ConnectionAiNotes connectionId="conn-1" />);

    await waitFor(() => {
      expect((screen.getByTestId('connection-notes-input') as HTMLTextAreaElement).value).toBe(
        'read-only replica',
      );
    });

    fireEvent.change(screen.getByTestId('connection-notes-input'), {
      target: { value: 'Use UTC timestamps.' },
    });
    fireEvent.click(screen.getByTestId('connection-notes-save'));

    await waitFor(() => {
      expect(notesSet).toHaveBeenCalledWith('conn-1', 'Use UTC timestamps.');
    });
  });

  it('shows the 4KB counter and prevents an over-limit save', async () => {
    render(<ConnectionAiNotes connectionId="conn-1" />);
    await waitFor(() => expect(notesGet).toHaveBeenCalledWith('conn-1'));

    fireEvent.change(screen.getByTestId('connection-notes-input'), {
      target: { value: 'a'.repeat(4097) },
    });

    expect(screen.getByTestId('connection-notes-count').textContent).toContain('4097 / 4096');
    expect(screen.getByTestId('connection-notes-count').textContent).toContain(
      'connection.notes.overLimit',
    );
    expect(screen.getByTestId('connection-notes-save')).toBeDisabled();
    expect(notesSet).not.toHaveBeenCalled();
  });
});
