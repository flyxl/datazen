import { describe, expect, it, vi } from 'vitest';
import type { BackendClient, ExecutionTarget, OwnerRef, SessionView } from '@datazen/backend-client';
import { ApiError } from '@datazen/backend-client';
import { QueryPanelSessionRegistry, switchDatabaseSession } from '../QueryPanelSession';
import { EditorSessionController } from '../EditorSessionController';

const target = { connectionId: 'c', namespace: { database: 'A', catalog: null, schema: null, path: [] }, object: null } as unknown as ExecutionTarget;
const ownerA = { kind: 'editor', clientInstanceId: 'client', editorSessionId: 'a' } as unknown as OwnerRef;
const ownerB = { kind: 'editor', clientInstanceId: 'client', editorSessionId: 'b' } as unknown as OwnerRef;

function session(revision = '1', tx: 'none' | 'active' | 'unknown' = 'none'): SessionView {
  return { handle: { dbSessionId: 's', runtimeEpoch: 'epoch' }, connectionId: 'c', configRevision: '1', owner: ownerA,
    initialTarget: target, contextRevision: revision, state: 'ready', attachmentState: 'attached', activeExecutionId: null,
    expiresAt: null, observedContext: { namespace: target.namespace, searchPath: [], effectiveIdentity: 'user',
      transactionState: tx, autocommit: true, confidence: 'confirmed' } } as unknown as SessionView;
}

function fixture(tx: 'none' | 'active' | 'unknown' = 'none', conflict = false) {
  const client = {
    backendId: 'desktop', issueSubmissionToken: vi.fn(async () => ({ idempotencyKey: 'token', expiresAt: 999 })),
    openSession: vi.fn(async () => ({ session: session('1', tx), attachmentToken: 'attach' })),
    executeInSession: vi.fn(async () => ({ executionId: 'e1', streamId: 'stream', state: 'queued' })),
    setSessionContext: conflict
      ? vi.fn(async () => { throw new ApiError('ContextConflict', 'revision mismatch'); })
      : vi.fn(async () => ({ session: session('2', tx), replacedSessionId: 's', attachmentToken: 'new' })),
    getSession: vi.fn(async () => session('1', tx)),
    closeSession: vi.fn(async () => ({ dbSessionId: 's', state: 'closed', effectOutcome: 'unknown', resourceRelease: 'quarantined' })),
  };
  return client as unknown as BackendClient & typeof client;
}

describe('QueryPanelSessionRegistry', () => {
  it('lazily creates one controller per editor key and duplicates with a fresh owner', async () => {
    const client = fixture();
    const registry = new QueryPanelSessionRegistry(client);
    const a1 = registry.get('panel-1', target, ownerA);
    const a2 = registry.get('panel-1', target, ownerA);
    expect(a1).toBe(a2);
    const dup = registry.duplicate('panel-1', 'panel-1-copy', target, ownerB);
    expect(dup).not.toBe(a1);
    await a1.ensureSession();
    await dup.ensureSession();
    expect(client.openSession).toHaveBeenCalledTimes(2);
    const sessions = (client.openSession.mock.calls as unknown as Array<[{ owner: OwnerRef }]>).map(
      (c) => (c[0].owner as { kind: 'editor'; editorSessionId: string }).editorSessionId,
    );
    expect(sessions).toEqual(['a', 'b']);
  });
});

describe('switchDatabaseSession', () => {
  it('blocks on active transaction until confirmed and keeps previous handle on conflict', async () => {
    const client = fixture('active', true);
    const controller = new EditorSessionController(client, target, ownerA);
    await controller.ensureSession();
    const before = controller.snapshot;
    let declined = await switchDatabaseSession({ controller, desired: { ...target.namespace, database: 'B' } });
    expect(declined.status).toBe('cancelled');
    expect(controller.snapshot?.handle.dbSessionId).toBe(before?.handle.dbSessionId);
    expect(controller.snapshot?.contextRevision).toBe(before?.contextRevision);
    const onConflict = vi.fn();
    const conflict = await switchDatabaseSession({
      controller, desired: { ...target.namespace, database: 'B' },
      confirmTransaction: () => true, onRevisionConflict: onConflict,
    });
    expect(conflict.status).toBe('conflict');
    expect(onConflict).toHaveBeenCalledOnce();
    expect(controller.snapshot?.handle.dbSessionId).toBe('s');
  });

  it('switches context and updates the session on success', async () => {
    const client = fixture('none');
    const controller = new EditorSessionController(client, target, ownerA);
    const result = await switchDatabaseSession({ controller, desired: { ...target.namespace, database: 'B' } });
    expect(result.status).toBe('switched');
    expect(controller.snapshot?.contextRevision).toBe('2');
  });
});
