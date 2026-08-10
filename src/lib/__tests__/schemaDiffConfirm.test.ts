import { describe, expect, it } from 'vitest';
import { canConfirmDestructiveDeploy, canRunDeploy } from '../schemaDiffConfirm';

describe('schemaDiffConfirm', () => {
  it('allows additive without token', () => {
    expect(canConfirmDestructiveDeploy('', false)).toBe(true);
  });

  it('requires DEPLOY for destructive', () => {
    expect(canConfirmDestructiveDeploy('deploy', true)).toBe(false);
    expect(canConfirmDestructiveDeploy('DEPLOY', true)).toBe(true);
  });

  it('blocks when rollback required but incomplete', () => {
    expect(
      canRunDeploy({
        hasDestructive: false,
        confirmText: '',
        requireRollback: true,
        rollbackComplete: false,
        statementCount: 2,
      }),
    ).toBe(false);
  });

  it('allows when rollback complete', () => {
    expect(
      canRunDeploy({
        hasDestructive: false,
        confirmText: '',
        requireRollback: true,
        rollbackComplete: true,
        statementCount: 2,
      }),
    ).toBe(true);
  });
});
