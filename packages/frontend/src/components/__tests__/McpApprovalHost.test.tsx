import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { McpApprovalHost } from '../McpApprovalHost';
import type { McpApprovalRequest } from '../../types';

const { listeners, respondMock, cancelMock } = vi.hoisted(() => ({
  listeners: new Map<string, (payload?: unknown) => void>(),
  respondMock: vi.fn(async () => true),
  cancelMock: vi.fn(async () => true),
}));

vi.mock('../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../platform/contracts', () => ({
  requirePlatform: () => ({
    events: {
      listenCrossWindow: async (event: string, handler: (payload?: unknown) => void) => {
        listeners.set(event, handler);
        return () => listeners.delete(event);
      },
    },
  }),
}));

vi.mock('../../lib/windowKind', () => ({ getWindowKind: () => 'main' }));

vi.mock('../../commands/ai', () => ({
  aiCommands: {
    mcpRespondApproval: respondMock,
    mcpCancelApproval: cancelMock,
  },
}));

function approval(overrides: Partial<McpApprovalRequest> = {}): McpApprovalRequest {
  const now = Date.now();
  return {
    requestId: 'req-1',
    tool: 'query',
    connectionId: 'conn-1',
    database: 'analytics',
    namespace: 'public',
    sql: 'DELETE FROM users WHERE id = 42',
    arguments: { connection_id: 'conn-1', sql: 'DELETE FROM users WHERE id = 42' },
    createdAt: new Date(now).toISOString(),
    expiresAt: new Date(now + 5 * 60_000).toISOString(),
    timeoutMs: 300_000,
    ...overrides,
  };
}

async function emit(request: McpApprovalRequest) {
  await waitFor(() => expect(listeners.get('mcp:approval-requested')).toBeTypeOf('function'));
  listeners.get('mcp:approval-requested')?.(request);
}

describe('McpApprovalHost', () => {
  beforeEach(() => {
    listeners.clear();
    respondMock.mockClear();
    cancelMock.mockClear();
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it('shows the exact SQL, target, and arguments from the real event', async () => {
    render(<McpApprovalHost />);
    await emit(approval());
    expect(await screen.findByText('DELETE FROM users WHERE id = 42')).toBeTruthy();
    expect(screen.getByText('conn-1')).toBeTruthy();
    expect(screen.getByText('analytics')).toBeTruthy();
    expect(screen.getByText('public')).toBeTruthy();
    expect(screen.getByText(/connection_id/)).toBeTruthy();
  });

  it('allows exactly once and closes only that request', async () => {
    render(<McpApprovalHost />);
    await emit(approval());
    fireEvent.click(await screen.findByText('mcp.approval.allowOnce'));
    await waitFor(() => expect(respondMock).toHaveBeenCalledWith('req-1', 'allow_once'));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  });

  it('denies without executing and reports a failed IPC instead of closing', async () => {
    respondMock.mockRejectedValueOnce(new Error('ipc down'));
    render(<McpApprovalHost />);
    await emit(approval());
    fireEvent.click(await screen.findByText('mcp.approval.deny'));
    await waitFor(() => expect(respondMock).toHaveBeenCalledWith('req-1', 'deny'));
    expect(await screen.findByText('ipc down')).toBeTruthy();
    expect(screen.getByRole('dialog')).toBeTruthy();
  });

  it('stops a pending approval through the cancel command', async () => {
    render(<McpApprovalHost />);
    await emit(approval());
    fireEvent.click(await screen.findByText('mcp.approval.stop'));
    await waitFor(() => expect(cancelMock).toHaveBeenCalledWith('req-1'));
  });

  it('treats an expired request as timeout and disables Allow once', async () => {
    render(<McpApprovalHost />);
    await emit(approval({ expiresAt: new Date(Date.now() - 1000).toISOString() }));
    await waitFor(() => expect(respondMock).toHaveBeenCalledWith('req-1', 'timeout'));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(screen.queryByText('mcp.approval.allowOnce')).toBeNull();
  });
});
