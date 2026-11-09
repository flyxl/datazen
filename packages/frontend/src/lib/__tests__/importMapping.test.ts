import { describe, expect, it } from 'vitest';
import { mappingIsExecutable, planImportMapping } from '../importMapping';

describe('planImportMapping: name matching', () => {
  it('matches source to target case-insensitively but records the real target name', () => {
    const plan = planImportMapping({
      sourceColumns: ['ID', 'name'],
      targetColumns: ['id', 'name', 'extra'],
      mode: 'name',
    });
    expect(plan.entries).toEqual([
      { source: 'ID', target: 'id', matchedBy: 'name' },
      { source: 'name', target: 'name', matchedBy: 'name' },
    ]);
    expect(plan.unmapped).toEqual([]);
    expect(plan.untargeted).toEqual(['extra']);
    expect(plan.conflicts).toEqual([]);
  });

  it('leaves unmatched sources unmapped in name-only mode', () => {
    const plan = planImportMapping({
      sourceColumns: ['a', 'nope'],
      targetColumns: ['a', 'b'],
      mode: 'name',
    });
    expect(plan.entries).toEqual([
      { source: 'a', target: 'a', matchedBy: 'name' },
      { source: 'nope', target: null, matchedBy: 'none' },
    ]);
    expect(mappingIsExecutable(plan)).toBe(true);
  });
});

describe('planImportMapping: positional matching', () => {
  it('maps by index regardless of names', () => {
    const plan = planImportMapping({
      sourceColumns: ['x', 'y', 'z'],
      targetColumns: ['first', 'second'],
      mode: 'position',
    });
    expect(plan.entries).toEqual([
      { source: 'x', target: 'first', matchedBy: 'position' },
      { source: 'y', target: 'second', matchedBy: 'position' },
      { source: 'z', target: null, matchedBy: 'none' },
    ]);
  });
});

describe('planImportMapping: auto mode (name first, then position)', () => {
  it('name matches win, leftovers fill free targets in order', () => {
    const plan = planImportMapping({
      sourceColumns: ['name', 'src_b', 'extra'],
      targetColumns: ['id', 'name', 'value'],
      mode: 'auto',
    });
    expect(plan.entries).toEqual([
      { source: 'name', target: 'name', matchedBy: 'name' },
      { source: 'src_b', target: 'id', matchedBy: 'position' },
      { source: 'extra', target: 'value', matchedBy: 'position' },
    ]);
    expect(plan.untargeted).toEqual([]);
  });

  it('positional fill skips targets already taken by name matches', () => {
    const plan = planImportMapping({
      sourceColumns: ['b', 'id'],
      targetColumns: ['id', 'b', 'c'],
      mode: 'auto',
    });
    expect(plan.entries).toEqual([
      { source: 'b', target: 'b', matchedBy: 'name' },
      { source: 'id', target: 'id', matchedBy: 'name' },
    ]);
    expect(plan.untargeted).toEqual(['c']);
  });
});

describe('planImportMapping: overrides and conflicts', () => {
  it('manual overrides win over automatic matching', () => {
    const plan = planImportMapping({
      sourceColumns: ['a', 'b'],
      targetColumns: ['a', 'b'],
      mode: 'auto',
      overrides: { a: 'b', b: null },
    });
    expect(plan.entries).toEqual([
      { source: 'a', target: 'b', matchedBy: 'manual' },
      { source: 'b', target: null, matchedBy: 'manual' },
    ]);
    expect(mappingIsExecutable(plan)).toBe(true);
  });

  it('two sources resolving onto one target are a blocking conflict', () => {
    const plan = planImportMapping({
      sourceColumns: ['id', 'ID'],
      targetColumns: ['id'],
      mode: 'auto',
    });
    expect(plan.conflicts).toEqual([
      { target: 'id', sources: expect.arrayContaining(['id', 'ID']) },
    ]);
    expect(mappingIsExecutable(plan)).toBe(false);
  });

  it('an override can resolve a conflict', () => {
    const conflict = planImportMapping({
      sourceColumns: ['id', 'ID'],
      targetColumns: ['id'],
      mode: 'auto',
    });
    expect(mappingIsExecutable(conflict)).toBe(false);
    const resolved = planImportMapping({
      sourceColumns: ['id', 'ID'],
      targetColumns: ['id'],
      mode: 'auto',
      overrides: { ID: null },
    });
    expect(resolved.conflicts).toEqual([]);
    expect(mappingIsExecutable(resolved)).toBe(true);
  });

  it('a plan that imports nothing is not executable', () => {
    const plan = planImportMapping({
      sourceColumns: ['a'],
      targetColumns: ['x'],
      mode: 'name',
    });
    expect(mappingIsExecutable(plan)).toBe(false);
  });
});
