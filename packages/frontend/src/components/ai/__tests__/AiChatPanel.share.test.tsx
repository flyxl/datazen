import { forwardRef } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { AiChatPanel } from '../AiChatPanel';
import type { AiGrantReceipt } from '../../../ai/targetClaim';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params && typeof params.error === 'string' ? `${key}:${params.error}` : key,
  }),
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

vi.mock('../../../lib/windowManager', () => ({
  openSettingsWindow: vi.fn(),
  openDocsWindow: vi.fn(),
}));

vi.mock('../WorkflowPanel', () => ({
  WorkflowPanel: () => <div data-testid="workflow-panel" />,
}));

vi.mock('../../SqlCodeBlock', () => ({
  SqlCodeBlock: () => <div />,
}));

vi.mock('../AiInput', () => ({
  AiInput: forwardRef<
    HTMLTextAreaElement,
    {
      value: string;
      onChange: (v: string) => void;
      onSubmit: () => void;
      onStop?: () => void;
      disabled?: boolean;
    }
  >(function AiInput({ value, onChange, onSubmit, onStop, disabled }, ref) {
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
        <button type="button" data-testid="chat-stop" onClick={onStop}>
          stop
        </button>
      </div>
    );
  }),
}));

const aiState = vi.hoisted(() => ({
  isConfigured: true,
  chatSession: {
    messages: [] as { role: 'user' | 'assistant'; content: string }[],
    isStreaming: false,
    streamContent: '',
    streamReasoning: '',
    streamMcpToolName: null as string | null,
    requestId: null as string | null,
  },
  initChatSession: vi.fn(),
  sendChatMessage: vi.fn().mockResolvedValue(undefined),
  clearChat: vi.fn(),
  cancelChat: vi.fn().mockResolvedValue(null),
  settingsConfig: null,
  setActiveProfile: vi.fn().mockResolvedValue(undefined),
  loadConfig: vi.fn(),
}));

vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (sel: (s: typeof aiState) => unknown) => sel(aiState),
}));

const grantEditorSession = vi.hoisted(() => vi.fn());
const revokeEditorGrant = vi.hoisted(() => vi.fn());
const touchEditorGrant = vi.hoisted(() => vi.fn());

vi.mock('../../../commands/ai', () => ({
  aiCommands: {
    grantEditorSession: (...args: unknown[]) => grantEditorSession(...args),
    revokeEditorGrant: (...args: unknown[]) => revokeEditorGrant(...args),
    touchEditorGrant: (...args: unknown[]) => touchEditorGrant(...args),
  },
}));

const ensureSession = vi.hoisted(() => vi.fn());
const peek = vi.hoisted(() => vi.fn());
const queryPanelClientInstanceId = vi.hoisted(() => vi.fn());

vi.mock('../../../lib/session/queryPanelSessionRoot', () => ({
  getQueryPanelSessionRegistry: () => ({ peek: (...args: unknown[]) => peek(...args) }),
  queryPanelClientInstanceId: (...args: unknown[]) => queryPanelClientInstanceId(...args),
}));

function receipt(overrides: Partial<AiGrantReceipt> = {}): AiGrantReceipt {
  return {
    grantId: 'grant-1',
    connectionId: 'conn-1',
    database: 'db',
    configRevision: 7,
    expiresAtMs: Date.now() + 10_000,
    ...overrides,
  };
}

function renderPanel(overrides: Partial<Parameters<typeof AiChatPanel>[0]> = {}) {
  return render(
    <AiChatPanel
      dbSessionId="db-sess"
      connectionId="conn-1"
      database="db"
      editorSessionId="panel-1"
      {...overrides}
    />,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  aiState.isConfigured = true;
  aiState.chatSession = {
    messages: [],
    isStreaming: false,
    streamContent: '',
    streamReasoning: '',
    streamMcpToolName: null,
    requestId: null,
  };
  ensureSession.mockResolvedValue({
    handle: { dbSessionId: 'db-sess', runtimeEpoch: 'epoch-1' },
  });
  peek.mockReturnValue({ ensureSession });
  queryPanelClientInstanceId.mockResolvedValue('client-99');
  grantEditorSession.mockResolvedValue(receipt());
  revokeEditorGrant.mockResolvedValue(true);
  touchEditorGrant.mockResolvedValue(true);
});

afterEach(cleanup);

describe('AiChatPanel — shared editor session (P6 §14.4)', () => {
  it('defaults to independent and sends an independent claim', () => {
    const { getByTestId } = renderPanel();
    const toggle = getByTestId('ai-share-editor-toggle') as HTMLInputElement;
    expect(toggle.checked).toBe(false);

    fireEvent.change(getByTestId('chat-input'), { target: { value: 'hello' } });
    fireEvent.click(getByTestId('chat-send'));

    expect(aiState.sendChatMessage).toHaveBeenCalledTimes(1);
    expect(aiState.sendChatMessage.mock.calls[0][0]).toMatchObject({
      content: 'hello',
      targetClaim: { kind: 'independent' },
    });
    expect(grantEditorSession).not.toHaveBeenCalled();
  });

  it('grants the pane session on enable and attaches the resulting claim', async () => {
    const { getByTestId } = renderPanel();
    const toggle = getByTestId('ai-share-editor-toggle') as HTMLInputElement;

    fireEvent.click(toggle);

    await waitFor(() => expect(grantEditorSession).toHaveBeenCalledTimes(1));
    expect(peek).toHaveBeenCalledWith('panel-1');
    expect(ensureSession).toHaveBeenCalledTimes(1);
    expect(queryPanelClientInstanceId).toHaveBeenCalledTimes(1);
    expect(grantEditorSession).toHaveBeenCalledWith({
      connectionId: 'conn-1',
      database: 'db',
      editorSessionId: 'panel-1',
      claimedClientInstanceId: 'client-99',
      sessionHandle: { dbSessionId: 'db-sess', runtimeEpoch: 'epoch-1' },
    });
    await waitFor(() => expect(toggle.checked).toBe(true));

    fireEvent.change(getByTestId('chat-input'), { target: { value: 'hello' } });
    fireEvent.click(getByTestId('chat-send'));

    expect(aiState.sendChatMessage).toHaveBeenCalledTimes(1);
    expect(aiState.sendChatMessage.mock.calls[0][0]).toMatchObject({
      targetClaim: {
        kind: 'editorGrant',
        grantId: 'grant-1',
        claimedClientInstanceId: 'client-99',
        editorSessionId: 'panel-1',
      },
    });
  });

  it('shows the rejection and stays independent (no silent downgrade)', async () => {
    grantEditorSession.mockRejectedValue(new Error('claim rejected by host'));
    const { getByTestId, queryByTestId } = renderPanel();
    const toggle = getByTestId('ai-share-editor-toggle') as HTMLInputElement;

    fireEvent.click(toggle);

    await waitFor(() =>
      expect(getByTestId('ai-share-editor-error').textContent).toContain(
        'claim rejected by host',
      ),
    );
    expect(toggle.checked).toBe(false);
    expect(queryByTestId('ai-share-editor-error')).not.toBeNull();

    fireEvent.change(getByTestId('chat-input'), { target: { value: 'hello' } });
    fireEvent.click(getByTestId('chat-send'));

    expect(aiState.sendChatMessage.mock.calls[0][0]).toMatchObject({
      targetClaim: { kind: 'independent' },
    });
  });

  it('revokes the grant when switched off', async () => {
    const { getByTestId } = renderPanel();
    const toggle = getByTestId('ai-share-editor-toggle') as HTMLInputElement;

    fireEvent.click(toggle);
    await waitFor(() => expect(toggle.checked).toBe(true));

    fireEvent.click(toggle);
    await waitFor(() => expect(revokeEditorGrant).toHaveBeenCalledWith('grant-1'));
    await waitFor(() => expect(toggle.checked).toBe(false));
  });

  it('cancels the in-flight run on unmount', () => {
    aiState.chatSession.requestId = 'req-9';
    aiState.chatSession.isStreaming = true;
    const { unmount } = renderPanel();

    unmount();

    expect(aiState.cancelChat).toHaveBeenCalledWith('req-9');
  });

  it('cancels the in-flight run when the pane changes', () => {
    aiState.chatSession.requestId = 'req-9';
    aiState.chatSession.isStreaming = true;
    const { rerender } = renderPanel();
    rerender(
      <AiChatPanel
        dbSessionId="db-sess"
        connectionId="conn-1"
        database="db"
        editorSessionId="panel-2"
      />,
    );

    expect(aiState.cancelChat).toHaveBeenCalledWith('req-9');
  });

  it('stop asks the store to cancel instead of invoking ai_cancel directly', () => {
    aiState.chatSession.requestId = 'req-7';
    aiState.chatSession.isStreaming = true;
    const { getByTestId } = renderPanel();

    fireEvent.click(getByTestId('chat-stop'));

    expect(aiState.cancelChat).toHaveBeenCalledWith('req-7');
  });

  it('disables sharing when the active tab has no editor session', () => {
    const { getByTestId } = renderPanel({ editorSessionId: undefined });
    const toggle = getByTestId('ai-share-editor-toggle') as HTMLInputElement;
    expect(toggle.disabled).toBe(true);
    expect(toggle.checked).toBe(false);
  });
});
