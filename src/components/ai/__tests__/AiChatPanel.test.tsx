import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { AiChatPanel, QuestionBlock } from '../AiChatPanel';
import type { AiChatDraftRequest } from '../../../windows/connection/query/aiDraftBridge';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const openSettingsWindow = vi.fn();
const openDocsWindow = vi.fn();
vi.mock('../../../lib/windowManager', () => ({
  openSettingsWindow: (...args: unknown[]) => openSettingsWindow(...args),
  openDocsWindow: (...args: unknown[]) => openDocsWindow(...args),
}));

vi.mock('../WorkflowPanel', () => ({
  WorkflowPanel: ({ dbSessionId }: { dbSessionId: string }) => (
    <div data-testid="workflow-panel">{dbSessionId}</div>
  ),
}));

vi.mock('../../SqlCodeBlock', () => ({
  SqlCodeBlock: ({ code }: { code: string }) => <div data-testid="sql-code-block">{code}</div>,
}));

vi.mock('../AiInput', () => ({
  AiInput: ({
    value,
    onChange,
    onSubmit,
    disabled,
  }: {
    value: string;
    onChange: (v: string) => void;
    onSubmit: () => void;
    disabled?: boolean;
  }) => (
    <div>
      <textarea
        data-testid="chat-input"
        value={value}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
      />
      <button type="button" data-testid="chat-send" onClick={onSubmit}>
        send
      </button>
    </div>
  ),
}));

const aiState = vi.hoisted(() => ({
  isConfigured: true,
  chatSession: {
    messages: [] as {
      role: 'user' | 'assistant';
      content: string;
      reasoning?: string;
      questions?: {
        id: string;
        prompt: string;
        allowMultiple?: boolean;
        options: { id: string; label: string }[];
      }[];
    }[],
    isStreaming: false,
    streamContent: '',
    streamReasoning: '',
    streamMcpToolName: null as string | null,
  },
  initChatSession: vi.fn(),
  sendChatMessage: vi.fn().mockResolvedValue(undefined),
  clearChat: vi.fn(),
}));

vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (sel: (s: typeof aiState) => unknown) => sel(aiState),
}));

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  Element.prototype.scrollIntoView = vi.fn();
  aiState.isConfigured = true;
  aiState.chatSession = {
    messages: [],
    isStreaming: false,
    streamContent: '',
    streamReasoning: '',
  };
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: { writeText: vi.fn().mockResolvedValue(undefined) },
  });
});

describe('AiChatPanel', () => {
  it('shows not configured empty state', () => {
    aiState.isConfigured = false;
    const { getByText } = render(<AiChatPanel dbSessionId="c1" database="db" />);
    fireEvent.click(getByText('settings.ai.goToConfigure'));
    expect(openSettingsWindow).toHaveBeenCalledWith('ai');
  });

  it('initializes chat session and shows welcome', () => {
    aiState.chatSession = null as unknown as typeof aiState.chatSession;
    const { getByText, rerender } = render(<AiChatPanel dbSessionId="c1" />);
    expect(aiState.initChatSession).toHaveBeenCalled();
    aiState.chatSession = {
      messages: [],
      isStreaming: false,
      streamContent: '',
      streamReasoning: '',
      streamMcpToolName: null,
    };
    rerender(<AiChatPanel dbSessionId="c1" />);
    expect(getByText('chat.welcome')).toBeInTheDocument();
  });

  it('sends message from input', () => {
    const { getByTestId } = render(<AiChatPanel dbSessionId="c1" database="db" />);
    fireEvent.change(getByTestId('chat-input'), { target: { value: 'hello' } });
    fireEvent.click(getByTestId('chat-send'));
    expect(aiState.sendChatMessage).toHaveBeenCalledWith({
      dbSessionId: 'c1',
      database: 'db',
      content: 'hello',
      contextFiles: undefined,
      contextTables: undefined,
    });
  });

  it('renders messages with inline code blocks and insert SQL', () => {
    const onInsertSql = vi.fn();
    aiState.chatSession.messages = [
      { role: 'user', content: 'help' },
      {
        role: 'assistant',
        content: 'Try:\n```sql\nSELECT 1\n```',
        reasoning: 'thinking...',
      },
    ];
    const { getByText, getByTestId, queryByText } = render(
      <AiChatPanel dbSessionId="c1" onInsertSql={onInsertSql} />,
    );
    expect(getByText('help')).toBeInTheDocument();
    expect(getByText('Try:')).toBeInTheDocument();
    expect(queryByText('```sql')).toBeNull();
    expect(getByTestId('ai-code-block')).toBeInTheDocument();
    expect(getByTestId('sql-code-block')).toHaveTextContent('SELECT 1');
    fireEvent.click(getByText('chat.reasoning'));
    expect(getByText('thinking...')).toBeInTheDocument();
    fireEvent.click(getByTestId('ai-code-insert'));
    expect(onInsertSql).toHaveBeenCalledWith('SELECT 1');
    fireEvent.click(getByTestId('ai-code-copy'));
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith('SELECT 1');
  });

  it('shows streaming states', () => {
    aiState.chatSession.isStreaming = true;
    aiState.chatSession.streamContent = 'partial';
    const { getByText, rerender } = render(<AiChatPanel dbSessionId="c1" />);
    expect(getByText('partial')).toBeInTheDocument();

    aiState.chatSession.streamContent = '';
    aiState.chatSession.streamReasoning = '';
    aiState.chatSession.streamMcpToolName = null;
    rerender(<AiChatPanel dbSessionId="c1" />);
    expect(getByText('chat.thinking')).toBeInTheDocument();

    aiState.chatSession.streamMcpToolName = 'mcp/files/read_file';
    rerender(<AiChatPanel dbSessionId="c1" />);
    expect(getByText('chat.callingMcpTool')).toBeInTheDocument();
  });

  it('switches to workflows tab and clears chat', () => {
    aiState.chatSession.messages = [{ role: 'user', content: 'x' }];
    const { getByText, getByTestId } = render(<AiChatPanel dbSessionId="c1" />);
    fireEvent.click(getByText('ai.workflows.tab'));
    expect(getByTestId('workflow-panel')).toHaveTextContent('c1');
    fireEvent.click(getByText('common.aiAssistant'));
    const clearBtn = Array.from(document.querySelectorAll('button')).find(
      (b) => !b.disabled && b.querySelector('.lucide-trash2'),
    )!;
    fireEvent.click(clearBtn);
    expect(aiState.clearChat).toHaveBeenCalled();
    fireEvent.click(getByText('common.aiAssistant'));
    const docsBtn = Array.from(document.querySelectorAll('button')).find(
      (b) => b.getAttribute('title') === 'docs.openAiHelp',
    );
    if (docsBtn) fireEvent.click(docsBtn);
    expect(openDocsWindow).toHaveBeenCalledWith('context');
  });
});

describe('QuestionBlock', () => {
  it('submits selected answers', () => {
    const onSubmit = vi.fn();
    const questions = [
      {
        id: 'q1',
        prompt: 'Pick one',
        options: [
          { id: 'a', label: 'Alpha' },
          { id: 'b', label: 'Beta' },
        ],
      },
    ];
    const { getByText } = render(<QuestionBlock questions={questions} onSubmit={onSubmit} />);
    fireEvent.click(getByText('Alpha'));
    fireEvent.click(getByText('chat.questions.submit'));
    expect(onSubmit).toHaveBeenCalledWith('Pick one\nAlpha');
  });
});

describe('AiChatPanel — AI Draft Bridge (S3-B2)', () => {
  const makeDraft = (overrides?: Partial<Parameters<typeof AiChatPanel>[0]>) =>
    ({
      requestId: 'req-1',
      source: 'query-error' as const,
      panelId: 'p1',
      connectionId: 'conn1',
      database: 'testdb',
      content: 'SELECT * FROM users WHERE id = 1',
      focus: true,
      ...overrides,
    }) as AiChatDraftRequest;

  it('prefills empty input with draft and calls onDraftConsumed', async () => {
    const onDraftConsumed = vi.fn();
    const draft = makeDraft();
    const { getByTestId } = render(
      <AiChatPanel
        dbSessionId="c1"
        database="db"
        draftRequest={draft}
        onDraftConsumed={onDraftConsumed}
      />,
    );
    await waitFor(() => {
      expect(getByTestId('chat-input')).toHaveValue('SELECT * FROM users WHERE id = 1');
    });
    expect(onDraftConsumed).toHaveBeenCalledWith('req-1');
  });

  it('skips draft when AI is not configured', () => {
    aiState.isConfigured = false;
    const onDraftConsumed = vi.fn();
    const draft = makeDraft();
    render(
      <AiChatPanel
        dbSessionId="c1"
        database="db"
        draftRequest={draft}
        onDraftConsumed={onDraftConsumed}
      />,
    );
    expect(onDraftConsumed).not.toHaveBeenCalled();
  });

  it('does not send during streaming, only prefills', async () => {
    aiState.chatSession.isStreaming = true;
    const onDraftConsumed = vi.fn();
    const draft = makeDraft();
    render(
      <AiChatPanel
        dbSessionId="c1"
        database="db"
        draftRequest={draft}
        onDraftConsumed={onDraftConsumed}
      />,
    );
    // Draft prefills but does not trigger send.
    expect(aiState.sendChatMessage).not.toHaveBeenCalled();
  });

  it('shows conflict bar when input already has content', async () => {
    const onDraftConsumed = vi.fn();
    // First render with empty input to let the draft through.
    const draft1 = makeDraft({ requestId: 'req-1', content: 'old text' });
    const { rerender, getByTestId } = render(
      <AiChatPanel
        dbSessionId="c1"
        database="db"
        draftRequest={draft1}
        onDraftConsumed={onDraftConsumed}
      />,
    );
    // Simulate user typing in the input.
    fireEvent.change(getByTestId('chat-input'), { target: { value: 'user typed here' } });

    // Now a second draft arrives while input has content.
    const draft2 = makeDraft({ requestId: 'req-2', content: 'NEW DRAFT' });
    rerender(
      <AiChatPanel
        dbSessionId="c1"
        database="db"
        draftRequest={draft2}
        onDraftConsumed={onDraftConsumed}
      />,
    );

    await waitFor(() => {
      expect(getByTestId('draft-conflict-bar')).toBeInTheDocument();
    });
    expect(onDraftConsumed).not.toHaveBeenCalledWith('req-2');
  });

  it('replace strategy overwrites input', async () => {
    const onDraftConsumed = vi.fn();
    const draft1 = makeDraft({ requestId: 'req-1', content: 'old' });
    const { rerender, getByTestId, queryByTestId } = render(
      <AiChatPanel
        dbSessionId="c1"
        database="db"
        draftRequest={draft1}
        onDraftConsumed={onDraftConsumed}
      />,
    );
    fireEvent.change(getByTestId('chat-input'), { target: { value: 'existing text' } });

    const draft2 = makeDraft({ requestId: 'req-2', content: 'replaced content' });
    rerender(
      <AiChatPanel
        dbSessionId="c1"
        database="db"
        draftRequest={draft2}
        onDraftConsumed={onDraftConsumed}
      />,
    );

    await waitFor(() => {
      expect(getByTestId('draft-replace')).toBeInTheDocument();
    });
    fireEvent.click(getByTestId('draft-replace'));

    expect(getByTestId('chat-input')).toHaveValue('replaced content');
    expect(onDraftConsumed).toHaveBeenCalledWith('req-2');
    expect(queryByTestId('draft-conflict-bar')).not.toBeInTheDocument();
  });

  it('append strategy adds separator and content', async () => {
    const onDraftConsumed = vi.fn();
    const draft1 = makeDraft({ requestId: 'req-1', content: 'a' });
    const { rerender, getByTestId } = render(
      <AiChatPanel
        dbSessionId="c1"
        database="db"
        draftRequest={draft1}
        onDraftConsumed={onDraftConsumed}
      />,
    );
    fireEvent.change(getByTestId('chat-input'), { target: { value: 'existing' } });

    const draft2 = makeDraft({ requestId: 'req-2', content: 'appended' });
    rerender(
      <AiChatPanel
        dbSessionId="c1"
        database="db"
        draftRequest={draft2}
        onDraftConsumed={onDraftConsumed}
      />,
    );

    await waitFor(() => {
      expect(getByTestId('draft-append')).toBeInTheDocument();
    });
    fireEvent.click(getByTestId('draft-append'));

    const textarea = getByTestId('chat-input') as HTMLTextAreaElement;
    expect(textarea.value).toContain('existing');
    expect(textarea.value).toContain('appended');
    expect(onDraftConsumed).toHaveBeenCalledWith('req-2');
  });

  it('dismiss conflict clears pending without consuming', async () => {
    const onDraftConsumed = vi.fn();
    const draft1 = makeDraft({ requestId: 'req-1', content: 'x' });
    const { rerender, getByTestId, queryByTestId } = render(
      <AiChatPanel
        dbSessionId="c1"
        database="db"
        draftRequest={draft1}
        onDraftConsumed={onDraftConsumed}
      />,
    );
    fireEvent.change(getByTestId('chat-input'), { target: { value: 'keep this' } });

    const draft2 = makeDraft({ requestId: 'req-2', content: 'ignored' });
    rerender(
      <AiChatPanel
        dbSessionId="c1"
        database="db"
        draftRequest={draft2}
        onDraftConsumed={onDraftConsumed}
      />,
    );

    await waitFor(() => {
      expect(getByTestId('draft-cancel')).toBeInTheDocument();
    });
    fireEvent.click(getByTestId('draft-cancel'));

    expect(getByTestId('chat-input')).toHaveValue('keep this');
    expect(onDraftConsumed).not.toHaveBeenCalledWith('req-2');
    expect(queryByTestId('draft-conflict-bar')).not.toBeInTheDocument();
  });

  it('same requestId is not processed twice (idempotent ack)', async () => {
    const onDraftConsumed = vi.fn();
    const draft = makeDraft({ requestId: 'req-1' });
    const { rerender } = render(
      <AiChatPanel
        dbSessionId="c1"
        database="db"
        draftRequest={draft}
        onDraftConsumed={onDraftConsumed}
      />,
    );
    await waitFor(() => {
      expect(onDraftConsumed).toHaveBeenCalledTimes(1);
    });
    // Re-render with same draft — should not trigger again.
    rerender(
      <AiChatPanel
        dbSessionId="c1"
        database="db"
        draftRequest={draft}
        onDraftConsumed={onDraftConsumed}
      />,
    );
    // Still only 1 call.
    expect(onDraftConsumed).toHaveBeenCalledTimes(1);
  });
});
