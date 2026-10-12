import { describe, expect, it, vi } from 'vitest';
import type {
  BackendClient,
  CloseMode,
  ContextChangeReceipt,
  ExecutionTarget,
  Id,
  NamespaceTarget,
  OwnerRef,
  SessionHandle,
  SessionView,
} from '@datazen/backend-client';
import { EditorSessionController } from '../EditorSessionController';

/**
 * `connection-management.md` §7.4 step 6 / §6.1 — 需要重连的切库采用两阶段替换:
 * the runtime builds the candidate session internally, then on commit publishes
 * it together with a receipt (`{ session, replacedSessionId, attachmentToken }`)
 * and marks the previous session `Closing`. The UI side of that contract is
 * "return the new session, then atomically swap the binding" — and only then is
 * the old handle, which nothing else in the editor owner can reach any more,
 * released.
 *
 * The three cases below are the state machine's three exits:
 *   - committed replacement  → rebind, drop the dead execution, release the old handle;
 *   - committed in-place     → rebind, keep everything (no orphan to release);
 *   - rejected (revision / transaction / replacement failure) → previous handle
 *     stays bound and usable, and the old session is not closed.
 */

/** Runtime identifiers are branded at the backend-client boundary. */
const id = (value: string): Id => value as unknown as Id;

const namespace = { database: 'A', catalog: null, schema: null, path: [] };
const target = { connectionId: 'c', namespace, object: null } as unknown as ExecutionTarget;
const owner = { kind: 'editor', clientInstanceId: 'client', editorSessionId: 'editor' } as OwnerRef;

function session(id: string, revision = '1'): SessionView {
  return {
    handle: { dbSessionId: id, runtimeEpoch: `epoch-${id}` },
    connectionId: 'c',
    configRevision: '1',
    owner,
    initialTarget: target,
    contextRevision: revision,
    state: 'ready',
    attachmentState: 'attached',
    activeExecutionId: null,
    expiresAt: null,
    observedContext: {
      namespace,
      searchPath: [],
      effectiveIdentity: 'user',
      transactionState: 'none',
      autocommit: true,
      confidence: 'confirmed',
    },
  } as unknown as SessionView;
}

interface Fixture {
  controller: EditorSessionController;
  setSessionContext: ReturnType<typeof vi.fn>;
  closeSession: ReturnType<typeof vi.fn>;
  /** `controller.snapshot.handle.dbSessionId` as seen by each `closeSession` call. */
  boundAtRelease: Array<string | undefined>;
  receipt: (next: ContextChangeReceipt) => void;
}

function fixture(): Fixture {
  /** The handle the controller was bound to at each `closeSession` call. */
  const boundAtRelease: Array<string | undefined> = [];
  let controller: EditorSessionController;
  const closeSession = vi.fn(async (request: { handle: SessionHandle; mode: CloseMode }) => {
    boundAtRelease.push(controller.snapshot?.handle.dbSessionId as string | undefined);
    return {
      dbSessionId: request.handle.dbSessionId,
      state: 'closed',
      effectOutcome: 'confirmed',
      resourceRelease: 'confirmed',
    };
  });
  const setSessionContext = vi.fn(async () => ({}) as ContextChangeReceipt);
  const client = {
    backendId: 'desktop',
    issueSubmissionToken: vi.fn(async () => ({ idempotencyKey: 'token', expiresAt: 999 })),
    openSession: vi.fn(async () => ({ session: session('s1'), attachmentToken: 'attach-1' })),
    setSessionContext,
    closeSession,
    getSession: vi.fn(async () => session('s1')),
    executeInSession: vi.fn(async () => ({ executionId: 'e1', streamId: 'stream', state: 'queued' })),
    cancelExecution: vi.fn(async () => ({ executionId: 'e1', disposition: 'requested', state: 'cancelRequested' })),
  };
  controller = new EditorSessionController(client as unknown as BackendClient, target, owner);
  return {
    controller,
    setSessionContext,
    closeSession,
    boundAtRelease,
    receipt: (next) => {
      setSessionContext.mockResolvedValue(next);
    },
  };
}

const toB: NamespaceTarget = { ...namespace, database: 'B' };

describe('two-phase session replacement binding', () => {
  it('rebinds to the new session and releases the replaced handle after a committed replacement', async () => {
    const { controller, setSessionContext, closeSession, boundAtRelease, receipt } = fixture();
    await controller.ensureSession();
    expect(controller.snapshot?.handle.dbSessionId).toBe('s1');
    await controller.execute({ command: 'query', input: { sql: 'SELECT 1' } });
    expect(controller.activeExecution).not.toBeNull();

    receipt({ session: session('s2', '2'), replacedSessionId: id('s1'), attachmentToken: 'attach-2' });
    const bound = await controller.setContext(toB);

    expect(setSessionContext).toHaveBeenCalledTimes(1);
    expect(bound.handle.dbSessionId).toBe('s2');
    expect(controller.snapshot?.handle.dbSessionId).toBe('s2');
    // The execution belonged to the retired handle and is no longer cancellable here.
    expect(controller.activeExecution).toBeNull();
    // Exactly one release, for the pre-replacement handle, after the rebind.
    expect(closeSession).toHaveBeenCalledTimes(1);
    expect(closeSession).toHaveBeenCalledWith({
      handle: { dbSessionId: 's1', runtimeEpoch: 'epoch-s1' },
      mode: 'requireNoTransaction',
    });
    // The controller was already bound to the new session when the old handle was
    // released, so a listener woken by the rebind never observes the retired one.
    expect(boundAtRelease).toEqual(['s2']);
  });

  it('keeps every handle when the change commits in place (nothing was replaced)', async () => {
    const { controller, closeSession, receipt } = fixture();
    await controller.ensureSession();

    receipt({ session: session('s1', '2'), replacedSessionId: null, attachmentToken: null });
    await controller.setContext(toB);

    expect(controller.snapshot?.handle.dbSessionId).toBe('s1');
    expect(closeSession).not.toHaveBeenCalled();
  });

  it('leaves the previous handle bound and the previous session open when the runtime rejects the change', async () => {
    const { controller, setSessionContext, closeSession } = fixture();
    await controller.ensureSession();
    setSessionContext.mockRejectedValueOnce(new Error('ContextConflict'));

    await expect(controller.setContext(toB)).rejects.toThrow('ContextConflict');

    expect(controller.snapshot?.handle.dbSessionId).toBe('s1');
    expect(closeSession).not.toHaveBeenCalled();
    // Still usable: the old handle can still run something afterwards.
    await expect(controller.execute({ command: 'query', input: { sql: 'SELECT 1' } })).resolves.toMatchObject({ executionId: 'e1' });
  });

  it('does not fail the switch when the replaced session is already gone', async () => {
    const { controller, closeSession, receipt } = fixture();
    await controller.ensureSession();
    receipt({ session: session('s2', '2'), replacedSessionId: id('s1'), attachmentToken: 'attach-2' });
    closeSession.mockRejectedValueOnce(new Error('SessionLost'));

    await expect(controller.setContext(toB)).resolves.toMatchObject({ handle: { dbSessionId: 's2' } });
    expect(controller.snapshot?.handle.dbSessionId).toBe('s2');
  });
});
