/**
 * Cross-component bridge for "run this SQL in the query panel".
 *
 * The AI chat drawer lives outside the mounted `QueryPanel`, while execution
 * safety (Safe Mode, read-only blocking, production/high-risk confirmation)
 * lives inside the panel's execution gate. This registry lets the chat hand a
 * statement to the owning panel without re-implementing that gate.
 *
 * A request that arrives before the panel has registered is kept as pending
 * and consumed exactly once when the panel mounts. That is what makes
 * "create a query tab and run once" a single user action.
 */

type QueryRunner = (sql: string) => void;

const runners = new Map<string, QueryRunner>();
const pending = new Map<string, string>();

/** Register the active panel's runner and flush a request queued while mounting. */
export function registerQueryRunner(panelId: string, runner: QueryRunner): () => void {
  runners.set(panelId, runner);
  const queued = pending.get(panelId);
  if (queued !== undefined) {
    pending.delete(panelId);
    runner(queued);
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
  pending.set(panelId, sql);
  return false;
}
