import { describe, expect, it } from 'vitest';
import { isSameEndpointIdentity } from '../endpointIdentity';

const source = { connectionId: 'conn-1', database: 'app', schema: 'public' };
const target = { connectionId: 'conn-1', database: 'app', schema: 'public' };

describe('isSameEndpointIdentity', () => {
  it('treats identical endpoints as the same place', () => {
    expect(isSameEndpointIdentity(source, target)).toBe(true);
  });

  it('compares schemas after trimming', () => {
    expect(isSameEndpointIdentity({ ...source, schema: ' public ' }, target)).toBe(true);
  });

  it('separates different connections', () => {
    expect(isSameEndpointIdentity(source, { ...target, connectionId: 'conn-2' })).toBe(false);
  });

  it('separates different databases', () => {
    expect(isSameEndpointIdentity(source, { ...target, database: 'other' })).toBe(false);
  });

  it('separates different schemas', () => {
    expect(isSameEndpointIdentity(source, { ...target, schema: 'audit' })).toBe(false);
  });

  it('never matches an unnamed database on either side', () => {
    expect(isSameEndpointIdentity({ ...source, database: '' }, target)).toBe(false);
    expect(isSameEndpointIdentity(source, { ...target, database: '' })).toBe(false);
  });
});
