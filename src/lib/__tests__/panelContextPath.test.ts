import { describe, expect, it } from 'vitest';
import { formatPanelContextPath } from '../panelContextPath';

describe('formatPanelContextPath', () => {
  it('joins connection, database, and schema', () => {
    expect(
      formatPanelContextPath({
        connectionName: 'Local PG',
        database: 'app',
        schema: 'public',
      }),
    ).toBe('Local PG.app.public');
  });

  it('omits schema when absent (e.g. MySQL)', () => {
    expect(
      formatPanelContextPath({
        connectionName: 'Local MySQL',
        database: 'app',
        schema: null,
      }),
    ).toBe('Local MySQL.app');
  });

  it('shows connection only when database is missing', () => {
    expect(
      formatPanelContextPath({
        connectionName: 'Redis',
        database: '',
        schema: undefined,
      }),
    ).toBe('Redis');
  });
});
