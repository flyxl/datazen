import { describe, expect, it } from 'vitest';
import { assessExecutionRisk, hostGuardWouldBlock } from '../queryExecutionRisk';
import fixture from '../../../../lib/__tests__/fixtures/sqlRiskCases.json';

describe('assessExecutionRisk', () => {
  it.each(fixture.cases)('$id: hardBlocked matches Host guard', (tc) => {
    const decision = assessExecutionRisk(tc.sql, {
      readOnly: tc.mode.readOnly,
      safeMode: tc.mode.safeMode,
      isProduction: false,
    });
    expect(decision.hardBlocked).toBe(!tc.guardAllowed);
  });

  it('readOnly + mutation -> hardBlocked with reason readOnly and no confirm', () => {
    const d = assessExecutionRisk('DELETE FROM t WHERE id = 1', {
      readOnly: true,
      safeMode: false,
      isProduction: false,
    });
    expect(d.hardBlocked).toBe(true);
    expect(d.hardBlockReason).toBe('readOnly');
    expect(d.needsConfirm).toBe(false);
  });

  it('safeMode + no-WHERE -> hardBlocked with reason safeMode and no confirm', () => {
    const d = assessExecutionRisk('UPDATE t SET x = 1', {
      readOnly: false,
      safeMode: true,
      isProduction: false,
    });
    expect(d.hardBlocked).toBe(true);
    expect(d.hardBlockReason).toBe('safeMode');
    expect(d.needsConfirm).toBe(false);
  });

  it('safeMode blocks DROP without a bypassable confirm', () => {
    const d = assessExecutionRisk('DROP TABLE t', {
      readOnly: false,
      safeMode: true,
      isProduction: false,
    });
    expect(d.hardBlocked).toBe(true);
    expect(d.needsConfirm).toBe(false);
  });

  it('non-safeMode high-risk -> confirm high-risk', () => {
    const d = assessExecutionRisk('DROP TABLE t', {
      readOnly: false,
      safeMode: false,
      isProduction: false,
    });
    expect(d.hardBlocked).toBe(false);
    expect(d.needsConfirm).toBe(true);
    expect(d.confirmReasons).toContain('high-risk');
  });

  it('production + mutation confirms even when safeMode allows it', () => {
    const d = assessExecutionRisk('UPDATE t SET x = 1 WHERE id = 1', {
      readOnly: false,
      safeMode: true,
      isProduction: true,
    });
    expect(d.hardBlocked).toBe(false);
    expect(d.needsConfirm).toBe(true);
    expect(d.confirmReasons).toContain('production');
  });

  it('production + read does not confirm', () => {
    const d = assessExecutionRisk('SELECT 1', {
      readOnly: false,
      safeMode: false,
      isProduction: true,
    });
    expect(d.needsConfirm).toBe(false);
  });

  it('high-risk + production merge into one confirm with all reasons', () => {
    const d = assessExecutionRisk('DROP TABLE t', {
      readOnly: false,
      safeMode: false,
      isProduction: true,
    });
    expect(d.needsConfirm).toBe(true);
    expect(d.confirmReasons).toEqual(['production', 'high-risk']);
  });

  it('unknown confirms only under production', () => {
    const plain = assessExecutionRisk('GIBBERISH foo', {
      readOnly: false,
      safeMode: false,
      isProduction: false,
    });
    expect(plain.needsConfirm).toBe(false);
    const prod = assessExecutionRisk('GIBBERISH foo', {
      readOnly: false,
      safeMode: false,
      isProduction: true,
    });
    expect(prod.needsConfirm).toBe(true);
    expect(prod.confirmReasons).toContain('production');
  });

  it('SELECT INTO is may-write: production confirms, but no hard block under readOnly', () => {
    const prod = assessExecutionRisk('SELECT a INTO b FROM c', {
      readOnly: false,
      safeMode: false,
      isProduction: true,
    });
    expect(prod.needsConfirm).toBe(true);
    expect(prod.confirmReasons).toContain('production');
    const ro = assessExecutionRisk('SELECT a INTO b FROM c', {
      readOnly: true,
      safeMode: false,
      isProduction: false,
    });
    expect(ro.hardBlocked).toBe(false);
  });
});

describe('hostGuardWouldBlock', () => {
  it.each(fixture.cases)('$id', (tc) => {
    expect(
      hostGuardWouldBlock(tc.sql, {
        readOnly: tc.mode.readOnly,
        safeMode: tc.mode.safeMode,
      }),
    ).toBe(!tc.guardAllowed);
  });
});
