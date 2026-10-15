/**
 * Closing a tab (or a pane) whose statement has not finished yet.
 *
 * ## The order this file exists to enforce
 *
 * A running statement holds a physical connection. Taking the tab away first and
 * cancelling afterwards releases the tab while the backend is still working, and
 * the cancel becomes an afterthought on a session that is already gone — the
 * desktop flow used to do exactly that. The contract here is the other order:
 *
 *   1. ask the user (the tab is about to lose work),
 *   2. ask the runtime to stop the statement and **wait for it to be over**,
 *   3. return, so the caller releases the session and only then drops the tab.
 *
 * ## Why the wait reads the store, not the client
 *
 * `running` in `queryExec` is flipped by the execution stream the store itself
 * owns, so it is the same signal the run button and the status line use, it
 * survives the tab being unmounted (the stream belongs to the store action, not
 * to a component), and it needs no extra backend round trip per poll. A pane
 * whose entry disappeared counts as finished.
 *
 * ## What happens when the wait cannot be satisfied
 *
 * Two different situations must not be conflated:
 *
 * * **Nothing could be asked to stop.** Either the driver cannot cancel
 *   precisely, or the pane has no live editor session left to cancel through.
 *   Then there is no request to confirm, and the caller's session close *is* the
 *   cleanup: closing the connection aborts the statement server-side. Waiting
 *   would only delay a close that already cleans up, so this returns `settled`
 *   immediately — and the caller decides *before* it gets here whether there is
 *   anything to ask about at all.
 * * **A cancel was requested and never confirmed.** Nobody can claim the
 *   statement stopped, so the tab must not disappear: this reports `settled:
 *   false` and the caller keeps the tab and says why.
 */

import type { Panel } from './panelTypes';
import type { QueryExecState } from './queryExecActions';
import { DEFAULT_PANE_ID, paneIdOfPaneKey, paneKey, paneKeysOfPanel } from './paneKeys';

/** One pane of a tab whose statement is still in flight. */
export interface RunningPane {
  paneId: string;
  exec: QueryExecState;
}

/** What the host is told before a tab with a live statement is taken away. */
export interface RunningExecutionRequest {
  panelId: string;
  /** Tab title, so the prompt can say which tab it is asking about. */
  title: string;
  /** Pane whose SQL the prompt shows. */
  paneId: string;
  sql: string;
  /** How many panes of this tab are still running. */
  runningPanes: number;
}

/**
 * Host-supplied confirmation. Resolves `true` to continue closing.
 *
 * A promise rather than a boolean because the answer comes from a human.
 */
export type RunningExecutionGate = (request: RunningExecutionRequest) => Promise<boolean>;

/** How long a close waits for a requested cancel to be confirmed. */
export const RUNNING_CLOSE_TIMEOUT_MS = 15_000;

/** Poll interval while waiting for the stream to report the terminal state. */
export const RUNNING_CLOSE_POLL_MS = 50;

let gate: RunningExecutionGate | null = null;
let closeTimeoutMs = RUNNING_CLOSE_TIMEOUT_MS;
let queue: Promise<unknown> = Promise.resolve();
let pending: ((allowed: boolean) => void) | null = null;

/**
 * Register the prompt host. The store is headless, so the view layer installs
 * this; `null` (headless callers, tests, MCP) means "no gate" and closing
 * proceeds without asking, which is the pre-existing behaviour.
 *
 * Changing the host also releases a prompt that is still open, because that
 * prompt belongs to a host that is no longer on screen and can never be answered:
 * leaving it pending would strand its own close *and* make every later close queue
 * behind a question with no asker. It is released as "allowed", the same answer
 * the no-gate path gives, so a caller behaves the same before and after the host
 * goes away. Asks that were waiting their turn are put to the host on screen now.
 */
export function setRunningExecutionGate(next: RunningExecutionGate | null): void {
  if (gate === next) return;
  gate = next;
  pending?.(true);
  pending = null;
  queue = Promise.resolve();
}

/** Tighten or widen the cancel-confirmation deadline. Exported for tests. */
export function setRunningCloseTimeoutMs(ms: number): void {
  closeTimeoutMs = ms;
}

/**
 * Ask the host whether a running tab may be closed.
 *
 * Serialized: closing several tabs at once (Close All / Close to the Right)
 * fires one close per tab, and a dialog host can only show one prompt. Chaining
 * also means the *answers* cannot interleave — without this, the second prompt
 * would replace the first and the first tab's close would never resolve.
 */
export function askRunningExecutionGate(request: RunningExecutionRequest): Promise<boolean> {
  const current = gate;
  if (!current) return Promise.resolve(true);
  const asked = queue.then(
    () => askNow(current, request),
    () => askNow(current, request),
  );
  queue = asked.then(
    () => undefined,
    () => undefined,
  );
  return asked;
}

/**
 * Run one prompt and settle it even if the host fails or disappears.
 *
 * A host that throws is not an answer, and an unanswered "may I destroy this
 * running statement?" must not be read as consent: the tab is kept. Swallowing
 * the failure here also keeps a broken dialog from rejecting the caller's close.
 */
function askNow(current: RunningExecutionGate, request: RunningExecutionRequest): Promise<boolean> {
  // Re-read the live gate: it may have been withdrawn, or replaced, while this
  // ask waited its turn. A withdrawn host leaves nobody to ask, which is the
  // same situation the no-gate path above already answers with "allowed".
  if (gate !== current) {
    const live = gate;
    return live ? askNow(live, request) : Promise.resolve(true);
  }
  return new Promise<boolean>((resolve) => {
    const settle = (allowed: boolean) => {
      if (pending !== settle) return; // already released by `setRunningExecutionGate(null)`
      pending = null;
      resolve(allowed);
    };
    pending = settle;
    void current(request).then(
      (allowed) => settle(allowed),
      () => settle(false),
    );
  });
}

/** Panes of `panel` whose statement has not finished. */
export function runningPanesOf(
  panel: Panel,
  currentExec: Map<string, QueryExecState>,
): RunningPane[] {
  if (panel.type !== 'query') return [];
  const panes: RunningPane[] = [];
  for (const key of paneKeysOfPanel(currentExec, panel.id)) {
    const exec = currentExec.get(key);
    if (exec?.running) panes.push({ paneId: paneIdOfPaneKey(key), exec });
  }
  return panes;
}

/** The subset of `panes` whose cancel can be requested *and* confirmed. */
export function stoppablePanes(input: {
  panes: RunningPane[];
  /** Whether the driver supports precise cancellation at all. */
  canCancel: boolean;
  /** Whether the pane still has an editor session to cancel through. */
  hasSession: (paneId: string) => boolean;
}): RunningPane[] {
  if (!input.canCancel) return [];
  return input.panes.filter((pane) => input.hasSession(pane.paneId));
}

/** Outcome of the conversation the caller runs before it takes a tab away. */
export type RunningCloseDecision =
  | { kind: 'proceed' }
  /** The user chose to keep the tab; nothing has been touched. */
  | { kind: 'declined' }
  /** A cancel was asked for and never confirmed; keep the tab and say so. */
  | { kind: 'unconfirmed'; pending: string[] };

export interface ConfirmRunningCloseDeps {
  /** Live exec state for a `queryExec` key; a missing entry counts as finished. */
  readExec: (key: string) => QueryExecState | undefined;
  /** Ask the runtime to stop this pane's statement; see {@link SettleRunningDeps}. */
  cancelPane: (paneId: string) => Promise<boolean>;
}

/**
 * Ask the user, then stop what is running and wait for it to be over.
 *
 * Only call this with panes that passed {@link stoppablePanes}: with nothing
 * stoppable there is no question worth asking and no stop to confirm.
 */
export async function confirmStoppableRunningPanes(
  panel: Panel,
  stoppable: RunningPane[],
  deps: ConfirmRunningCloseDeps,
): Promise<RunningCloseDecision> {
  const first = stoppable[0];
  const allowed = await askRunningExecutionGate({
    panelId: panel.id,
    title: panel.type === 'query' ? panel.title : panel.connectionName,
    paneId: first?.paneId ?? DEFAULT_PANE_ID,
    sql: first?.exec.sql ?? '',
    runningPanes: stoppable.length,
  });
  if (!allowed) return { kind: 'declined' };

  const settlement = await settleRunningPanes(panel, stoppable, {
    readExec: deps.readExec,
    requestCancel: deps.cancelPane,
  });
  return settlement.settled
    ? { kind: 'proceed' }
    : { kind: 'unconfirmed', pending: settlement.pending };
}

export interface RunningCloseWiring {
  /** The store's live `queryExec` map. */
  queryExec: () => Map<string, QueryExecState>;
  /** Whether that pane still has an editor session to cancel through. */
  hasSession: (panelId: string, paneId: string) => boolean;
  /** Whether the panel's driver supports precise cancellation. */
  canCancel: (panel: Panel) => boolean;
  /** Ask the runtime to stop one pane; see {@link SettleRunningDeps.requestCancel}. */
  cancelPane: (panelId: string, paneId: string) => Promise<boolean>;
  /** Mark the pane that kept its tab because its cancel was never confirmed. */
  markBlocked: (panelId: string, paneId: string) => void;
}

/**
 * Build the guard the panel store runs before it takes a tab (or a pane) away.
 *
 * Returns `true` **synchronously** when there is nothing that could be stopped —
 * no running statement, a driver without precise cancellation, or a pane whose
 * editor session is already gone. That is deliberate: in those states the close
 * path the caller runs next *is* the cleanup, and forcing an extra microtask
 * would change behaviour for callers that never had a statement to stop. Only
 * when a cancel can really be requested does the guard hand back a promise.
 */
export function createRunningCloseGuard(wiring: RunningCloseWiring) {
  return function guard(input: {
    panel: Panel;
    panelId: string;
    /** Restrict the question to a single pane; omit to cover every pane of the tab. */
    paneId?: string;
  }): true | Promise<boolean> {
    const stoppable = stoppablePanes({
      panes: runningPanesOf(input.panel, wiring.queryExec()).filter(
        (pane) => input.paneId == null || pane.paneId === input.paneId,
      ),
      canCancel: wiring.canCancel(input.panel),
      hasSession: (paneId) => wiring.hasSession(input.panelId, paneId),
    });
    if (stoppable.length === 0) return true;

    return confirmStoppableRunningPanes(input.panel, stoppable, {
      readExec: (key) => wiring.queryExec().get(key),
      cancelPane: (paneId) => wiring.cancelPane(input.panelId, paneId),
    }).then((decision) => {
      if (decision.kind === 'unconfirmed') {
        wiring.markBlocked(input.panelId, decision.pending[0] ?? DEFAULT_PANE_ID);
      }
      return decision.kind === 'proceed';
    });
  };
}

export interface SettleRunningDeps {
  /** Live exec state for a `queryExec` key; a missing entry counts as finished. */
  readExec: (key: string) => QueryExecState | undefined;
  /**
   * Ask the runtime to stop this pane's statement.
   *
   * Resolves `false` when no request could be delivered (the session is gone,
   * the driver refused). There is then nothing to confirm, and the caller's
   * session close is the cleanup.
   */
  requestCancel: (paneId: string) => Promise<boolean>;
  timeoutMs?: number;
  pollMs?: number;
}

export interface RunningPaneSettlement {
  /** Whether closing may proceed. */
  settled: boolean;
  /** Panes still running when the deadline passed; only filled when `!settled`. */
  pending: string[];
}

/**
 * Cancel what is running and wait until it is over.
 *
 * See the module header for why an uncancellable driver settles immediately and
 * why an unconfirmed cancel does not.
 */
export async function settleRunningPanes(
  panel: Panel,
  panes: RunningPane[],
  deps: SettleRunningDeps,
): Promise<RunningPaneSettlement> {
  if (panes.length === 0) return { settled: true, pending: [] };

  const asked: RunningPane[] = [];
  for (const pane of panes) {
    let accepted = false;
    try {
      accepted = await deps.requestCancel(pane.paneId);
    } catch {
      // A failed cancel is not proof the statement is still running, but it is
      // also not a request anyone can confirm: it is not waited on.
      accepted = false;
    }
    if (accepted) asked.push(pane);
  }
  // Nothing was asked to stop, so there is no stop to wait for: the caller's
  // session close releases the resources on its own.
  if (asked.length === 0) return { settled: true, pending: [] };

  const deadline = Date.now() + (deps.timeoutMs ?? closeTimeoutMs);
  const pollMs = deps.pollMs ?? RUNNING_CLOSE_POLL_MS;
  // A missing entry is finished too: there is no longer anything to observe, and
  // waiting for a state the store has already dropped would time out instead.
  const stillRunning = (pane: RunningPane): boolean => {
    const exec = deps.readExec(paneKey(panel.id, pane.paneId));
    return exec ? exec.running : false;
  };

  for (;;) {
    const pending = asked.filter(stillRunning);
    if (pending.length === 0) return { settled: true, pending: [] };
    if (Date.now() >= deadline) {
      return { settled: false, pending: pending.map((pane) => pane.paneId) };
    }
    await new Promise<void>((resolve) => {
      setTimeout(resolve, pollMs);
    });
  }
}
