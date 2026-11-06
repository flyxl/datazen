import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { AiWriteProposalHost } from '../AiWriteProposalHost';
import type { AiWriteProposal } from '../../types';

const { listeners, applyMock, denyMock } = vi.hoisted(() => ({
  listeners: new Map<string, (payload?: unknown) => void>(),
  applyMock: vi.fn(async () => ({
    proposalId: 'proposal-1',
    connectionId: 'conn-1',
    database: 'app',
    sqlHash: 'a'.repeat(64),
    rowCount: 3,
    result: {},
  })),
  denyMock: vi.fn(async () => true),
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
    aiApplyWrite: applyMock,
    aiDenyWrite: denyMock,
  },
}));

function proposal(overrides: Partial<AiWriteProposal> = {}): AiWriteProposal {
  const now = Date.now();
  return {
    proposalId: 'proposal-1',
    token: 'proposal-1',
    connectionId: 'conn-1',
    database: 'app',
    sql: "UPDATE users SET role = 'admin' WHERE id = 42",
    sqlHash: 'a'.repeat(64),
    createdAt: new Date(now).toISOString(),
    expiresAt: new Date(now + 5 * 60_000).toISOString(),
    statements: [
      {
        index: 0,
        verb: 'UPDATE',
        table: 'users',
        hasWhere: true,
        destructive: false,
        sqlHash: 'b'.repeat(64),
      },
    ],
    impact: {
      statementCount: 1,
      tables: ['users'],
      hasWhere: true,
      destructive: false,
      writesData: true,
      schemaChanges: false,
      summary: '1 statement(s): UPDATE',
    },
    productionProtected: false,
    applyAllowed: true,
    ...overrides,
  };
}

async function emit(value: AiWriteProposal) {
  await waitFor(() => expect(listeners.get('ai:write-proposal-requested')).toBeTypeOf('function'));
  listeners.get('ai:write-proposal-requested')?.(value);
}

describe('AiWriteProposalHost', () => {
  beforeEach(() => {
    listeners.clear();
    applyMock.mockClear();
    denyMock.mockClear();
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it('shows exact SQL, target, hash, diff and static impact', async () => {
    render(<AiWriteProposalHost />);
    await emit(proposal({ diff: '-UPDATE users SET role = 1\n+UPDATE users SET role = 2' }));
    expect(await screen.findByText("UPDATE users SET role = 'admin' WHERE id = 42")).toBeTruthy();
    expect(screen.getByTestId('ai-write-proposal-sql').textContent).toContain('WHERE id = 42');
    expect(screen.getByTestId('ai-write-proposal-connection').textContent).toBe('conn-1');
    expect(screen.getByTestId('ai-write-proposal-database').textContent).toBe('app');
    expect(screen.getByTestId('ai-write-proposal-summary').textContent).toContain('UPDATE');
    expect(screen.getByTestId('ai-write-proposal-diff').textContent).toContain('role = 2');
    expect(screen.getByText('a'.repeat(64))).toBeTruthy();
  });

  it('Allow once applies the exact proposal fields and reports row count', async () => {
    render(<AiWriteProposalHost />);
    await emit(proposal());
    fireEvent.click(await screen.findByTestId('ai-write-proposal-allow'));
    await waitFor(() => expect(applyMock).toHaveBeenCalledTimes(1));
    expect(applyMock).toHaveBeenCalledWith({
      proposalId: 'proposal-1',
      connectionId: 'conn-1',
      database: 'app',
      sql: "UPDATE users SET role = 'admin' WHERE id = 42",
    });
    expect(await screen.findByTestId('ai-write-applied-receipt')).toBeTruthy();
  });

  it('Deny never calls apply and removes the proposal', async () => {
    render(<AiWriteProposalHost />);
    await emit(proposal());
    fireEvent.click(await screen.findByTestId('ai-write-proposal-deny'));
    await waitFor(() => expect(denyMock).toHaveBeenCalledWith('proposal-1'));
    expect(applyMock).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.queryByTestId('ai-write-proposal-dialog')).toBeNull());
  });

  it('production proposals show the warning and cannot be allowed', async () => {
    render(<AiWriteProposalHost />);
    await emit(proposal({ productionProtected: true, applyAllowed: false }));
    expect(await screen.findByTestId('ai-write-proposal-production-warning')).toBeTruthy();
    expect((screen.getByTestId('ai-write-proposal-allow') as HTMLButtonElement).disabled).toBe(
      true,
    );
  });

  it('expired proposals are denied without applying', async () => {
    render(<AiWriteProposalHost />);
    await emit(proposal({ expiresAt: new Date(Date.now() - 1000).toISOString() }));
    await waitFor(() => expect(denyMock).toHaveBeenCalledWith('proposal-1'));
    expect(applyMock).not.toHaveBeenCalled();
  });
});
