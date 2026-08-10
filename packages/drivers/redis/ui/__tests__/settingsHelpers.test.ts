import { describe, expect, it } from 'vitest';
import { readClusterRouting, resolvePinnedNodeAddr } from '../settingsHelpers';

describe('settingsHelpers', () => {
  it('defaults clusterRouting to auto', () => {
    expect(readClusterRouting(undefined)).toBe('auto');
    expect(readClusterRouting({})).toBe('auto');
    expect(readClusterRouting({ clusterRouting: 'bogus' })).toBe('auto');
  });

  it('reads pinnedNode routing', () => {
    expect(readClusterRouting({ clusterRouting: 'pinnedNode' })).toBe('pinnedNode');
  });

  it('resolves pinned addr only when routing is pinnedNode', () => {
    expect(resolvePinnedNodeAddr('auto', '10.0.0.1:7000')).toBeNull();
    expect(resolvePinnedNodeAddr('pinnedNode', ' 10.0.0.1:7000 ')).toBe('10.0.0.1:7000');
    expect(resolvePinnedNodeAddr('pinnedNode', '  ')).toBeNull();
  });
});
