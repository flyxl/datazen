import { afterEach, describe, expect, it, vi } from 'vitest';
import type {
  BackendClient,
  CloseMode,
  OwnerRef,
  ExecutionTarget,
  SessionHandle,
  SessionView,
} from '@datazen/backend-client';
import { EditorSessionController } from '../EditorSessionController';
import { registerPageUnloadCloser, releaseOwnedSessions } from '../pageUnload';

const target = {
  connectionId: 'c',
  namespace: { database: 'A', catalog: null, schema: null, path: [] },
  object: null,
} as unknown as ExecutionTarget;
const owner = { kind: 'editor', clientInstanceId: 'client', editorSessionId: 'editor' } as OwnerRef;

function session(id: string): SessionView {
  return {
    handle: { dbSessionId: id, runtimeEpoch: 'rte-00000001' },
    connectionId: 'c',
    configRevision: '1',
    owner,
    initialTarget: target,
    contextRevision: '1',
    state: 'ready',
    attachmentState: 'attached',
    activeExecutionId: null,
    expiresAt: null,
    observedContext: {
      namespace: target.namespace,
      searchPath: [],
      effectiveIdentity: 'user',
      transactionState: 'none',
      autocommit: true,
      confidence: 'confirmed',
    },
  } as unknown as SessionView;
}

function fixture() {
  const client = {
    backendId: 'desktop',
    issueSubmissionToken: vi.fn(async () => ({ idempotencyKey: 'token', expiresAt: 999 })),
    openSession: vi.fn(async () => ({ session: session('s1'), attachmentToken: 'attach' })),
    setSessionContext: vi.fn(async () => ({
      session: session('s2'),
      replacedSessionId: 's1',
      attachmentToken: 'new',
    })),
    getSession: vi.fn(async () => session('s2')),
    closeSession: vi.fn(async (_request: { handle: SessionHandle; mode: CloseMode }) => ({
      dbSessionId: 's1',
      state: 'closed',
      effectOutcome: 'none',
      resourceRelease: 'released',
    })),
  };
  return { client, facade: client as unknown as BackendClient };
}

afterEach(() => {
  // A closer that outlives its test would fire into the next one's fixture.
  releaseOwnedSessions();
});

describe('page-death session release', () => {
  it('closes the session a dying page still owns', async () => {
    const { client, facade } = fixture();
    const editor = new EditorSessionController(facade, target, owner);
    await editor.ensureSession();
    expect(client.closeSession).not.toHaveBeenCalled();

    window.dispatchEvent(new Event('pagehide'));

    expect(client.closeSession).toHaveBeenCalledTimes(1);
    expect(client.closeSession.mock.calls[0][0]).toMatchObject({
      handle: { dbSessionId: 's1' },
      mode: 'requireNoTransaction',
    });
  });

  it('closes the current handle, not the replaced one, when the page dies', async () => {
    const { client, facade } = fixture();
    const editor = new EditorSessionController(facade, target, owner);
    await editor.ensureSession();
    // A context change replaces the live session on the same controller.
    await editor.setContext({ ...target.namespace, database: 'B' });
    expect(editor.snapshot?.handle.dbSessionId).toBe('s2');

    window.dispatchEvent(new Event('beforeunload'));

    const handles = client.closeSession.mock.calls.map(([request]) => request.handle.dbSessionId);
    expect(handles).toContain('s2');
    expect(handles[handles.length - 1]).toBe('s2');
  });

  it('does not speak for a session the page already closed', async () => {
    const { client, facade } = fixture();
    const editor = new EditorSessionController(facade, target, owner);
    await editor.ensureSession();
    await editor.close();
    expect(client.closeSession).toHaveBeenCalledTimes(1);

    window.dispatchEvent(new Event('pagehide'));

    expect(client.closeSession).toHaveBeenCalledTimes(1);
  });

  it('drops a closer whose owner unregistered it', () => {
    const closer = vi.fn();
    const unregister = registerPageUnloadCloser(closer);
    unregister();
    releaseOwnedSessions();
    expect(closer).not.toHaveBeenCalled();
  });

  it('runs every closer even when one throws', () => {
    const survivor = vi.fn();
    registerPageUnloadCloser(() => {
      throw new Error('this closer is broken');
    });
    registerPageUnloadCloser(survivor);

    expect(() => releaseOwnedSessions()).not.toThrow();
    expect(survivor).toHaveBeenCalledTimes(1);
  });
});
