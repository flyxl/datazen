/**
 * E2E-only stable locator attributes (mirrors host `src/lib/tid.ts`).
 */
export type TidAttrs = { 'data-testid': string } | Record<string, never>;

export const tid = (id: string): TidAttrs =>
  import.meta.env.VITE_E2E ? { 'data-testid': id } : {};
