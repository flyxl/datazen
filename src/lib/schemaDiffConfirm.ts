import { DESTRUCTIVE_CONFIRM_TOKEN } from '../commands/schemaDiff';

/** Confirm gate for destructive schema deploy. */
export function canConfirmDestructiveDeploy(
  typed: string,
  hasDestructive: boolean,
): boolean {
  if (!hasDestructive) return true;
  return typed.trim() === DESTRUCTIVE_CONFIRM_TOKEN;
}

export function canRunDeploy(opts: {
  hasDestructive: boolean;
  confirmText: string;
  requireRollback: boolean;
  rollbackComplete: boolean;
  statementCount: number;
}): boolean {
  if (opts.statementCount === 0) return false;
  if (opts.requireRollback && !opts.rollbackComplete) return false;
  return canConfirmDestructiveDeploy(opts.confirmText, opts.hasDestructive);
}
