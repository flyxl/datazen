import { forwardRef } from 'react';
import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { AiChatPanel, QuestionBlock } from '../AiChatPanel';
import { buildKvAiContext } from '../../../lib/kvAiContext';
import type { AiChatDraftRequest } from '../../../windows/connection/query/aiDraftBridge';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useAutoScroll', () => ({
  useAutoScroll: () => ({
    atBottom: true,
    unreadCount: 0,
    jumpToBottom: vi.fn(),
    onScroll: vi.fn(),
    containerRef: { current: null },
  }),
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
  AiInput: forwardRef<
    HTMLTextAreaElement,
    {
      value: string;
      onChange: (v: string) => void;
      onSubmit: () => void;
      disabled?: boolean;
    }
  >(function AiInput({ value, onChange, onSubmit, disabled }, ref) {
    return (
      <div>
        <textarea
          ref={ref}
          data-testid="chat-input"
          value={value}
          disabled={disabled}
          onChange={(e) => onChange(e.target.value)}
        />
        <button type="button" data-testid="chat-send" onClick={onSubmit}>
          send
        </button>
      </div>
    );
  }),
}));

const aiState = vi.hoisted(() => ({
  isConfigured: true,
  chatSession: {
    sessionKey: 'c1::',
    messages: [] as {
      role: 'user' | 'assistant' | 'tool';
      content: string;
      reasoning?: string;
      memoryUsed?: {
        reference: string;
        id: string;
        subject: string;
        kind: string;
        scope: string;
        score: number;
      }[];
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
    egressSummary: null as string | null,
    injection: null as null | {
      templateName?: string | null;
      hasConnectionNotes: boolean;
      hasGlobalInstructions: boolean;
    },
  },
  initChatSession: vi.fn(),
  sendChatMessage: vi.fn().mockResolvedValue(undefined),
  clearChat: vi.fn(),
  cancelChat: vi.fn().mockResolvedValue(null),
  settingsConfig: null as {
    activeProfileId: string;
    profiles: {
      id: string;
      name: string;
      model: string;
      isDefault?: boolean;
      safetyGate?: string;
    }[];
  } | null,
  setActiveProfile: vi.fn().mockResolvedValue(undefined),
  loadConfig: vi.fn(),
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
    sessionKey: 'c1::',
    messages: [],
    isStreaming: false,
    streamContent: '',
    streamReasoning: '',
    streamMcpToolName: null,
    egressSummary: null,
    injection: null,
  };
  aiState.settingsConfig = null;
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

  it('initializes chat session with the stable connection id and shows welcome', () => {
    aiState.chatSession = null as unknown as typeof aiState.chatSession;
    const { getByText, rerender } = render(
      <AiChatPanel dbSessionId="c1" database="db" connectionId="conn-1" />,
    );
    expect(aiState.initChatSession).toHaveBeenCalledWith('c1', 'db', 'conn-1');
    aiState.chatSession = {
      sessionKey: 'conn-1::db',
      messages: [],
      isStreaming: false,
      streamContent: '',
      streamReasoning: '',
      streamMcpToolName: null,
      egressSummary: null,
      injection: null,
    };
    rerender(<AiChatPanel dbSessionId="c1" database="db" connectionId="conn-1" />);
    expect(getByText('chat.welcome')).toBeInTheDocument();
  });

  it('renders an expandable memory chip and wires citation clicks', () => {
    aiState.chatSession = {
      sessionKey: 'c1::',
      messages: [
        {
          role: 'assistant',
          content: 'Use UTC [M1].',
          memoryUsed: [
            {
              reference: 'M1',
              id: 'mem-1',
              subject: 'Timezone preference',
              kind: 'preference',
              scope: 'user:me',
              score: 0.9,
            },
          ],
        },
      ],
      isStreaming: false,
      streamContent: '',
      streamReasoning: '',
      streamMcpToolName: null,
      egressSummary: null,
      injection: null,
    };
    const { getByTestId, getByText, queryByTestId } = render(
      <AiChatPanel dbSessionId="c1" database="db" />,
    );
    expect(getByTestId('ai-memory-chip')).toBeInTheDocument();
    expect(getByText('chat.memory.used')).toBeInTheDocument();
    expect(queryByTestId('ai-memory-expanded')).toBeNull();

    fireEvent.click(getByTestId('ai-memory-chip'));
    expect(getByTestId('ai-memory-expanded')).toBeInTheDocument();
    expect(getByText('Timezone preference')).toBeInTheDocument();

    fireEvent.click(getByTestId('ai-memory-chip'));
    expect(queryByTestId('ai-memory-expanded')).toBeNull();
    fireEvent.click(getByText('[M1]'));
    expect(getByTestId('ai-memory-expanded')).toBeInTheDocument();
  });

  it('hides the memory chip when no memories were used', () => {
    aiState.chatSession = {
      sessionKey: 'c1::',
      messages: [{ role: 'assistant', content: 'No memory here.' }],
      isStreaming: false,
      streamContent: '',
      streamReasoning: '',
      streamMcpToolName: null,
      egressSummary: null,
      injection: null,
    };
    const { queryByTestId } = render(<AiChatPanel dbSessionId="c1" database="db" />);
    expect(queryByTestId('ai-memory-chip')).toBeNull();
  });

  it('renders template and notes chips from the injection snapshot', () => {
    aiState.chatSession = {
      sessionKey: 'c1::',
      messages: [],
      isStreaming: false,
      streamContent: '',
      streamReasoning: '',
      streamMcpToolName: null,
      egressSummary: null,
      injection: {
        templateName: 'Postgres style',
        hasConnectionNotes: true,
        hasGlobalInstructions: true,
      },
    };
    const { getByTestId } = render(<AiChatPanel dbSessionId="c1" database="db" />);

    expect(getByTestId('ai-injection-template').textContent).toContain('chat.injection.template');
    expect(getByTestId('ai-injection-notes')).toBeDefined();
    expect(getByTestId('ai-injection-priority')).toBeDefined();
  });

  it('keeps the conversation when dbSessionId changes on the same connection', () => {
    aiState.chatSession.sessionKey = 'conn-1::db';
    const { rerender } = render(
      <AiChatPanel dbSessionId="db-a" database="db" connectionId="conn-1" />,
    );
    expect(aiState.initChatSession).not.toHaveBeenCalled();

    rerender(<AiChatPanel dbSessionId="db-b" database="db" connectionId="conn-1" />);
    expect(aiState.initChatSession).not.toHaveBeenCalled();
  });

  it('reinitializes when the connection id changes', () => {
    aiState.chatSession.sessionKey = 'conn-1::db';
    const { rerender } = render(
      <AiChatPanel dbSessionId="db-sess" database="db" connectionId="conn-1" />,
    );
    expect(aiState.initChatSession).not.toHaveBeenCalled();

    rerender(<AiChatPanel dbSessionId="db-sess" database="db" connectionId="conn-2" />);
    expect(aiState.initChatSession).toHaveBeenCalledWith('db-sess', 'db', 'conn-2');
  });

  it('sends message from input', () => {
    const { getByTestId } = render(<AiChatPanel dbSessionId="c1" database="db" />);
    fireEvent.change(getByTestId('chat-input'), { target: { value: 'hello' } });
    fireEvent.click(getByTestId('chat-send'));
    expect(aiState.sendChatMessage).toHaveBeenCalledWith({
      dbSessionId: 'c1',
      connectionId: undefined,
      database: 'db',
      agentMode: 'ask',
      content: 'hello',
      contextFiles: undefined,
      contextTables: undefined,
      // P6 §14.4: default target is independent unless the user shares a pane.
      targetClaim: { kind: 'independent' },
    });
  });

  it('defaults to Ask mode and shows the mode switch', () => {
    const { getByTestId } = render(<AiChatPanel dbSessionId="c1" database="db" />);
    expect(getByTestId('ai-agent-mode-ask')).toHaveAttribute('aria-pressed', 'true');
    expect(getByTestId('ai-agent-mode-agent')).toHaveAttribute('aria-pressed', 'false');
    expect(getByTestId('ai-agent-mode-hint')).toHaveTextContent('chat.mode.askHint');
  });

  it('sends the toggled Agent mode', () => {
    const { getByTestId } = render(<AiChatPanel dbSessionId="c1" database="db" />);
    fireEvent.click(getByTestId('ai-agent-mode-agent'));
    expect(getByTestId('ai-agent-mode-hint')).toHaveTextContent('chat.mode.agentHint');
    fireEvent.change(getByTestId('chat-input'), { target: { value: 'check live data' } });
    fireEvent.click(getByTestId('chat-send'));
    expect(aiState.sendChatMessage).toHaveBeenCalledWith(
      expect.objectContaining({ agentMode: 'agent', content: 'check live data' }),
    );
  });

  it('flips both mode buttons when switching Ask and Agent', () => {
    const { getByTestId } = render(<AiChatPanel dbSessionId="c1" database="db" />);
    fireEvent.click(getByTestId('ai-agent-mode-agent'));
    expect(getByTestId('ai-agent-mode-agent')).toHaveAttribute('aria-pressed', 'true');
    expect(getByTestId('ai-agent-mode-ask')).toHaveAttribute('aria-pressed', 'false');

    fireEvent.click(getByTestId('ai-agent-mode-ask'));
    expect(getByTestId('ai-agent-mode-ask')).toHaveAttribute('aria-pressed', 'true');
    expect(getByTestId('ai-agent-mode-agent')).toHaveAttribute('aria-pressed', 'false');
  });

  it('shows the host egress summary and bound database once the first chunk arrives', () => {
    aiState.chatSession.egressSummary = '3 connections (postgresql, mysql) accessible via tools.';
    const { getByTestId } = render(<AiChatPanel dbSessionId="c1" database="appdb" />);

    expect(getByTestId('ai-egress-summary')).toHaveTextContent(
      '3 connections (postgresql, mysql) accessible via tools.',
    );
    expect(getByTestId('ai-egress-database')).toHaveTextContent('chat.egressDatabase');
  });

  it('keeps tool output collapsed until the user expands it', () => {
    aiState.chatSession.messages = [{ role: 'tool', content: 'rows: 2' }];
    const { getByTestId, queryByText } = render(<AiChatPanel dbSessionId="c1" />);

    expect(queryByText('rows: 2')).toBeNull();
    fireEvent.click(getByTestId('tool-role-toggle'));
    expect(queryByText('rows: 2')).toBeInTheDocument();
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
    expect(onSubmit).toHaveBeenCalledWith('[q1] Pick one\nAlpha');
  });
});

describe('AiChatPanel — AI Draft Bridge (S3-B2)', () => {
  // Overrides describe a *draft*, not panel props: the panel's `draftRequest`
  // prop is typed `AiChatDraftRequest`, whose fields (`requestId`, `content`, …)
  // are the ones each case varies.
  const makeDraft = (overrides?: Partial<AiChatDraftRequest>) =>
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

/**
 * On a KV panel the assistant is told the host-owned facts about the key in
 * scope, and the user sees that before sending. The context is built with the
 * same source the drawer uses, so this cannot drift from `kvAiContext.ts`.
 */
describe('AiChatPanel — KV panel context', () => {
  const kvContext = buildKvAiContext({
    connectionName: 'KV Local',
    dbSessionId: 'sess-1',
    database: 'db7',
    selectedKey: 'user:42',
  });

  it('sends the question with the host fact block attached', () => {
    const { getByTestId } = render(
      <AiChatPanel dbSessionId="sess-1" database="db7" kvContext={kvContext} />,
    );
    fireEvent.change(getByTestId('chat-input'), { target: { value: 'why is this key big?' } });
    fireEvent.click(getByTestId('chat-send'));

    const payload = aiState.sendChatMessage.mock.calls[0]?.[0];
    expect(payload?.dbSessionId).toBe('sess-1');
    // The block rides inside the message content — there is no KV wire field, and
    // inventing one would mean a new backend command (out of this track).
    expect(typeof payload?.content).toBe('string');
    expect(payload?.content as string).toContain('selected_key=user:42');
    expect(payload?.content as string).toContain('connection=KV Local');
    expect(payload?.content as string).toMatch(/^why is this key big\?\n\n\[kv-context\]/);
  });

  it('shows the egress chip naming the key the block is about', () => {
    const { getByTestId } = render(
      <AiChatPanel dbSessionId="sess-1" database="db7" kvContext={kvContext} />,
    );
    const chip = getByTestId('ai-kv-context-chip');
    // Asserted through the data attribute + i18n key: the visible label is copy.
    expect(chip.getAttribute('data-key-name')).toBe('user:42');
    expect(chip.textContent).toContain('redis.ai.context.attached');
  });

  it('sends a plain message and shows no chip without KV context', () => {
    const { getByTestId, queryByTestId } = render(<AiChatPanel dbSessionId="sess-1" />);
    expect(queryByTestId('ai-kv-context-chip')).not.toBeInTheDocument();

    fireEvent.change(getByTestId('chat-input'), { target: { value: 'hello' } });
    fireEvent.click(getByTestId('chat-send'));
    expect(aiState.sendChatMessage).toHaveBeenCalledWith({
      dbSessionId: 'sess-1',
      connectionId: undefined,
      database: undefined,
      agentMode: 'ask',
      content: 'hello',
      contextFiles: undefined,
      contextTables: undefined,
      targetClaim: { kind: 'independent' },
    });
  });

  // [tester] The drawer stays open across a key switch (keep-alive tabs): every
  // send must carry the block for the key in scope *now*, and the chip must
  // name that key — not the one from when the panel was first opened.
  it('[tester] follows a key switch between sends', () => {
    const ctxOrders = buildKvAiContext({
      connectionName: 'KV Local',
      dbSessionId: 'sess-1',
      database: 'db7',
      selectedKey: 'orders:99',
    });
    const { getByTestId, rerender } = render(
      <AiChatPanel dbSessionId="sess-1" database="db7" kvContext={kvContext} />,
    );

    fireEvent.change(getByTestId('chat-input'), { target: { value: 'first' } });
    fireEvent.click(getByTestId('chat-send'));

    rerender(<AiChatPanel dbSessionId="sess-1" database="db7" kvContext={ctxOrders} />);
    expect(getByTestId('ai-kv-context-chip').getAttribute('data-key-name')).toBe('orders:99');

    fireEvent.change(getByTestId('chat-input'), { target: { value: 'second' } });
    fireEvent.click(getByTestId('chat-send'));

    const first = aiState.sendChatMessage.mock.calls[0]?.[0];
    const second = aiState.sendChatMessage.mock.calls[1]?.[0];
    expect(aiState.sendChatMessage).toHaveBeenCalledTimes(2);
    expect(first?.content as string).toContain('selected_key=user:42');
    expect(second?.content as string).toContain('selected_key=orders:99');
  });
});

describe('AiChatPanel automatic context snapshot', () => {
  const SNAPSHOT = {
    editorSql: 'SELECT * FROM orders',
    lastError: 'relation orders does not exist',
    resultPreview: {
      columns: ['id'],
      rows: [[1] as (string | number | boolean | null)[]],
      rowCount: 1,
      elapsedMs: 4,
    },
  };

  it('shows chips for the host snapshot and lets each be removed', () => {
    const { getByTestId, queryByTestId } = render(
      <AiChatPanel dbSessionId="c1" database="db" autoContext={SNAPSHOT} />,
    );

    expect(getByTestId('ai-auto-context-sql')).toHaveTextContent('chat.autoContext.sql');
    expect(getByTestId('ai-auto-context-error')).toHaveTextContent('chat.autoContext.error');
    expect(getByTestId('ai-auto-context-result')).toHaveTextContent('chat.autoContext.result');

    fireEvent.click(getByTestId('ai-auto-context-sql-remove'));
    expect(queryByTestId('ai-auto-context-sql')).toBeNull();
    expect(getByTestId('ai-auto-context-error')).toBeInTheDocument();
  });

  it('sends only the remaining chips and freezes them into the request', () => {
    const { getByTestId } = render(
      <AiChatPanel dbSessionId="c1" database="db" autoContext={SNAPSHOT} />,
    );
    fireEvent.click(getByTestId('ai-auto-context-sql-remove'));
    fireEvent.change(getByTestId('chat-input'), { target: { value: 'why did this fail?' } });
    fireEvent.click(getByTestId('chat-send'));

    expect(aiState.sendChatMessage).toHaveBeenCalledWith(
      expect.objectContaining({
        autoContext: {
          lastError: SNAPSHOT.lastError,
          resultPreview: SNAPSHOT.resultPreview,
        },
      }),
    );
    // The chips are consumed by the send; the next turn starts from fresh host facts.
    expect(document.querySelector('[data-testid="ai-auto-context-chips"]')).toBeNull();
  });

  it('shows, removes, and sends the selection chip', () => {
    const { getByTestId, queryByTestId, rerender } = render(
      <AiChatPanel dbSessionId="c1" database="db" autoContext={{ selection: 'WHERE id=1' }} />,
    );
    expect(getByTestId('ai-auto-context-selection')).toHaveTextContent(
      'chat.autoContext.selection',
    );

    fireEvent.change(getByTestId('chat-input'), { target: { value: 'explain' } });
    fireEvent.click(getByTestId('chat-send'));
    expect(aiState.sendChatMessage).toHaveBeenLastCalledWith(
      expect.objectContaining({ autoContext: { selection: 'WHERE id=1' } }),
    );

    // A changed host selection re-arms the chip; removing it then sends plain.
    rerender(
      <AiChatPanel dbSessionId="c1" database="db" autoContext={{ selection: 'WHERE id=2' }} />,
    );
    fireEvent.click(getByTestId('ai-auto-context-selection-remove'));
    expect(queryByTestId('ai-auto-context-selection')).toBeNull();
    fireEvent.change(getByTestId('chat-input'), { target: { value: 'plain' } });
    fireEvent.click(getByTestId('chat-send'));
    expect(aiState.sendChatMessage).toHaveBeenLastCalledWith(
      expect.not.objectContaining({ autoContext: expect.anything() }),
    );
  });

  it('does not resurrect a removed chip on an ordinary rerender', () => {
    const { getByTestId, queryByTestId, rerender } = render(
      <AiChatPanel dbSessionId="c1" database="db" autoContext={SNAPSHOT} />,
    );
    fireEvent.click(getByTestId('ai-auto-context-sql-remove'));
    expect(queryByTestId('ai-auto-context-sql')).toBeNull();

    rerender(
      <AiChatPanel
        dbSessionId="c1"
        database="db"
        autoContext={{ ...SNAPSHOT, resultPreview: { ...SNAPSHOT.resultPreview } }}
      />,
    );
    expect(queryByTestId('ai-auto-context-sql')).toBeNull();
  });

  it('keeps the sent snapshot frozen when the host changes afterwards', () => {
    const { getByTestId, rerender } = render(
      <AiChatPanel dbSessionId="c1" database="db" autoContext={{ editorSql: 'SELECT 1' }} />,
    );
    fireEvent.change(getByTestId('chat-input'), { target: { value: 'first' } });
    fireEvent.click(getByTestId('chat-send'));
    const sent = aiState.sendChatMessage.mock.calls[0]?.[0];

    rerender(
      <AiChatPanel dbSessionId="c1" database="db" autoContext={{ editorSql: 'SELECT 2' }} />,
    );
    expect(sent?.autoContext).toEqual({ editorSql: 'SELECT 1' });
  });

  it('does not attach an empty snapshot', () => {
    const { getByTestId } = render(<AiChatPanel dbSessionId="c1" database="db" />);
    fireEvent.change(getByTestId('chat-input'), { target: { value: 'hello' } });
    fireEvent.click(getByTestId('chat-send'));
    expect(aiState.sendChatMessage).toHaveBeenCalledWith(
      expect.not.objectContaining({ autoContext: expect.anything() }),
    );
  });
});
