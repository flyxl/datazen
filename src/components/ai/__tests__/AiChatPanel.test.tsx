import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { AiChatPanel, QuestionBlock } from '../AiChatPanel';

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
  WorkflowPanel: ({ connectionId }: { connectionId: string }) => (
    <div data-testid="workflow-panel">{connectionId}</div>
  ),
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
      <button type="button" data-testid="chat-send" onClick={onSubmit}>send</button>
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
  aiState.chatSession = { messages: [], isStreaming: false, streamContent: '', streamReasoning: '' };
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: { writeText: vi.fn().mockResolvedValue(undefined) },
  });
});

describe('AiChatPanel', () => {
  it('shows not configured empty state', () => {
    aiState.isConfigured = false;
    const { getByText } = render(<AiChatPanel connectionId="c1" database="db" />);
    fireEvent.click(getByText('settings.ai.goToConfigure'));
    expect(openSettingsWindow).toHaveBeenCalledWith('ai');
  });

  it('initializes chat session and shows welcome', () => {
    aiState.chatSession = null as unknown as typeof aiState.chatSession;
    const { getByText, rerender } = render(<AiChatPanel connectionId="c1" />);
    expect(aiState.initChatSession).toHaveBeenCalled();
    aiState.chatSession = { messages: [], isStreaming: false, streamContent: '', streamReasoning: '' };
    rerender(<AiChatPanel connectionId="c1" />);
    expect(getByText('chat.welcome')).toBeInTheDocument();
  });

  it('sends message from input', () => {
    const { getByTestId } = render(<AiChatPanel connectionId="c1" database="db" />);
    fireEvent.change(getByTestId('chat-input'), { target: { value: 'hello' } });
    fireEvent.click(getByTestId('chat-send'));
    expect(aiState.sendChatMessage).toHaveBeenCalledWith({
      connectionId: 'c1',
      database: 'db',
      content: 'hello',
      contextFiles: undefined,
      contextTables: undefined,
    });
  });

  it('renders messages with code blocks and insert SQL', () => {
    const onInsertSql = vi.fn();
    aiState.chatSession.messages = [
      { role: 'user', content: 'help' },
      {
        role: 'assistant',
        content: 'Try:\n```sql\nSELECT 1\n```',
        reasoning: 'thinking...',
      },
    ];
    const { getByText, container } = render(
      <AiChatPanel connectionId="c1" onInsertSql={onInsertSql} />,
    );
    expect(getByText('help')).toBeInTheDocument();
    fireEvent.click(getByText('chat.reasoning'));
    expect(getByText('thinking...')).toBeInTheDocument();
    fireEvent.click(getByText('chat.insertSql'));
    expect(onInsertSql).toHaveBeenCalledWith('SELECT 1');
    const copyBtn = getByText('chat.insertSql')
      .closest('span')!
      .querySelector('button')!;
    fireEvent.click(copyBtn);
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith('SELECT 1');
  });

  it('shows streaming states', () => {
    aiState.chatSession.isStreaming = true;
    aiState.chatSession.streamContent = 'partial';
    const { getByText, rerender } = render(<AiChatPanel connectionId="c1" />);
    expect(getByText('partial')).toBeInTheDocument();

    aiState.chatSession.streamContent = '';
    aiState.chatSession.streamReasoning = '';
    rerender(<AiChatPanel connectionId="c1" />);
    expect(getByText('chat.thinking')).toBeInTheDocument();
  });

  it('switches to workflows tab and clears chat', () => {
    aiState.chatSession.messages = [{ role: 'user', content: 'x' }];
    const { getByText, getByTestId } = render(<AiChatPanel connectionId="c1" />);
    fireEvent.click(getByText('workflows.title'));
    expect(getByTestId('workflow-panel')).toHaveTextContent('c1');
    fireEvent.click(getByText('chat.title'));
    const clearBtn = Array.from(document.querySelectorAll('button')).find(
      (b) => !b.disabled && b.querySelector('.lucide-trash2'),
    )!;
    fireEvent.click(clearBtn);
    expect(aiState.clearChat).toHaveBeenCalled();
    fireEvent.click(getByText('chat.title'));
    const docsBtn = Array.from(document.querySelectorAll('button')).find((b) =>
      b.getAttribute('title') === 'docs.openAiHelp',
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
