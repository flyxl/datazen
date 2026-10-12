import {
  ApiError,
  type BackendClient, type CommandCall, type ExecutionReceipt,
  type ExecutionTarget, type NamespaceTarget, type OwnerRef, type SessionHandle,
  type SessionView,
  type CloseMode, type CloseReceipt, type CancelReceipt,
} from '@datazen/backend-client';
import { counterValue, sameHandle } from './counters';

/** One instance per editor/pane. Runtime handles never leave this in-memory owner. */
export class EditorSessionController {
  private latestTarget: ExecutionTarget;
  private session: SessionView | null = null;
  private opening: Promise<SessionView> | null = null;
  private tail: Promise<unknown> = Promise.resolve();
  private attachmentToken: string | null = null;
  private execution: ExecutionReceipt | null = null;
  private listeners = new Set<() => void>();

  constructor(
    readonly client: BackendClient,
    readonly initialTarget: ExecutionTarget,
    readonly owner: OwnerRef,
  ) { this.latestTarget = initialTarget; }

  get snapshot(): SessionView | null { return this.session; }
  /** Project a lost resource without replacing its handle or execution receipt. */
  markSessionLost(handle: SessionHandle | null = this.session?.handle ?? null): void {
    if (!handle || !this.session || !sameHandle(handle, this.session.handle)) return;
    this.session = { ...this.session, state: 'lost', observedContext: {
      ...this.session.observedContext, transactionState: 'unknown',
    } };
    this.publish();
  }
  acceptSessionError(error: unknown, handle: SessionHandle | null = this.session?.handle ?? null): boolean {
    if (!(error instanceof ApiError) || !['SessionLost', 'SessionNotFound', 'RuntimeEpochMismatch'].includes(error.code)) return false;
    this.markSessionLost(handle);
    return true;
  }
  async getExecution(executionId: ExecutionReceipt['executionId']) {
    const handle = this.session?.handle ?? null;
    try {
      const view = await this.client.getExecution(executionId);
      if (view.errorCode === 'resourceLost') this.markSessionLost(handle);
      return view;
    } catch (error) {
      this.acceptSessionError(error, handle);
      throw error;
    }
  }
  get activeExecution(): ExecutionReceipt | null { return this.execution; }
  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => { this.listeners.delete(listener); };
  }
  private publish(): void { for (const listener of this.listeners) listener(); }
  private serial<T>(operation: () => Promise<T>): Promise<T> {
    const next = this.tail.then(operation, operation);
    this.tail = next.catch(() => undefined);
    return next;
  }
  private token(operation: string) {
    return this.client.issueSubmissionToken(operation, this.session?.handle ?? null, {
      connectionId: this.initialTarget.connectionId, owner: this.owner,
    });
  }

  async ensureSession(): Promise<SessionView> {
    if (this.session) {
      if (['lost', 'closed', 'closing'].includes(this.session.state)) {
        throw new ApiError('SessionLost', 'Explicitly open a new session to continue.');
      }
      return this.session;
    }
    if (!this.opening) {
      this.opening = (async () => {
        const token = await this.token('openSession');
        const receipt = await this.client.openSession({
          initialTarget: this.latestTarget, owner: this.owner, idempotencyKey: token.idempotencyKey,
        }, token);
        this.session = receipt.session;
        if (receipt.session.observedContext.confidence === 'confirmed') this.latestTarget = { ...this.latestTarget, namespace: receipt.session.observedContext.namespace };
        this.attachmentToken = receipt.attachmentToken;
        this.publish();
        return receipt.session;
      })().finally(() => { this.opening = null; });
    }
    return this.opening;
  }

  execute(call: CommandCall): Promise<ExecutionReceipt> {
    return this.serial(async () => {
      const session = await this.ensureSession();
      try {
        const token = await this.token('executeInSession');
        const receipt = await this.client.executeInSession({
          handle: session.handle, expectedContextRevision: session.contextRevision,
          call, idempotencyKey: token.idempotencyKey,
        }, token);
        this.execution = receipt;
        this.publish();
        return receipt;
      } catch (error) {
        this.acceptSessionError(error, session.handle);
        throw error;
      }
    });
  }

  /**
   * Two-phase context change.
   *
   * Entry: a ready session plus a desired namespace target. In state: one
   * `setSessionContext` carrying the current `expectedContextRevision`. Exit:
   * either the runtime rejects it (revision conflict / open transaction /
   * replacement failure) and the previous handle stays bound and usable, or it
   * commits and the receipt decides what happens next.
   *
   * A successful commit with a non-null `replacedSessionId` is a replacement:
   * the runtime published a *new* session and marked the old one `Closing`.
   * Rebinding to the new session orphans the old handle — nothing else in the
   * owner can reach it (`close()`/`detach()` and the `QueryPanelSessionRegistry`
   * release path all act on the controller's *current* session). So the old
   * handle is released here, right after the atomic rebind.
   */
  setContext(desired: NamespaceTarget): Promise<SessionView> {
    return this.serial(async () => {
      const session = await this.ensureSession();
      const previousHandle = session.handle;
      const token = await this.token('setSessionContext');
      const receipt = await this.client.setSessionContext({
        handle: session.handle, expectedContextRevision: session.contextRevision,
        desired, idempotencyKey: token.idempotencyKey,
      }, token);
      // Failed revision/transaction/replacement leaves the previous handle intact.
      this.session = receipt.session;
      if (receipt.session.observedContext.confidence === 'confirmed') this.latestTarget = { ...this.latestTarget, namespace: receipt.session.observedContext.namespace };
      if (receipt.attachmentToken !== null) this.attachmentToken = receipt.attachmentToken;
      // The tracked execution belonged to the handle we just gave up; it can no
      // longer be cancelled through this controller. Its results stay readable
      // through their own `ExecutionProjection`.
      if (receipt.replacedSessionId !== null) this.execution = null;
      this.publish();
      if (receipt.replacedSessionId !== null) {
        await this.releaseReplacedSession(previousHandle);
      }
      return receipt.session;
    });
  }

  /**
   * Best-effort release of the session a replacement took over from.
   *
   * The runtime already marked it `Closing` and owns its teardown, so this is
   * idempotent cleanup rather than the mechanism that ends it: a close that
   * fails (already gone, `SessionLost`, transport error) must not surface to
   * the user, who has already moved to the new session. `requireNoTransaction`
   * keeps it from disturbing a transaction on the *replacement* side.
   */
  private async releaseReplacedSession(handle: SessionHandle): Promise<void> {
    if (sameHandle(handle, this.session?.handle ?? null)) return;
    try {
      await this.client.closeSession({ handle, mode: 'requireNoTransaction' });
    } catch {
      /* the replacement already retired it; nothing to report */
    }
  }

  async refresh(): Promise<SessionView | null> {
    const handle = this.session?.handle;
    if (!handle) return null;
    let view: SessionView;
    try {
      view = await this.client.getSession(handle);
    } catch (error) {
      this.acceptSessionError(error, handle);
      throw error;
    }
    this.acceptSession(view);
    return this.session;
  }
  acceptSession(view: SessionView): boolean {
    if (!this.session || !sameHandle(view.handle, this.session.handle)) return false;
    // A later stale observation cannot revive a handle declared lost.
    if (this.session.state === 'lost' && !['lost', 'closed'].includes(view.state)) return false;
    if (counterValue(view.contextRevision) < counterValue(this.session.contextRevision)) return false;
    this.session = view;
    if (view.observedContext.confidence === 'confirmed') this.latestTarget = { ...this.latestTarget, namespace: view.observedContext.namespace };
    this.publish();
    return true;
  }
  async cancel(): Promise<CancelReceipt | null> {
    if (!this.execution) return null;
    const token = await this.token('cancelExecution');
    return this.client.cancelExecution(this.execution.executionId, token);
  }
  close(mode: CloseMode = 'requireNoTransaction'): Promise<CloseReceipt | null> {
    return this.serial(async () => {
      if (this.opening) await this.opening;
      if (!this.session) return null;
      const receipt = await this.client.closeSession({ handle: this.session.handle, mode });
      this.session = { ...this.session, state: receipt.state };
      this.attachmentToken = null;
      this.publish();
      return receipt;
    });
  }
  async detach(): Promise<void> {
    if (this.session && this.attachmentToken) {
      this.acceptSession(await this.client.detachSession({
        handle: this.session.handle, attachmentToken: this.attachmentToken,
      }));
    }
  }
  async attach(): Promise<void> {
    if (this.session && this.attachmentToken) {
      this.acceptSession(await this.client.attachSession({
        handle: this.session.handle, attachmentToken: this.attachmentToken,
      }));
    }
  }
  openNewSession(): Promise<SessionView> {
    return this.serial(async () => {
      if (this.opening) await this.opening;
      if (this.session && !['lost', 'closed'].includes(this.session.state)) {
        throw new Error('Close the current session before opening its replacement.');
      }
      this.session = null;
      this.execution = null;
      this.attachmentToken = null;
      return this.ensureSession();
    });
  }
  clone(owner: OwnerRef): EditorSessionController {
    if (owner.kind === 'editor' && this.owner.kind === 'editor' &&
        owner.editorSessionId === this.owner.editorSessionId) {
      throw new Error('Cloned editors require a distinct owner ID');
    }
    return new EditorSessionController(this.client, this.latestTarget, owner);
  }
}

/** Uses the host-issued client identity; editorSessionId is the pane's stable local owner ID. */
export async function createEditorSessionController(
  client: BackendClient, initialTarget: ExecutionTarget, editorSessionId: string,
): Promise<EditorSessionController> {
  const identity = await client.getPlatformIdentity();
  if (!identity.clientInstanceId || !editorSessionId) throw new Error('Missing editor owner identity');
  return new EditorSessionController(client, initialTarget, {
    kind: 'editor', clientInstanceId: identity.clientInstanceId as import('@datazen/backend-client').Id,
    editorSessionId: editorSessionId as import('@datazen/backend-client').Id,
  });
}
