import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { AiSessionHistory } from '../AiSessionHistory';

const actions = vi.hoisted(() => ({
  refresh: vi.fn(),
  select: vi.fn(),
  startNew: vi.fn(),
  remove: vi.fn(),
  exportSession: vi.fn(),
}));

const state = vi.hoisted(() => ({
  chatSessions: [
    {
      id: 's1',
      sessionKey: 'conn-1::db',
      connectionId: 'conn-1',
      database: 'db',
      title: 'First conversation',
      metadata: null,
      messageCount: 4,
      preview: 'hello',
      createdAt: '',
      updatedAt: '',
      lastAccessAt: '',
    },
    {
      id: 's2',
      sessionKey: 'conn-1::db',
      connectionId: 'conn-1',
      database: 'db',
      title: 'Older conversation',
      metadata: null,
      messageCount: 2,
      preview: 'older',
      createdAt: '',
      updatedAt: '',
      lastAccessAt: '',
    },
  ],
  chatSession: {
    id: 's1',
    sessionKey: 'conn-1::db',
    messages: [{ role: 'user', content: 'hello' }],
    isStreaming: false,
    streamContent: '',
    streamReasoning: '',
    streamMcpToolName: null,
    requestId: null,
  },
  sessionHistoryLoading: false,
  sessionHistoryError: null,
  sessionMigrationError: null as string | null,
  sessionMigrationSuccess: 0,
  refreshChatSessions: actions.refresh,
  selectChatSession: actions.select,
  newChatSession: actions.startNew,
  deleteChatSession: actions.remove,
  exportChatSession: actions.exportSession,
}));

vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (selector: (value: typeof state) => unknown) => selector(state),
}));
vi.mock('../../../hooks/useI18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  state.sessionMigrationError = null;
  state.sessionMigrationSuccess = 0;
});

describe('AiSessionHistory', () => {
  it('lists sessions for the current scope and navigates between them', () => {
    const onClose = vi.fn();
    render(<AiSessionHistory open onClose={onClose} />);

    expect(screen.getByTestId('ai-session-history')).toBeInTheDocument();
    expect(screen.getByText('First conversation')).toBeInTheDocument();
    expect(screen.getByText('Older conversation')).toBeInTheDocument();
    expect(actions.refresh).toHaveBeenCalled();

    fireEvent.click(screen.getByText('Older conversation'));
    expect(actions.select).toHaveBeenCalledWith('s2');
    expect(onClose).toHaveBeenCalled();
  });

  it('starts a new session, deletes and exports via visible controls', () => {
    render(<AiSessionHistory open onClose={vi.fn()} />);

    fireEvent.click(screen.getByTestId('ai-session-new'));
    expect(actions.startNew).toHaveBeenCalled();

    fireEvent.click(screen.getByTestId('ai-session-export-json'));
    expect(actions.exportSession).toHaveBeenCalledWith('json');
    fireEvent.click(screen.getByTestId('ai-session-export-markdown'));
    expect(actions.exportSession).toHaveBeenCalledWith('markdown');

    const deleteButtons = screen.getAllByLabelText('chat.sessions.delete');
    fireEvent.click(deleteButtons[1]);
    expect(actions.remove).toHaveBeenCalledWith('s2');
  });

  it('shows a successful migration count', () => {
    state.sessionMigrationSuccess = 2;
    render(<AiSessionHistory open onClose={vi.fn()} />);
    expect(screen.getByTestId('ai-session-migration-success')).toHaveTextContent(
      'chat.sessions.migrationSuccess',
    );
  });

  it('keeps a failed migration visible', () => {
    state.sessionMigrationError = 'migration failed';
    render(<AiSessionHistory open onClose={vi.fn()} />);
    expect(screen.getByTestId('ai-session-migration-error')).toHaveTextContent('migration failed');
  });
});
