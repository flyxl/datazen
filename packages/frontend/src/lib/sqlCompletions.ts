import type { Completion } from '@codemirror/autocomplete';
import { buildFunctionCompletions } from './sqlFunctionRegistry';

/**
 * Backward-compatible function completions projection.
 *
 * Delegates to the unified function registry so that completions and
 * signature help share the same single source of truth.
 */
export function sqlFunctionCompletions(databaseType?: string): Completion[] {
  // Add SQL keyword completions that are not function-specific.
  const keywords: Completion[] = [
    { label: 'CASE', type: 'keyword', apply: 'CASE ' },
    { label: 'EXISTS', type: 'keyword', apply: 'EXISTS (' },
  ];
  return [...buildFunctionCompletions(databaseType), ...keywords];
}
