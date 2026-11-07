import { beforeEach, describe, expect, it, vi } from 'vitest';

const mockAiCommands = vi.hoisted(() => ({
  cancel: vi.fn(),
  sessionsList: vi.fn().mockResolvedValue([]),
  sessionsGet: vi.fn(),
  sessionsUpsert: vi.fn().mockResolvedValue({}),
  sessionsDelete: vi.fn(),
  sessionsExport: vi.fn(),
}));

vi.mock('../../commands/ai', () => ({ aiCommands: mockAiCommands }));
vi.mock('../settingsStore', () => ({
  useSettingsStore: {
    getState: () => ({ settings: { mcpClientServers: [] }, updateSettings: vi.fn() }),
  },
}));

import type { AiStore } from '../ai/types';
import type { AiChatSession } from '../../types';

function session(requestId: string | null): AiChatSession {
  return {
    id: 's1',
    sessionKey: 'conn-1::db',
    connectionId: 'conn-1',
    database: 'db',
    messages: [{ id: 'm1', role: 'user', content: 'hello', status: 'complete' }],
    isStreaming: requestId !== null,
    streamContent: '',
    streamReasoning: '',
    streamMcpToolName: null,
    egressSummary: null,
    streamMemoryUsed: null,
    memoryUsed: null,
    requestId,
    attribution: null,
  };
}

describe('AI session terminal persistence', () => {
  let useAiStore: typeof import('../aiStore').useAiStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    localStorage.clear();
    mockAiCommands.sessionsList.mockResolvedValue([]);
    mockAiCommands.sessionsUpsert.mockResolvedValue({});
    const mod = await import('../aiStore');
    useAiStore = mod.useAiStore;
    useAiStore.setState({
      chatSession: null,
      chatSessions: [],
      sessionMigrationError: null,
      sessionMigrationSuccess: 0,
      sessionHistoryError: null,
    });
  });

  it('persists a cancel-timeout message as cancelled', async () => {
    useAiStore.setState({ chatSession: session('r1') });
    useAiStore.getState().handleCancelTimeout('r1');

    expect(useAiStore.getState().chatSession?.messages[1].status).toBe('cancelled');
    await vi.waitFor(() => expect(mockAiCommands.sessionsUpsert).toHaveBeenCalled());
    const payload = mockAiCommands.sessionsUpsert.mock.calls[0][0];
    expect(payload.messages[1]).toEqual(
      expect.objectContaining({ role: 'assistant', status: 'cancelled' }),
    );
  });

  it('replaces a stale local session with the latest AppDb session', async () => {
    localStorage.setItem(
      'datazen.aiSessions.v1',
      JSON.stringify({
        'conn-1::db': {
          id: 'local-stale',
          messages: [{ role: 'user', content: 'stale' }],
          lastAccess: 1,
        },
      }),
    );
    mockAiCommands.sessionsUpsert.mockResolvedValue({});
    mockAiCommands.sessionsList.mockResolvedValue([
      {
        id: 'backend-latest',
        sessionKey: 'conn-1::db',
        connectionId: 'conn-1',
        database: 'db',
        title: 'Latest',
        metadata: null,
        messageCount: 1,
        preview: 'fresh',
        createdAt: '2026-01-01T00:00:00Z',
        updatedAt: '2026-01-01T00:00:01Z',
        lastAccessAt: '2026-01-01T00:00:01Z',
      },
    ]);
    mockAiCommands.sessionsGet.mockResolvedValue({
      id: 'backend-latest',
      sessionKey: 'conn-1::db',
      connectionId: 'conn-1',
      database: 'db',
      title: 'Latest',
      metadata: null,
      messageCount: 1,
      preview: 'fresh',
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-01-01T00:00:01Z',
      lastAccessAt: '2026-01-01T00:00:01Z',
      messages: [
        {
          id: 'm1',
          role: 'user',
          content: 'fresh',
          status: 'complete',
          metadata: null,
          createdAt: '2026-01-01T00:00:00Z',
        },
      ],
    });

    await useAiStore.getState().initChatSession('db-sess', 'db', 'conn-1');

    expect(useAiStore.getState().chatSession?.id).toBe('backend-latest');
    expect(useAiStore.getState().chatSession?.messages[0].content).toBe('fresh');
    expect(useAiStore.getState().sessionMigrationSuccess).toBe(1);
  });

  it('persists a stream error message as failed', async () => {
    useAiStore.setState({ chatSession: session('r2') });
    const { handleStreamError } = await import('../ai/streaming');
    handleStreamError(
      useAiStore.getState,
      useAiStore.setState as unknown as (partial: Partial<AiStore>) => void,
      { requestId: 'r2', error: 'boom' },
    );

    expect(useAiStore.getState().chatSession?.messages[1].status).toBe('failed');
    await vi.waitFor(() => expect(mockAiCommands.sessionsUpsert).toHaveBeenCalled());
    const payload = mockAiCommands.sessionsUpsert.mock.calls[0][0];
    expect(payload.messages[1]).toEqual(
      expect.objectContaining({ role: 'assistant', status: 'failed' }),
    );
  });
});
