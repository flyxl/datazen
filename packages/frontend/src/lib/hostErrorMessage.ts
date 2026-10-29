/**
 * The message of a failed call, whatever the transport threw.
 *
 * It lives in its own module because it is a **pure** function about error
 * text, not about foreign keys: the saved-views menu needs it, and a
 * component that reaches for it must not drag the navigation stores in with
 * it. `src/lib/fkNavigationActions.ts` re-exports it, so the path the rest of
 * the code already uses keeps working.
 *
 * The trip from the host to the front end is a **contract**: the `grid.*`
 * classification reads the prefix of this text, so it has to be the host's own
 * text and not a wrapper's.
 *
 * The backend client prefixes the message of a failed call with
 * `"<method> failed: "`, and the `grid.*` classification reads the **prefix**
 * of the message — so the original text (the one the host sent, reachable
 * through the cause chain) is the one that is classified and shown.
 */
export function hostErrorMessage(error: unknown): string {
  const messages: string[] = [];
  let current: unknown = error;
  for (let depth = 0; depth < 5 && current != null; depth += 1) {
    if (typeof current === 'string') {
      messages.push(current);
      break;
    }
    if (current instanceof Error) {
      messages.push(current.message);
      current = (current as { cause?: unknown }).cause;
      continue;
    }
    if (typeof current === 'object') {
      const message = (current as { message?: unknown }).message;
      if (typeof message === 'string') messages.push(message);
      break;
    }
    break;
  }
  const original = messages[messages.length - 1];
  return original && original.trim() !== '' ? original : String(error);
}
