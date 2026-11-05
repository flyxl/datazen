import { beforeEach, describe, expect, it, vi } from 'vitest';

const mockAiCommands = vi.hoisted(() => ({
  sessionsList: vi.fn(),
  sessionsGet: vi.fn(),
  sessionsUpsert: vi.fn(),
  sessionsDelete: vi.fn(),
  sessionsExport: vi.fn(),
}));

vi.mock('../../../commands/ai', () => ({ aiCommands: mockAiCommands }));

import {
  chatSessionFromDetail,
  chatSessionToUpsertInput,
  exportChatSession,
  loadSessions,
  migrateLocalSessionsToBackend,
  persistChatSession,
  restoreChatSession,
} from '../sessions';
import type { AiChatSession, AiSessionDetail, AiSessionSummary } from '../../../types';

const summary: AiSessionSummary = {
  id: 's1',
  sessionKey: 'conn-1::db',
  connectionId: 'conn-1',
  database: 'db',
  title: 'Saved conversation',
  metadata: { templateId: 'default' },
  messageCount: 2,
  preview: 'hello',
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-01T00:00:01Z',
  lastAccessAt: '2026-01-01T00:00:01Z',
};

const detail: AiSessionDetail = {
  ...summary,
  messages: [
    {
      id: 'm1',
      role: 'user',
      content: 'hello',
      status: 'complete',
      metadata: { memoryUsed: [{ id: 'mem-1' }] },
      createdAt: '2026-01-01T00:00:00Z',
    },
    {
      id: 'm2',
      role: 'assistant',
      content: 'stopped',
      status: 'cancelled',
      metadata: { reasoning: 'thinking', cancelled: true },
      createdAt: '2026-01-01T00:00:01Z',
    },
    {
      id: 'm3',
      role: 'assistant',
      content: 'Error: boom',
      status: 'failed',
      metadata: null,
      createdAt: '2026-01-01T00:00:02Z',
    },
  ],
};

function session(): AiChatSession {
  return chatSessionFromDetail(detail);
}

beforeEach(() => {
  vi.clearAllMocks();
  mockAiCommands.sessionsList.mockReset();
  mockAiCommands.sessionsGet.mockReset();
  mockAiCommands.sessionsUpsert.mockReset().mockResolvedValue({});
  mockAiCommands.sessionsDelete.mockReset();
  mockAiCommands.sessionsExport.mockReset();
  const store: Record<string, string> = {};
  vi.stubGlobal('localStorage', {
    getItem: vi.fn((key: string) => store[key] ?? null),
    setItem: vi.fn((key: string, value: string) => {
      store[key] = value;
    }),
    removeItem: vi.fn((key: string) => {
      delete store[key];
    }),
    clear: vi.fn(() => {
      for (const key of Object.keys(store)) delete store[key];
    }),
  });
});

describe('backend-first session restore', () => {
  it('restores cancelled/failed messages and memory metadata from AppDb', async () => {
    mockAiCommands.sessionsList.mockResolvedValue([summary]);
    mockAiCommands.sessionsGet.mockResolvedValue(detail);

    const restored = await restoreChatSession('conn-1::db', {
      connectionId: 'conn-1',
      database: 'db',
    });

    expect(restored.backendAvailable).toBe(true);
    expect(restored.session?.id).toBe('s1');
    expect(restored.session?.messages[1].status).toBe('cancelled');
    expect(restored.session?.messages[2].status).toBe('failed');
    expect(restored.session?.messages[0].memoryUsed?.[0].id).toBe('mem-1');
    expect(restored.session?.messages[1].reasoning).toBe('thinking');
    expect(mockAiCommands.sessionsList).toHaveBeenCalledWith({
      connectionId: 'conn-1',
      database: 'db',
      sessionKey: 'conn-1::db',
    });
  });

  it('keeps a local-only conversation when the backend is reachable but empty', async () => {
    localStorage.setItem(
      'datazen.aiSessions.v1',
      JSON.stringify({
        'conn-1::db': {
          messages: [{ role: 'user', content: 'offline draft' }],
          lastAccess: 5,
        },
      }),
    );
    mockAiCommands.sessionsList.mockResolvedValue([]);
    mockAiCommands.sessionsUpsert.mockRejectedValue(new Error('write failed'));

    const restored = await restoreChatSession('conn-1::db', {
      connectionId: 'conn-1',
      database: 'db',
    });

    expect(restored.backendAvailable).toBe(true);
    expect(restored.session?.messages[0].content).toBe('offline draft');
  });

  it('falls back to localStorage without cross-connection bleed when backend fails', async () => {
    localStorage.setItem(
      'datazen.aiSessions.v1',
      JSON.stringify({
        'conn-1::db': {
          messages: [
            { role: 'user', content: 'one' },
            { role: 'assistant', content: 'stopped', cancelled: true },
          ],
          lastAccess: 5,
        },
        'conn-2::db': {
          messages: [{ role: 'user', content: 'other connection' }],
          lastAccess: 6,
        },
      }),
    );
    mockAiCommands.sessionsList.mockRejectedValue(new Error('offline'));

    const restored = await restoreChatSession('conn-1::db', {
      connectionId: 'conn-1',
      database: 'db',
    });

    expect(restored.backendAvailable).toBe(false);
    expect(restored.session?.messages.map((message) => message.content)).toEqual([
      'one',
      'stopped',
    ]);
    expect(restored.session?.messages[1].status).toBe('cancelled');
    expect(restored.migrationError).toContain('offline');
  });
});

describe('legacy localStorage migration', () => {
  it('upserts deterministic ids, preserves status, then marks the old key', async () => {
    localStorage.setItem(
      'datazen.aiSessions.v1',
      JSON.stringify({
        'conn-1::db': {
          messages: [
            { role: 'user', content: 'hello' },
            { role: 'assistant', content: 'done', cancelled: true },
            { role: 'assistant', content: 'Error: x', status: 'failed' },
          ],
          lastAccess: 1000,
        },
      }),
    );
    mockAiCommands.sessionsUpsert.mockResolvedValue(detail);

    const first = await migrateLocalSessionsToBackend();
    expect(first.error).toBeNull();
    expect(first.migrated).toBe(1);
    expect(mockAiCommands.sessionsUpsert).toHaveBeenCalledWith(
      expect.objectContaining({
        id: 'legacy:conn-1::db',
        sessionKey: 'conn-1::db',
        connectionId: 'conn-1',
        database: 'db',
        messages: [
          expect.objectContaining({ status: 'complete' }),
          expect.objectContaining({ status: 'cancelled' }),
          expect.objectContaining({ status: 'failed' }),
        ],
      }),
    );
    expect(localStorage.getItem('datazen.aiSessions.v1')).toContain('hello');
    expect(localStorage.getItem('datazen.aiSessions.v1.migratedAt')).not.toBeNull();

    const second = await migrateLocalSessionsToBackend();
    expect(second.migrated).toBe(0);
    expect(mockAiCommands.sessionsUpsert).toHaveBeenCalledTimes(1);
  });

  it('keeps legacy data and reports failure when migration cannot write', async () => {
    localStorage.setItem(
      'datazen.aiSessions.v1',
      JSON.stringify({
        'conn-1::db': { messages: [{ role: 'user', content: 'hello' }], lastAccess: 1 },
      }),
    );
    mockAiCommands.sessionsUpsert.mockRejectedValue(new Error('write failed'));
    mockAiCommands.sessionsList.mockRejectedValue(new Error('offline'));

    const restored = await restoreChatSession('conn-1::db', {
      connectionId: 'conn-1',
      database: 'db',
    });

    expect(restored.migrationError).toContain('write failed');
    expect(restored.session?.messages[0].content).toBe('hello');
    expect(localStorage.getItem('datazen.aiSessions.v1.migratedAt')).toBeNull();
    expect(localStorage.getItem('datazen.aiSessions.v1')).toContain('hello');
  });
});

describe('persistence, export and message mapping', () => {
  it('maps the full chat session to an explicit replace payload', () => {
    const input = chatSessionToUpsertInput(session());
    expect(input.id).toBe('s1');
    expect(input.sessionKey).toBe('conn-1::db');
    expect(input.append).toBe(false);
    expect(input.messages[1]).toEqual(
      expect.objectContaining({ role: 'assistant', status: 'cancelled' }),
    );
    expect(input.messages[0].metadata).toEqual(
      expect.objectContaining({ memoryUsed: [{ id: 'mem-1' }] }),
    );
  });

  it('mirrors to localStorage when the backend write fails', async () => {
    mockAiCommands.sessionsUpsert.mockRejectedValue(new Error('db locked'));
    const result = await persistChatSession(session());

    expect(result.ok).toBe(false);
    expect(result.error).toContain('db locked');
    const local = loadSessions()['conn-1::db'];
    expect(local?.messages[1].status).toBe('cancelled');
    expect(local?.id).toBe('s1');
  });

  it('exports backend JSON/Markdown and falls back locally offline', async () => {
    mockAiCommands.sessionsExport.mockResolvedValueOnce('{"id":"s1"}');
    const json = await exportChatSession(session(), 'json');
    expect(json.content).toBe('{"id":"s1"}');
    expect(json.filename).toBe('Saved-conversation.json');

    mockAiCommands.sessionsExport.mockRejectedValueOnce(new Error('offline'));
    const markdown = await exportChatSession(session(), 'markdown');
    expect(markdown.content).toContain('## Assistant (cancelled)');
    expect(markdown.content).toContain('## Assistant (failed)');
    expect(markdown.filename).toBe('Saved-conversation.md');
  });
});
