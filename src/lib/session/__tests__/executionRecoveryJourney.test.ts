import { describe, expect, it, vi } from 'vitest';
import type { ArtifactChunk, BackendClient, EventEnvelope, ConnectionEvent, ExecutionReceipt, ExecutionView, Id, OwnerRef, ExecutionTarget, SessionView, SessionHandle } from '@datazen/backend-client';
import { EditorSessionController } from '../EditorSessionController';
import { ExecutionProjection } from '../ExecutionProjection';
import { switchDatabaseSession } from '../QueryPanelSession';
import { sessionErrorPromptKey, projectionErrorPromptKey, switchResultPromptKey } from '../sessionPrompts';

const target = { connectionId: 'c', namespace: { database: 'A', catalog: null, schema: null, path: [] }, object: null } as unknown as ExecutionTarget;
const owner = { kind: 'editor', clientInstanceId: 'client', editorSessionId: 'editor' } as unknown as OwnerRef;

function session(id = 's1', revision = '1', handle?: SessionHandle): SessionView {
  return { handle: handle ?? { dbSessionId: id, runtimeEpoch: 'epoch' }, connectionId: 'c', configRevision: '1', owner,
    initialTarget: target, contextRevision: revision, state: 'ready', attachmentState: 'attached', activeExecutionId: null,
    expiresAt: null, observedContext: { namespace: target.namespace, searchPath: [], effectiveIdentity: 'user',
      transactionState: 'none', autocommit: true, confidence: 'confirmed' } } as unknown as SessionView;
}

function event(sequence: string, executionId: string): EventEnvelope<ConnectionEvent> {
  return { streamId: 'stream', sequence, executionId, runtimeEpoch: 'epoch', jobId: null,
    sessionHandle: session().handle, contextRevision: '1', payload: { kind: 'resultChunk', artifactId: 'a', chunkIndex: '0',
      source: { executionId, statementIndex: 0, context: session().observedContext, relation: target, writableMapping: 'verified' } } } as unknown as EventEnvelope<ConnectionEvent>;
}

describe('execute → cancel → recover → re-execute journey', () => {
  it('keeps chunks recoverable, invalidates bindings across database switch, and re-executes cleanly', async () => {
    let nextExec = 0;
    const readArtifact = vi.fn(async (request: { chunkIndex: unknown }) => ({
      artifactId: 'a', chunkIndex: request.chunkIndex, totalChunks: null, publishedChunkCount: '1',
      offset: '0', bytes: new Uint8Array([1]), resultCompleteness: 'complete',
    }) as unknown as ArtifactChunk);
    const client = {
      backendId: 'desktop',
      issueSubmissionToken: vi.fn(async () => ({ idempotencyKey: 'token', expiresAt: 999 })),
      openSession: vi.fn(async () => ({ session: session(), attachmentToken: 'attach' })),
      executeInSession: vi.fn(async () => ({ executionId: `e${++nextExec}`, streamId: 'stream', state: 'queued' }) as ExecutionReceipt),
      setSessionContext: vi.fn(async () => ({ session: session('s2', '2', { dbSessionId: 's2', runtimeEpoch: 'epoch' }), replacedSessionId: 's1', attachmentToken: 'new' })),
      getSession: vi.fn(async () => session('s2', '2', { dbSessionId: 's2', runtimeEpoch: 'epoch' })),
      getExecution: vi.fn(async () => ({ executionId: 'e1', state: 'cancelled', effectOutcome: 'unknown', provenance: { requestedTarget: target }, artifactIds: ['a'], resultCompleteness: 'truncated', truncationReason: 'cancelled', errorCode: 'cancelled', runtimeBinding: null }) as unknown as ExecutionView),
      readArtifact,
      cancelExecution: vi.fn(async () => ({ executionId: 'e1', disposition: 'requested', state: 'cancelRequested' })),
      closeSession: vi.fn(async () => ({ dbSessionId: 's1', state: 'closed', effectOutcome: 'unknown', resourceRelease: 'quarantined' })),
    } as unknown as BackendClient & {
      executeInSession: ReturnType<typeof vi.fn>;
      cancelExecution: ReturnType<typeof vi.fn>;
      setSessionContext: ReturnType<typeof vi.fn>;
    };

    const controller = new EditorSessionController(client, target, owner);
    const first = await controller.execute({ command: 'query', input: { sql: 'select 1' } });
    const projection = new ExecutionProjection(client, first, session().handle);
    await projection.accept(event('100', 'e1'));
    expect(projection.chunks).toHaveLength(1);

    await controller.cancel();
    expect(client.cancelExecution).toHaveBeenCalledWith('e1', expect.anything());
    await projection.recover();
    expect(projection.view?.state).toBe('cancelled');
    expect(sessionErrorPromptKey(new Error('x'))).toBe('query.session.executionFailed');

    // Database switch replaces the session handle: the old result binding dies.
    const switched = await switchDatabaseSession({ controller, desired: { ...target.namespace, database: 'B' } });
    expect(switched.status).toBe('switched');
    projection.invalidateRuntimeBinding();
    expect(projection.writableBinding(controller.snapshot?.handle ?? null)).toBeNull();
    await projection.dispose();

    // Re-execute under the replacement session yields a fresh execution/projection.
    const second = await controller.execute({ command: 'query', input: { sql: 'select 2' } });
    expect(second.executionId).toBe('e2');
    expect(controller.snapshot?.handle.dbSessionId).toBe('s2');
    const projection2 = new ExecutionProjection(client, second, controller.snapshot?.handle ?? null);
    await projection2.accept(event('100', 'e2'));
    expect(projection2.chunks).toHaveLength(1);

    // 断线 / disconnect style failure maps to the disconnected prompt.
    expect(projectionErrorPromptKey(new Error('network lost'))).toBe('query.session.disconnected');
    expect(switchResultPromptKey({ status: 'conflict', session: null })).toBe('query.session.revisionConflict');
    await projection2.dispose();
  });
});
