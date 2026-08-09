import { describe, expect, it } from 'vitest';
import { getConnectionView } from '../connectionViews';
import { SqlConnectionView } from '../../windows/connection/SqlConnectionView';
import { RedisConnectionView } from '../../windows/connection/RedisConnectionView';
import { DocumentConnectionView } from '../../windows/connection/DocumentConnectionView';

describe('getConnectionView', () => {
  it('returns mapped views by mode', () => {
    expect(getConnectionView('sql')).toBe(SqlConnectionView);
    expect(getConnectionView('keyvalue')).toBe(RedisConnectionView);
    expect(getConnectionView('document')).toBe(DocumentConnectionView);
  });

  it('falls back to sql view for unknown mode', () => {
    expect(getConnectionView('unknown')).toBe(SqlConnectionView);
  });
});
