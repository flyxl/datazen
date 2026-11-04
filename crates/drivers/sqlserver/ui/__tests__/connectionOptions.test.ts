import { describe, expect, it } from 'vitest';
import { mergeSqlServerOptions, readSqlServerOptions } from '../connectionOptions';

describe('sqlserver connection options', () => {
  it('reads supported options', () => {
    expect(
      readSqlServerOptions({
        trustServerCertificate: false,
        applicationName: '  DataZen  ',
      }),
    ).toEqual({
      trustServerCertificate: false,
      applicationName: 'DataZen',
    });
  });

  it('merges and removes optional values', () => {
    const merged = mergeSqlServerOptions(
      { trustServerCertificate: true, applicationName: 'DataZen' },
      { trustServerCertificate: false, applicationName: '' },
    );
    expect(merged).toEqual({ trustServerCertificate: false });
  });
});
