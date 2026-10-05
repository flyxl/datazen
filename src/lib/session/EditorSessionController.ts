import {
  ApiError,
  type BackendClient, type CommandCall, type ExecutionReceipt,
  type ExecutionTarget, type NamespaceTarget, type OwnerRef, type SessionView,
  type CloseMode, type CloseReceipt, type CancelReceipt,
} from '@datazen/backend-client';
import { counterValue, sameHandle } from './counters';

/** One instance per editor/pane. Runtime handles never leave this in-memory owner. */
export class EditorSessionController {
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
  ) {}

  get snapshot(): SessionView | null { return this.session; }
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
          initialTarget: this.initialTarget, owner: this.owner, idempotencyKey: token.idempotencyKey,
        }, token);
        this.session = receipt.session;
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
      const token = await this.token('executeInSession');
      const receipt = await this.client.executeInSession({
        handle: session.handle, expectedContextRevision: session.contextRevision,
        call, idempotencyKey: token.idempotencyKey,
      }, token);
      this.execution = receipt;
      this.publish();
      return receipt;
    });
  }

  setContext(desired: NamespaceTarget): Promise<SessionView> {
    return this.serial(async () => {
      const session = await this.ensureSession();
      const token = await this.token('setSessionContext');
      const receipt = await this.client.setSessionContext({
        handle: session.handle, expectedContextRevision: session.contextRevision,
        desired, idempotencyKey: token.idempotencyKey,
      }, token);
      // Failed revision/transaction/replacement leaves the previous handle intact.
      this.session = receipt.session;
      if (receipt.attachmentToken !== null) this.attachmentToken = receipt.attachmentToken;
      this.publish();
      return receipt.session;
    });
  }

  async refresh(): Promise<SessionView | null> {
    const handle = this.session?.handle;
    if (!handle) return null;
    const view = await this.client.getSession(handle);
    this.acceptSession(view);
    return this.session;
  }
  acceptSession(view: SessionView): boolean {
    if (!this.session || !sameHandle(view.handle, this.session.handle)) return false;
    if (counterValue(view.contextRevision) < counterValue(this.session.contextRevision)) return false;
    this.session = view;
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
    return new EditorSessionController(this.client, this.initialTarget, owner);
  }
}
