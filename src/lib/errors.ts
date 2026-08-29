export function toErrorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
