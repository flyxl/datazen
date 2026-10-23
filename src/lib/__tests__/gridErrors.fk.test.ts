/**
 * The `grid.fk.*` codes are produced by the shared foreign-key navigation layer
 * (`packages/feature-core/src/services/fk_navigation.rs`). They must be registered
 * here, otherwise `classifyGridError` returns `unknown` and the front end cannot
 * branch on them.
 */
import { describe, expect, it } from 'vitest';
import { GRID_ERROR_CODES, classifyGridError } from '../gridErrors';

const FK_CODES = [
  'grid.fk.shapeMismatch',
  'grid.fk.unknownConstraint',
  'grid.fk.valueNull',
  'grid.fk.targetUnresolved',
  'grid.fk.probeFailed',
] as const;

describe('grid.fk.* codes', () => {
  it('registers every foreign-key code the back end produces', () => {
    for (const code of FK_CODES) {
      expect(GRID_ERROR_CODES).toContain(code);
    }
  });

  it('classifies the back-end messages to the matching code', () => {
    const messages: readonly [string, string][] = [
      [
        'grid.fk.shapeMismatch: source columns [a, b] and values [a] have different lengths',
        'grid.fk.shapeMismatch',
      ],
      [
        'grid.fk.unknownConstraint: constraint "fk_orders_customer" was not found on "orders"',
        'grid.fk.unknownConstraint',
      ],
      [
        'grid.fk.valueNull: column "customer_id" is null, so the target row cannot be addressed',
        'grid.fk.valueNull',
      ],
      [
        'grid.fk.targetUnresolved: the referenced table "customers" could not be resolved',
        'grid.fk.targetUnresolved',
      ],
      [
        'grid.fk.probeFailed: the existence probe against "customers" failed',
        'grid.fk.probeFailed',
      ],
    ];
    for (const [message, code] of messages) {
      expect(classifyGridError(message)).toBe(code);
    }
  });

  it('keeps an unrelated message on the unknown fallback', () => {
    expect(classifyGridError('permission denied for table users')).toBe('unknown');
  });
});
