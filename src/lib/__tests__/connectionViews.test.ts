import { describe, expect, it } from 'vitest';
import { getConnectionView } from '../connectionViews';
import { SqlConnectionView } from '../../windows/connection/SqlConnectionView';
import { DocumentConnectionView } from '../../windows/connection/DocumentConnectionView';
import { getPluginConnectionView } from '../../plugins/generated';

describe('getConnectionView', () => {
  it('returns mapped views by mode', () => {
    expect(getConnectionView('sql')).toBe(SqlConnectionView);
    expect(getConnectionView('keyvalue')).toBe(getPluginConnectionView('keyvalue'));
    expect(getConnectionView('document')).toBe(DocumentConnectionView);
  });

  it('falls back to sql view for unknown mode', () => {
    expect(getConnectionView('unknown')).toBe(SqlConnectionView);
  });
});
