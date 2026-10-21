/** Preserve the first failure and attempt every cleanup without reporting raw errors. */
export async function withFixtureCleanup<T>(
  body: () => Promise<T>,
  cleanups: Array<{ label: string; run: () => Promise<unknown> }>,
): Promise<T> {
  let result!: T;
  let primary: unknown;
  let failed = false;
  try { result = await body(); }
  catch (error) { primary = error; failed = true; }
  const cleanupFailures: string[] = [];
  for (const cleanup of cleanups) {
    try { await cleanup.run(); }
    catch { cleanupFailures.push(cleanup.label); }
  }
  const suffix = cleanupFailures.length ? `; cleanup failed: ${cleanupFailures.join(', ')}` : '';
  if (failed) {
    if (primary instanceof Error) {
      primary.message += suffix;
      throw primary;
    }
    throw new Error(`Fixture primary failure (non-Error)${suffix}`);
  }
  if (cleanupFailures.length) throw new Error(`Fixture cleanup failed: ${cleanupFailures.join(', ')}`);
  return result;
}
