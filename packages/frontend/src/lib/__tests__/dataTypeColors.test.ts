import { describe, expect, it } from 'vitest';
import { cellValueTextClass, classifyDataType, dataTypeTextClass } from '../dataTypeColors';

describe('classifyDataType', () => {
  it('maps common SQL types to families', () => {
    expect(classifyDataType('integer')).toBe('number');
    expect(classifyDataType('varchar(255)')).toBe('text');
    expect(classifyDataType('timestamp with time zone')).toBe('datetime');
    expect(classifyDataType('boolean')).toBe('bool');
    expect(classifyDataType('jsonb')).toBe('json');
    expect(classifyDataType('bytea')).toBe('binary');
    expect(classifyDataType('')).toBe('text');
  });
});

describe('dataTypeTextClass', () => {
  it('returns theme token tailwind classes', () => {
    expect(dataTypeTextClass('numeric')).toBe('text-dt-number');
    expect(dataTypeTextClass('jsonb')).toBe('text-dt-json');
    expect(dataTypeTextClass('bytea')).toBe('text-dt-binary');
  });
});

describe('cellValueTextClass', () => {
  it('uses null token for nullish values', () => {
    expect(cellValueTextClass('text', null)).toBe('text-dt-null');
    expect(cellValueTextClass('text', undefined)).toBe('text-dt-null');
  });

  it('uses type family for non-null values', () => {
    expect(cellValueTextClass('boolean', true)).toBe('text-dt-bool');
    expect(cellValueTextClass('text', 'hello')).toBe('text-dt-text');
  });
});
