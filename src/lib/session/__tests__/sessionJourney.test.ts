import { describe, expect, it, vi } from 'vitest';
import type { BackendClient, SessionView, ExecutionReceipt, ExecutionView, EventEnvelope, ConnectionEvent, ArtifactChunk, OwnerRef, ExecutionTarget } from '@datazen/backend-client';
import { EditorSessionController } from '../EditorSessionController';
import { ExecutionProjection } from '../ExecutionProjection';

const target = { connectionId: 'c', namespace: { database: 'A', catalog: null, schema: null, path: [] }, object: null } as unknown as ExecutionTarget;
const owner = { kind: 'editor', clientInstanceId: 'client', editorSessionId: 'editor' } as OwnerRef;
function session(id = 's', revision = '1'): SessionView {
  return { handle: { dbSessionId: id, runtimeEpoch: 'epoch' }, connectionId: 'c', configRevision: '1', owner,
    initialTarget: target, contextRevision: revision, state: 'ready', attachmentState: 'attached', activeExecutionId: null,
    expiresAt: null, observedContext: { namespace: target.namespace, searchPath: [], effectiveIdentity: 'user',
      transactionState: 'none', autocommit: true, confidence: 'confirmed' } } as unknown as SessionView;
}
function fixture() {
  let next = 0;
  const client = {
    backendId: 'desktop', issueSubmissionToken: vi.fn(async () => ({ idempotencyKey: 'token', expiresAt: 999 })),
    openSession: vi.fn(async () => ({ session: session(`s${++next}`), attachmentToken: 'attach' })),
    executeInSession: vi.fn(async () => ({ executionId: `e${next}`, streamId: 'stream', state: 'queued' })),
    setSessionContext: vi.fn(async () => ({ session: session('replacement', '2'), replacedSessionId: 's1', attachmentToken: 'new' })),
    getSession: vi.fn(async () => session('s1', '2')),
    closeSession: vi.fn(async () => ({ dbSessionId: 's1', state: 'closed', effectOutcome: 'unknown', resourceRelease: 'quarantined' })),
    cancelExecution: vi.fn(async () => ({ executionId: 'e1', disposition: 'requested', state: 'cancelRequested' })),
  };
  return { client, facade: client as unknown as BackendClient };
}

describe('editor session journey', () => {
  it('opens lazily once, isolates clones, preserves rejected replacement, and exposes unknown close', async () => {
    const { client, facade } = fixture();
    const editor = new EditorSessionController(facade, target, owner);
    const clone = editor.clone({ ...owner, editorSessionId: 'clone' } as OwnerRef);
    expect(client.openSession).not.toHaveBeenCalled();
    await Promise.all([editor.ensureSession(), editor.ensureSession()]);
    expect(client.openSession).toHaveBeenCalledTimes(1);
    expect(clone.snapshot).toBeNull();
    expect(() => editor.clone(owner)).toThrow('distinct owner');
    await editor.execute({ command: 'query', input: { sql: 'BEGIN' } });
    editor.acceptSession({ ...session('s1', '2'), observedContext: { ...session().observedContext, transactionState: 'unknown' } });
    expect(editor.snapshot?.observedContext.transactionState).toBe('unknown');
    client.setSessionContext.mockRejectedValueOnce(new Error('ContextConflict'));
    await expect(editor.setContext({ ...target.namespace, database: 'B' })).rejects.toThrow('ContextConflict');
    expect(editor.snapshot?.handle.dbSessionId).toBe('s1');
    await editor.setContext({ ...target.namespace, database: 'B' });
    expect(editor.acceptSession(session('s1', '999'))).toBe(false);
    expect((await editor.close('rollbackAndClose'))?.effectOutcome).toBe('unknown');
    await expect(editor.execute({ command: 'query', input: {} })).rejects.toThrow();
    await editor.openNewSession();
    await clone.ensureSession();
    expect(clone.snapshot?.handle.dbSessionId).not.toBe(editor.snapshot?.handle.dbSessionId);
  });
});

function execution(): ExecutionView {
  return { executionId: 'e', state: 'cancelled', effectOutcome: 'unknown', provenance: { requestedTarget: target },
    artifactIds: ['a'], resultCompleteness: 'truncated', truncationReason: 'cancelled', errorCode: 'cancelled',
    runtimeBinding: { handle: session().handle, resourceBindingId: 'r', executionId: 'e' } } as unknown as ExecutionView;
}
function event(sequence: string, index: string): EventEnvelope<ConnectionEvent> {
  return { streamId: 'stream', sequence, executionId: 'e', runtimeEpoch: 'epoch', jobId: null,
    sessionHandle: session().handle, contextRevision: '1', payload: { kind: 'resultChunk', artifactId: 'a', chunkIndex: index,
      source: { executionId: 'e', statementIndex: 0, context: session().observedContext, relation: target, writableMapping: 'verified' } } } as unknown as EventEnvelope<ConnectionEvent>;
}
describe('result recovery journey', () => {
  it('deduplicates large sequences, retries unread blocks, recovers gaps, and keeps old target', async () => {
    let fail = true;
    const readArtifact = vi.fn(async (request: { chunkIndex: unknown }) => {
      if (String(request.chunkIndex) === '1' && fail) { fail = false; throw new Error('disconnected'); }
      return { artifactId: 'a', chunkIndex: request.chunkIndex, totalChunks: null, publishedChunkCount: '3',
        offset: '0', bytes: new Uint8Array([1]), resultCompleteness: 'truncated' } as unknown as ArtifactChunk;
    });
    const cancelExecution = vi.fn();
    const facade = { backendId: 'desktop', readArtifact, getExecution: vi.fn(async () => execution()), cancelExecution } as unknown as BackendClient;
    const projection = new ExecutionProjection(facade, { executionId: 'e', streamId: 'stream', state: 'queued' } as ExecutionReceipt, session().handle);
    await projection.accept(event('9007199254740993', '0'));
    await projection.accept(event('9007199254740993', '0'));
    expect(readArtifact).toHaveBeenCalledTimes(1);
    await expect(projection.accept(event('9007199254740994', '1'))).rejects.toThrow('disconnected');
    expect(projection.chunks).toHaveLength(1);
    await projection.accept(event('9007199254740996', '2'));
    expect(projection.chunks).toHaveLength(3);
    expect(projection.view?.effectOutcome).toBe('unknown');
    expect(projection.view?.provenance?.requestedTarget.namespace.database).toBe('A');
    expect(projection.writableBinding(session('new').handle)).toBeNull();
    projection.invalidateRuntimeBinding();
    expect(projection.writableBinding(session().handle)).toBeNull();
    await projection.recover();
    expect(projection.writableBinding(session().handle)).toBeNull();
    await projection.dispose();
    expect(cancelExecution).not.toHaveBeenCalled();
  });
});
