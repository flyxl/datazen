import { ApiError, type BackendClient, type ExecutionTarget, type NamespaceTarget, type OwnerRef, type SessionView } from '@datazen/backend-client';
import { EditorSessionController, createEditorSessionController } from './EditorSessionController';

/**
 * Per-editor lazy session registry. Each editor/pane owns an independent
 * runtime session; duplicating a tab must never copy the runtime ID of the
 * source tab — the duplicate gets a fresh owner ID and therefore a fresh
 * (lazily opened) session.
 */
export class QueryPanelSessionRegistry {
  private controllers = new Map<string, EditorSessionController>();

  constructor(private readonly client: BackendClient) {}

  /** Returns the existing controller for `key`, or lazily creates one. */
  get(key: string, target: ExecutionTarget, owner: OwnerRef): EditorSessionController {
    let controller = this.controllers.get(key);
    if (!controller) {
      controller = new EditorSessionController(this.client, target, owner);
      this.controllers.set(key, controller);
    }
    return controller;
  }

  peek(key: string): EditorSessionController | undefined {
    return this.controllers.get(key);
  }

  /** Copy semantics: a new owner ID, a new controller — never the runtime handle. */
  duplicate(sourceKey: string, targetKey: string, target: ExecutionTarget, owner: OwnerRef): EditorSessionController {
    const fresh = new EditorSessionController(this.client, target, owner);
    this.controllers.set(targetKey, fresh);
    return fresh;
  }

  async remove(key: string): Promise<void> {
    const controller = this.controllers.get(key);
    this.controllers.delete(key);
    if (controller) {
      try { await controller.close('requireNoTransaction'); } catch { /* session may already be gone */ }
    }
  }
}

export type DatabaseSwitchResult =
  | { status: 'switched'; session: SessionView }
  | { status: 'cancelled' }
  | { status: 'conflict'; session: SessionView | null }
  | { status: 'failed'; error: unknown };

export interface DatabaseSwitchOptions {
  controller: EditorSessionController;
  desired: NamespaceTarget;
  /** Confirms switching away from an active transaction. Return false to abort. */
  confirmTransaction?: (session: SessionView) => Promise<boolean> | boolean;
  /** Surfaces a revision conflict to the user. */
  onRevisionConflict?: (session: SessionView | null) => void;
}

/**
 * Two-phase database switch: refresh → transaction confirmation → setContext
 * with the latest expectedContextRevision. Failed revision / transaction /
 * replacement leaves the previous runtime handle intact.
 */
export async function switchDatabaseSession(options: DatabaseSwitchOptions): Promise<DatabaseSwitchResult> {
  const { controller, desired } = options;
  try {
    const current = await controller.refresh() ?? controller.snapshot;
    const txState = current?.observedContext?.transactionState;
    if (txState === 'active' || txState === 'unknown') {
      const ok = options.confirmTransaction ? await options.confirmTransaction(current as SessionView) : false;
      if (!ok) return { status: 'cancelled' };
    }
    try {
      const session = await controller.setContext(desired);
      return { status: 'switched', session };
    } catch (error) {
      if (error instanceof ApiError && error.code === 'ContextConflict') {
        const session = await controller.refresh().catch(() => controller.snapshot);
        options.onRevisionConflict?.(session);
        return { status: 'conflict', session };
      }
      throw error;
    }
  } catch (error) {
    return { status: 'failed', error };
  }
}
