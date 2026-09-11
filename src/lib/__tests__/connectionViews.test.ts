import { describe, expect, it } from 'vitest';
import { getConnectionView } from '../connectionViews';
import { DocumentConnectionView } from '../../windows/connection/DocumentConnectionView';
import { getDriverConnectionView } from '../../extensions/generated';

describe('getConnectionView', () => {
  it('returns mapped views by mode', () => {
    const driverKeyvalue = getDriverConnectionView('keyvalue');
    if (driverKeyvalue) {
      expect(getConnectionView('keyvalue')).toBe(driverKeyvalue);
    }
    expect(getConnectionView('document')).toBe(DocumentConnectionView);
  });

  it('falls back to document view for unknown mode', () => {
    expect(getConnectionView('unknown')).toBe(DocumentConnectionView);
  });
});
