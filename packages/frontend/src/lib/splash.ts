export function hideSplash(splash: HTMLElement | null): void {
  if (!splash) return;
  splash.classList.add('hide');
  window.setTimeout(() => splash.remove(), 350);
}

export type StartupWaitResult = 'completed' | 'timed-out';

/**
 * Keep optional startup preloads from holding the splash forever.
 *
 * The task is deliberately left running after the deadline so settings/theme
 * state can still settle once a temporarily delayed IPC responds.
 */
export async function waitForStartupTask(
  task: Promise<unknown>,
  timeoutMs: number,
): Promise<StartupWaitResult> {
  let timeoutId: number | undefined;
  const timeout = new Promise<StartupWaitResult>((resolve) => {
    timeoutId = window.setTimeout(() => resolve('timed-out'), timeoutMs);
  });
  const completed = task.then<StartupWaitResult>(() => 'completed');
  try {
    return await Promise.race([completed, timeout]);
  } finally {
    if (timeoutId !== undefined) window.clearTimeout(timeoutId);
  }
}
