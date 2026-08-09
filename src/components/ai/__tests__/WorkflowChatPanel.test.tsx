import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { WorkflowChatPanel } from '../WorkflowChatPanel';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const openSettingsWindow = vi.fn();
vi.mock('../../../lib/windowManager', () => ({
  openSettingsWindow: (...args: unknown[]) => openSettingsWindow(...args),
}));

vi.mock('../AiInput', () => ({
  AiInput: ({
    value,
    onChange,
    onSubmit,
    onStop,
    isLoading,
  }: {
    value: string;
    onChange: (v: string) => void;
    onSubmit: () => void;
    onStop?: () => void;
    isLoading?: boolean;
  }) => (
    <div>
      <textarea data-testid="wf-chat-input" value={value} onChange={(e) => onChange(e.target.value)} />
      <button type="button" data-testid="wf-chat-send" onClick={onSubmit}>send</button>
      {isLoading && onStop && <button type="button" data-testid="wf-chat-stop" onClick={onStop}>stop</button>}
    </div>
  ),
}));

vi.mock('../../ui/Select', () => ({
  Select: ({
    value,
    options,
    onChange,
  }: {
    value: string;
    options: { value: string; label: string }[];
    onChange: (v: string) => void;
  }) => (
    <select data-testid="conn-select" value={value} onChange={(e) => onChange(e.target.value)}>
      {options.map((o) => (
        <option key={o.value} value={o.value}>{o.label}</option>
      ))}
    </select>
  ),
}));

const workflowSave = vi.fn().mockResolvedValue(undefined);

vi.mock('../../../commands/ai', () => ({
  aiCommands: {
    workflowSave: (...args: unknown[]) => workflowSave(...args),
  },
}));

const aiState = vi.hoisted(() => ({
  isConfigured: true,
  workflowChat: {
    messages: [] as { role: 'user' | 'assistant'; content: string; reasoning?: string; questions?: unknown[] }[],
    isStreaming: false,
    streamContent: '',
    streamReasoning: '',
    requestId: null as string | null,
  },
  initWorkflowChat: vi.fn(),
  sendWorkflowChatMessage: vi.fn().mockResolvedValue(undefined),
  clearWorkflowChat: vi.fn(),
}));

vi.mock('../../../stores/aiStore', () => ({
  useAiStore: Object.assign(
    (sel: (s: typeof aiState) => unknown) => sel(aiState),
    {
      setState: vi.fn((partial: Partial<typeof aiState> | ((s: typeof aiState) => Partial<typeof aiState>)) => {
        if (typeof partial === 'function') {
          Object.assign(aiState, partial(aiState));
        } else {
          Object.assign(aiState, partial);
        }
      }),
    },
  ),
}));

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  Element.prototype.scrollIntoView = vi.fn();
  aiState.isConfigured = true;
  aiState.workflowChat = {
    messages: [],
    isStreaming: false,
    streamContent: '',
    streamReasoning: '',
    requestId: null,
  };
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: { writeText: vi.fn().mockResolvedValue(undefined) },
  });
});

const CONNECTIONS = [
  { id: 'c1', name: 'PG', databaseType: 'postgresql', database: 'postgres' },
];

describe('WorkflowChatPanel', () => {
  it('shows not configured state', () => {
    aiState.isConfigured = false;
    const { getByText } = render(<WorkflowChatPanel connections={CONNECTIONS} />);
    fireEvent.click(getByText('settings.ai.goToConfigure'));
    expect(openSettingsWindow).toHaveBeenCalledWith('ai');
  });

  it('sends message with selected connection', () => {
    const { getByTestId } = render(<WorkflowChatPanel connections={CONNECTIONS} />);
    fireEvent.change(getByTestId('conn-select'), { target: { value: 'c1' } });
    fireEvent.change(getByTestId('wf-chat-input'), { target: { value: 'make workflow' } });
    fireEvent.click(getByTestId('wf-chat-send'));
    expect(aiState.sendWorkflowChatMessage).toHaveBeenCalledWith({
      connectionId: 'c1',
      content: 'make workflow',
      includeSchema: true,
      contextFiles: undefined,
      contextTables: undefined,
    });
  });

  it('handles new chat and back navigation', () => {
    const onBack = vi.fn();
    const onSaved = vi.fn();
    const { getByText } = render(
      <WorkflowChatPanel connections={CONNECTIONS} onBack={onBack} onSaved={onSaved} />,
    );
    fireEvent.click(getByText('chat.clear'));
    expect(aiState.clearWorkflowChat).toHaveBeenCalled();
    expect(aiState.initWorkflowChat).toHaveBeenCalled();
    fireEvent.click(document.querySelector('button')!);
    expect(onBack).toHaveBeenCalled();
  });

  it('renders yaml block actions and saves workflow', async () => {
    const yaml = 'id: wf1\nname: Test\nsteps:\n  - type: query\n    id: s1\n    sql: SELECT 1';
    aiState.workflowChat.messages = [
      { role: 'assistant', content: `\`\`\`yaml\n${yaml}\n\`\`\`` },
    ];
    const { getByText, getAllByText } = render(<WorkflowChatPanel connections={CONNECTIONS} onSaved={vi.fn()} />);
    fireEvent.click(getByText('workflows.aiCreate.preview'));
    expect(getAllByText(/id: wf1/).length).toBeGreaterThan(0);
    fireEvent.click(getByText('workflows.aiCreate.save'));
    await waitFor(() => expect(workflowSave).toHaveBeenCalled());
  });

  it('stops streaming and flushes partial content', () => {
    aiState.workflowChat.isStreaming = true;
    aiState.workflowChat.streamContent = 'partial yaml';
    aiState.workflowChat.streamReasoning = 'reason';
    const { getByTestId } = render(<WorkflowChatPanel connections={CONNECTIONS} />);
    fireEvent.click(getByTestId('wf-chat-stop'));
    expect(aiState.workflowChat.isStreaming).toBe(false);
  });

  it('shows welcome and streaming thinking state', () => {
    const { getByText, rerender } = render(<WorkflowChatPanel connections={CONNECTIONS} />);
    expect(getByText('workflows.aiCreate.welcome')).toBeInTheDocument();
    aiState.workflowChat.isStreaming = true;
    rerender(<WorkflowChatPanel connections={CONNECTIONS} />);
    expect(getByText('chat.thinking')).toBeInTheDocument();
  });
});
