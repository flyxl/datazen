/**
 * The pure rules of the in-cell foreign-key selector (DB-20).
 *
 * These pin the two facts that keep the picker honest: only a single-column
 * key becomes a picker (a composite pick would have to stage several cells at
 * once, which the edit pipeline cannot express), and the candidates are the
 * target rows themselves — order kept, duplicates kept, nothing invented.
 */
import { describe, expect, it } from 'vitest';
import {
  candidatesToPickerOptions,
  fkGroupIsPickable,
  indexFkCandidateSources,
  type FkCandidateSource,
} from '../fkSelector';
import type { Value } from '../../types';

describe('fkGroupIsPickable', () => {
  it('accepts a single-column key', () => {
    expect(fkGroupIsPickable(['customer_id'])).toBe(true);
  });

  it('refuses a composite key — one pick cannot write several cells', () => {
    expect(fkGroupIsPickable(['tenant_id', 'customer_id'])).toBe(false);
  });

  it('refuses an empty group', () => {
    expect(fkGroupIsPickable([])).toBe(false);
  });
});

describe('indexFkCandidateSources', () => {
  it('indexes the single-column groups by their member column', () => {
    const groups = new Map([
      [
        'customer_id',
        {
          localColumns: ['customer_id'],
          targetColumns: ['id'],
          targetLabel: 'customers.id',
          constraintName: 'orders_customer_fk',
          referenceTable: 'customers',
        },
      ],
    ]);

    const sources = indexFkCandidateSources(groups.entries());

    expect(sources.get('customer_id')).toEqual<FkCandidateSource>({
      localColumn: 'customer_id',
      constraintName: 'orders_customer_fk',
    });
  });

  it('leaves composite members out — they must never appear pickable', () => {
    const groups = new Map<
      string,
      {
        localColumns: string[];
        targetColumns: string[];
        targetLabel: string;
        constraintName: string;
        referenceTable: string;
      }
    >([
      [
        'tenant_id',
        {
          localColumns: ['tenant_id', 'customer_id'],
          targetColumns: ['tenant', 'id'],
          targetLabel: 'customers.(tenant, id)',
          constraintName: 'orders_customer_fk',
          referenceTable: 'customers',
        },
      ],
      [
        'customer_id',
        {
          localColumns: ['tenant_id', 'customer_id'],
          targetColumns: ['tenant', 'id'],
          targetLabel: 'customers.(tenant, id)',
          constraintName: 'orders_customer_fk',
          referenceTable: 'customers',
        },
      ],
    ]);

    const sources = indexFkCandidateSources(groups.entries());

    expect(sources.size).toBe(0);
  });

  it('keeps an empty constraint name as null — the resolver treats it as absent', () => {
    const groups = new Map([
      [
        'customer_id',
        {
          localColumns: ['customer_id'],
          targetColumns: ['id'],
          targetLabel: 'customers.id',
          constraintName: '',
          referenceTable: 'customers',
        },
      ],
    ]);

    const sources = indexFkCandidateSources(groups.entries());

    expect(sources.get('customer_id')?.constraintName).toBeNull();
  });
});

describe('candidatesToPickerOptions', () => {
  it('formats the first column of every candidate row', () => {
    const rows: (Value | null)[][] = [[1], [42], ['abc']];

    expect(candidatesToPickerOptions(rows)).toEqual(['1', '42', 'abc']);
  });

  it('skips a NULL candidate — a key that cannot be picked must not be offered', () => {
    const rows: (Value | null)[][] = [[null], [7]];

    expect(candidatesToPickerOptions(rows)).toEqual(['7']);
  });

  it('keeps duplicates — the picker must not lie about what the table holds', () => {
    const rows: (Value | null)[][] = [[5], [5]];

    expect(candidatesToPickerOptions(rows)).toEqual(['5', '5']);
  });

  it('keeps the statement order — the command never orders on its own (07 U-8)', () => {
    const rows: (Value | null)[][] = [[9], [1], [5]];

    expect(candidatesToPickerOptions(rows)).toEqual(['9', '1', '5']);
  });

  it('formats booleans and numbers as their text form, the way a typed value travels', () => {
    const rows: (Value | null)[][] = [[true], [3.5]];

    expect(candidatesToPickerOptions(rows)).toEqual(['true', '3.5']);
  });
});
