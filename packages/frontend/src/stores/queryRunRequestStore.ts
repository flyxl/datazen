/**
 * Cross-component bridge for "run this SQL in the query panel".
 *
 * The AI chat drawer lives outside the mounted `QueryPanel`, while execution
 * safety (Safe Mode, read-only blocking, production/high-risk confirmation)
 * lives inside the panel's execution gate. This registry lets the chat hand a
 * statement to the owning panel without re-implementing that gate.
 *
 * Requests that arrive before the panel has registered are kept in order as
 * pending and flushed once when the panel mounts. That is what makes
 * "create a query tab and run once" a single user action, while two quick
 * clicks cannot lose the first statement.
 */

type QueryRunner = (sql: string) => void;

const runners = new Map<string, QueryRunner>();
const pending = new Map<string, string[]>();

/** Register the active panel's runner and flush a request queued while mounting. */
export function registerQueryRunner(panelId: string, runner: QueryRunner): () => void {
  runners.set(panelId, runner);
  const queued = pending.get(panelId);
  if (queued?.length) {
    pending.delete(panelId);
    for (const sql of queued) runner(sql);
  }
  return () => {
    if (runners.get(panelId) === runner) runners.delete(panelId);
    pending.delete(panelId);
  };
}

/**
 * Ask the panel to run `sql`. Returns true when a mounted panel accepted it
 * immediately; false means it was queued for the panel that is about to mount.
 */
export function requestQueryRun(panelId: string, sql: string): boolean {
  const runner = runners.get(panelId);
  if (runner) {
    runner(sql);
    return true;
  }
  const queue = pending.get(panelId);
  if (queue) queue.push(sql);
  else pending.set(panelId, [sql]);
  return false;
}
