import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { validateRedisConnection } from '../../../redis/ui/connection/connectionWizardValidate';
import { sqlServerValidate } from '../ConnectionFields';
import {
  validateSqlServerConnection,
  type SqlServerValidationFields,
} from '../connectionFieldsValidate';
import { sqlserverMeta } from '../meta';

const t = (key: string) => key;

function fields(overrides: Partial<SqlServerValidationFields> = {}): SqlServerValidationFields {
  return {
    host: '',
    port: '',
    database: '',
    username: '',
    password: '',
    schema: '',
    ...overrides,
  };
}

/**
 * The only input shape both drivers agree on. Redis branches on a `topology`
 * option that SQL Server has no concept of (`SqlServerConnectionOptions` is
 * `trustServerCertificate` / `applicationName` only), so parity is asserted on
 * the shared `standalone` shape — the one that is genuinely the same form.
 */
type SharedFields = {
  host: string;
  port: string;
  database: string;
  username: string;
  password: string;
  schema: string;
  options?: Record<string, unknown>;
};
type SharedValidator = (
  fields: SharedFields,
  t: (key: string) => string,
) => Record<string, string>;

const sqlserver: SharedValidator = validateSqlServerConnection;
const redis: SharedValidator = validateRedisConnection;

describe('validateSqlServerConnection — host', () => {
  it('reports the host as required when it is empty', () => {
    const errors = validateSqlServerConnection(fields({ port: '1433' }), t);

    expect(errors.host).toBe('newConn.required');
    // The port is fine, so it must not be dragged into the same error.
    expect(errors.port).toBeUndefined();
  });

  it('reports the host as required when it is whitespace only', () => {
    expect(validateSqlServerConnection(fields({ host: '   ', port: '1433' }), t).host).toBe(
      'newConn.required',
    );
  });
});

describe('validateSqlServerConnection — port', () => {
  it('reports the port as required when it is empty', () => {
    const errors = validateSqlServerConnection(fields({ host: 'db.internal' }), t);

    expect(errors.port).toBe('newConn.required');
    expect(errors.host).toBeUndefined();
  });

  it('reports the port as required when it is whitespace only', () => {
    expect(validateSqlServerConnection(fields({ host: 'db.internal', port: '  ' }), t).port).toBe(
      'newConn.required',
    );
  });

  it('reports the port as required when it is not a number', () => {
    for (const port of ['abc', '1433a', '14 33', '-', '1433.3.1', '1e']) {
      expect(validateSqlServerConnection(fields({ host: 'db.internal', port }), t).port).toBe(
        'newConn.required',
      );
    }
  });

  it('accepts a numeric port, including one padded with spaces', () => {
    expect(validateSqlServerConnection(fields({ host: 'db.internal', port: '1433' }), t)).toEqual(
      {},
    );
    expect(validateSqlServerConnection(fields({ host: 'db.internal', port: ' 1433 ' }), t)).toEqual(
      {},
    );
  });
});

describe('validateSqlServerConnection — the valid case', () => {
  it('returns zero errors when host and port are both usable', () => {
    const errors = validateSqlServerConnection(
      fields({
        host: 'db.internal',
        port: '1433',
        database: 'master',
        username: 'sa',
        password: 'secret',
      }),
      t,
    );

    expect(errors).toEqual({});
    expect(Object.keys(errors)).toHaveLength(0);
  });

  it('leaves database, username and password alone', () => {
    // `ConnectionFields.tsx` renders no `required` marker and no error slot for
    // any of the three, so blank here must stay valid: SQL Server resolves them
    // against server defaults (`master`, `sa`).
    expect(validateSqlServerConnection(fields({ host: 'db.internal', port: '1433' }), t)).toEqual(
      {},
    );
  });

  it('accepts the defaults the driver itself advertises', () => {
    expect(
      validateSqlServerConnection(
        fields({
          host: sqlserverMeta.defaultHost,
          port: String(sqlserverMeta.defaultPort),
          database: '',
          username: sqlserverMeta.defaultUser,
        }),
        t,
      ),
    ).toEqual({});
  });

  it('reports both fields at once when the form is opened empty', () => {
    expect(validateSqlServerConnection(fields(), t)).toEqual({
      host: 'newConn.required',
      port: 'newConn.required',
    });
  });
});

describe('validateSqlServerConnection — the keys it emits', () => {
  const CONNECTION_FIELDS_SOURCE = resolve(import.meta.dirname, '..', 'ConnectionFields.tsx');

  function renderedErrorSlots(): string[] {
    const source = readFileSync(CONNECTION_FIELDS_SOURCE, 'utf8');
    return [
      ...new Set([...source.matchAll(/form\.validationErrors\.(\w+)/g)].map((match) => match[1])),
    ].sort();
  }

  function requiredFieldLabels(): string[] {
    const source = readFileSync(CONNECTION_FIELDS_SOURCE, 'utf8');
    return [
      ...new Set(
        [...source.matchAll(/<Label required>\s*\{t\('newConn\.(\w+)'\)\}/g)].map((m) => m[1]),
      ),
    ].sort();
  }

  it('emits exactly the keys ConnectionFields renders an error slot for', () => {
    const emitted = Object.keys(validateSqlServerConnection(fields(), t)).sort();

    expect(emitted).toEqual(['host', 'port']);
    expect(emitted).toEqual(renderedErrorSlots());
  });

  it('covers every field ConnectionFields marks required', () => {
    // The defect this validator exists for: both labels were `required` and both
    // had a reserved error slot, yet nothing ever produced those two errors.
    expect(requiredFieldLabels()).toEqual(['host', 'port']);
  });

  it('emits no other key across a sweep of blank and malformed inputs', () => {
    const keys = new Set<string>();
    for (const host of ['', '  ', 'db.internal']) {
      for (const port of ['', '  ', '1433', 'abc']) {
        Object.keys(validateSqlServerConnection(fields({ host, port }), t)).forEach((k) =>
          keys.add(k),
        );
      }
    }

    expect([...keys].sort()).toEqual(['host', 'port']);
  });
});

describe('validateSqlServerConnection — parity with the redis driver validator', () => {
  const HOSTS = ['', '   ', '127.0.0.1', 'db.internal'];
  const PORTS = ['', '  ', '1433', ' 1433 ', '0', 'abc', '1433a'];

  it('returns the same errors as redis for the same standalone input shape', () => {
    for (const host of HOSTS) {
      for (const port of PORTS) {
        const input: SharedFields = fields({
          host,
          port,
          options: { topology: 'standalone' },
        });
        const expected = redis(input, t);
        expect(sqlserver(input, t), `host=${JSON.stringify(host)} port=${JSON.stringify(port)}`)
          .toEqual(expected);
      }
    }
  });

  it('returns the same errors as redis when no options are supplied at all', () => {
    for (const host of HOSTS) {
      for (const port of PORTS) {
        const input: SharedFields = fields({ host, port });
        expect(sqlserver(input, t)).toEqual(redis(input, t));
      }
    }
  });

  it('keeps redis\'s return convention: always a string->string record', () => {
    // Both validators are `Record<string, string>`, and "valid" is spelled as an
    // empty object — never `true`, never `undefined`, never a thrown error.
    const valid: SharedFields = fields({ host: 'db.internal', port: '1433' });
    const invalid: SharedFields = fields();

    expect(redis(valid, t)).toEqual({});
    expect(sqlserver(valid, t)).toEqual({});
    expect(redis(invalid, t)).toEqual(sqlserver(invalid, t));
  });

  it('emits the same i18n keys as redis for a blank form', () => {
    expect(Object.values(sqlserver(fields(), t)).sort()).toEqual(
      Object.values(redis(fields(), t)).sort(),
    );
  });
});

describe('sqlServerValidate — the export the driver registry imports', () => {
  it('is a function taking fields and a translator', () => {
    expect(typeof sqlServerValidate).toBe('function');
    expect(sqlServerValidate.length).toBe(2);
  });

  it('delegates to validateSqlServerConnection', () => {
    const input = fields({ host: 'db.internal', port: '1433', database: 'master' });

    expect(sqlServerValidate(input, t)).toEqual(validateSqlServerConnection(input, t));
  });

  it('produces the errors ConnectionFields.tsx looks for on an empty form', () => {
    expect(sqlServerValidate(fields(), t)).toEqual({
      host: 'newConn.required',
      port: 'newConn.required',
    });
  });

  it('agrees with the redis validator on the same valid input', () => {
    const input = fields({ host: 'db.internal', port: '1433', database: '0' });

    expect(sqlServerValidate(input, t)).toEqual({});
    expect(redis(input, t)).toEqual({});
  });
});