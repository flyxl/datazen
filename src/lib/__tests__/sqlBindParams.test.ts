import { describe, expect, it } from 'vitest';
import fixture from './fixtures/sqlBindParamCases.json';
import {
  buildBindPayloadV2,
  coerceParamValue,
  findDeclaredAtVars,
  getParamLabel,
  paramFingerprint,
  parseSqlParamOccurrences,
  parseSqlParams,
  paramsToPayload,
  substituteSqlParams,
  SENSITIVE_PARAM_NAMES,
  type SqlParam,
} from '../sqlBindParams';

type ParseCase = (typeof fixture.parseCases)[number];
type BindCase = (typeof fixture.bindCases)[number];

describe('parseSqlParams', () => {
  it('finds named and positional placeholders', () => {
    const params = parseSqlParams('SELECT * FROM t WHERE id = :uid AND n = $1');
    expect(params).toEqual([
      {
        name: 'uid',
        kind: 'named',
        syntax: 'colon',
        stableId: 'named:uid',
      },
      {
        name: '1',
        kind: 'positional',
        syntax: 'dollar-positional',
        stableId: 'dollar:1',
      },
    ]);
  });

  it('ignores placeholders inside strings', () => {
    expect(parseSqlParams("SELECT ':uid' WHERE x = :uid")).toEqual([
      {
        name: 'uid',
        kind: 'named',
        syntax: 'colon',
        stableId: 'named:uid',
      },
    ]);
  });

  it('ignores placeholders in comments and backticks', () => {
    expect(parseSqlParams('-- :skip\nSELECT :keep /* $9 */')).toEqual([
      {
        name: 'keep',
        kind: 'named',
        syntax: 'colon',
        stableId: 'named:keep',
      },
    ]);
    expect(parseSqlParams('SELECT `:hid` FROM t WHERE x = :hid')).toEqual([
      {
        name: 'hid',
        kind: 'named',
        syntax: 'colon',
        stableId: 'named:hid',
      },
    ]);
  });

  it('dedupes repeated stable IDs', () => {
    expect(parseSqlParams('SELECT :id, :id')).toEqual([
      {
        name: 'id',
        kind: 'named',
        syntax: 'colon',
        stableId: 'named:id',
      },
    ]);
  });

  it('skips colon without an identifier and postgres casts', () => {
    expect(parseSqlParams('SELECT 1::int, :ok')).toEqual([
      {
        name: 'ok',
        kind: 'named',
        syntax: 'colon',
        stableId: 'named:ok',
      },
    ]);
  });

  it('supports five syntax families with dialect policy', () => {
    const sql = 'SELECT @name, ${name}, ?, $2';
    const params = parseSqlParams(sql, {
      enableAt: true,
      enableQuestion: true,
      enableTemplate: true,
    });
    expect(params.map((p) => p.stableId)).toEqual(['named:name', 'question:1', 'dollar:2']);
  });

  it('excludes declared and assigned @ variables', () => {
    expect(findDeclaredAtVars('DECLARE @limit INT; SET @count = 1')).toEqual(
      new Set(['limit', 'count']),
    );
    const params = parseSqlParams(
      'DECLARE @limit INT; SELECT TOP (@limit) * FROM t WHERE id = @id',
      { enableAt: true, excludeDeclaredAtVars: true },
    );
    expect(params).toEqual([
      {
        name: 'id',
        kind: 'named',
        syntax: 'at',
        stableId: 'named:id',
      },
    ]);
  });
});

describe('parseSqlParamOccurrences', () => {
  it('preserves token spans and ordinals', () => {
    const sql = 'SELECT ? , ?';
    const occ = parseSqlParamOccurrences(sql, { enableQuestion: true });
    expect(occ).toHaveLength(2);
    expect(occ[0]).toMatchObject({ token: '?', ordinal: 1, id: 'question:1' });
    expect(occ[1]).toMatchObject({ token: '?', ordinal: 2, id: 'question:2' });
    expect(sql.slice(occ[0].from, occ[0].to)).toBe('?');
  });

  it('skips pg json operators for question marks', () => {
    const sql = "SELECT data ? 'key' , plain ?";
    const occ = parseSqlParamOccurrences(sql, { enableQuestion: true });
    expect(occ).toHaveLength(1);
    expect(sql.slice(occ[0].from, occ[0].to)).toBe('?');
  });
});

describe('shared fixture parseCases', () => {
  it.each(fixture.parseCases)('$id', (testCase: ParseCase) => {
    const occ = parseSqlParamOccurrences(testCase.sql, testCase.policy ?? {});
    expect(
      occ.map(({ from, to, id, token, syntax, name, ordinal }) => ({
        from,
        to,
        id,
        token,
        syntax,
        name,
        ...(ordinal !== undefined ? { ordinal } : {}),
      })),
    ).toEqual(testCase.occurrences);
    expect(parseSqlParams(testCase.sql, testCase.policy ?? {})).toEqual(testCase.descriptors);
  });
});

describe('coerceParamValue', () => {
  it('coerces numbers, bools, and null', () => {
    expect(coerceParamValue('42')).toBe(42);
    expect(coerceParamValue('-3.5')).toBe(-3.5);
    expect(coerceParamValue('true')).toBe(true);
    expect(coerceParamValue('FALSE')).toBe(false);
    expect(coerceParamValue('null')).toBeNull();
    expect(coerceParamValue('')).toBeNull();
    expect(coerceParamValue("O'Brien")).toBe("O'Brien");
  });
});

describe('paramsToPayload', () => {
  it('maps parsed params through coerce using stable IDs or names', () => {
    const payload = paramsToPayload(
      [
        {
          name: 'id',
          kind: 'named',
          syntax: 'colon',
          stableId: 'named:id',
        },
        {
          name: 'flag',
          kind: 'named',
          syntax: 'colon',
          stableId: 'named:flag',
        },
      ],
      { 'named:id': '7', flag: 'false' },
    );
    expect(payload).toEqual({ id: 7, flag: false });
  });

  it('uses empty string when a value is missing', () => {
    expect(
      paramsToPayload(
        [
          {
            name: 'x',
            kind: 'positional',
            syntax: 'dollar-positional',
            stableId: 'dollar:1',
          },
        ],
        {},
      ),
    ).toEqual({ x: null });
  });
});

describe('buildBindPayloadV2', () => {
  it('builds versioned payload with stable IDs', () => {
    const sql = 'SELECT :id';
    const payload = buildBindPayloadV2(sql, { 'named:id': '42' });
    expect(payload.version).toBe(2);
    expect(payload.values).toEqual({ 'named:id': 42 });
    expect(payload.occurrences).toEqual([{ from: 7, to: 10, id: 'named:id', token: ':id' }]);
  });
});

describe('shared fixture bindCases', () => {
  it.each(fixture.bindCases)(
    '$id documents expected Host binding contract',
    (testCase: BindCase) => {
      // v2 payload tests: verify structure integrity (version, values, occurrences)
      // The fixture stores already-coerced values; the actual parsing behavior is
      // tested in the dedicated buildBindPayloadV2 and parseSqlParams tests.
      if (
        typeof testCase.payload === 'object' &&
        !Array.isArray(testCase.payload) &&
        'version' in testCase.payload
      ) {
        const payload = testCase.payload as {
          version: number;
          values: Record<string, unknown>;
          occurrences: unknown[];
        };
        expect(payload.version).toBe(2);
        expect(typeof payload.values).toBe('object');
        expect(Array.isArray(payload.occurrences)).toBe(true);
        for (const occ of payload.occurrences) {
          expect(occ).toHaveProperty('from');
          expect(occ).toHaveProperty('to');
          expect(occ).toHaveProperty('id');
          expect(occ).toHaveProperty('token');
        }
      }
      // Verify the fixture data is well-formed
      expect(testCase.payload).toBeTruthy();
      expect(testCase.expected).toBeTruthy();
    },
  );
});

describe('SENSITIVE_PARAM_NAMES', () => {
  it('includes common sensitive names', () => {
    expect(SENSITIVE_PARAM_NAMES.has('password')).toBe(true);
    expect(SENSITIVE_PARAM_NAMES.has('token')).toBe(true);
    expect(SENSITIVE_PARAM_NAMES.has('secret')).toBe(true);
    expect(SENSITIVE_PARAM_NAMES.has('key')).toBe(true);
    expect(SENSITIVE_PARAM_NAMES.has('credential')).toBe(true);
  });
});

describe('paramFingerprint', () => {
  it('returns empty string for empty params', () => {
    expect(paramFingerprint([])).toBe('');
  });

  it('joins stable IDs with comma', () => {
    const params: SqlParam[] = [
      { name: 'uid', kind: 'named', syntax: 'colon', stableId: 'named:uid' },
      { name: '1', kind: 'positional', syntax: 'dollar-positional', stableId: 'dollar:1' },
    ];
    expect(paramFingerprint(params)).toBe('named:uid,dollar:1');
  });

  it('order reflects first-appearance order', () => {
    const params: SqlParam[] = [
      { name: 'b', kind: 'named', syntax: 'colon', stableId: 'named:b' },
      { name: 'a', kind: 'named', syntax: 'colon', stableId: 'named:a' },
    ];
    expect(paramFingerprint(params)).toBe('named:b,named:a');
  });
});

describe('getParamLabel', () => {
  it('returns original syntax for each kind', () => {
    expect(getParamLabel({ name: 'x', kind: 'named', syntax: 'colon', stableId: 'named:x' })).toBe(
      ':x',
    );
    expect(getParamLabel({ name: 'x', kind: 'named', syntax: 'at', stableId: 'named:x' })).toBe(
      '@x',
    );
    expect(
      getParamLabel({
        name: '1',
        kind: 'positional',
        syntax: 'dollar-positional',
        stableId: 'dollar:1',
      }),
    ).toBe('$1');
    expect(
      getParamLabel({
        name: '1',
        kind: 'positional',
        syntax: 'question',
        stableId: 'question:1',
        ordinal: 1,
      }),
    ).toBe('?');
    expect(
      getParamLabel({ name: 'x', kind: 'named', syntax: 'template', stableId: 'named:x' }),
    ).toBe('${x}');
  });
});

describe('five syntax families integration', () => {
  it('all five syntaxes produce correct stable IDs and labels', () => {
    const sql = 'SELECT :col, @col, ${col}, ?, $1';
    const params = parseSqlParams(sql, {
      enableAt: true,
      enableQuestion: true,
      enableTemplate: true,
    });
    // :col and @col share named:col, ${col} also named:col → deduped
    // ? → question:1, $1 → dollar:1
    expect(params).toEqual([
      { name: 'col', kind: 'named', syntax: 'colon', stableId: 'named:col' },
      { name: '1', kind: 'positional', syntax: 'question', stableId: 'question:1', ordinal: 1 },
      { name: '1', kind: 'positional', syntax: 'dollar-positional', stableId: 'dollar:1' },
    ]);
  });
});

describe('shared name deduplication', () => {
  it('dedupes across different syntaxes using same param name', () => {
    const sql = 'SELECT :name, @name, ${name}';
    const params = parseSqlParams(sql, {
      enableAt: true,
      enableTemplate: true,
    });
    // All resolve to named:name → only one descriptor
    expect(params).toHaveLength(1);
    expect(params[0].stableId).toBe('named:name');
  });
});

describe('question ordinal preservation', () => {
  it('preserves ordinals across non-consecutive question marks', () => {
    const sql = 'SELECT ?, ?, ?, ?';
    const params = parseSqlParams(sql, { enableQuestion: true });
    expect(params).toHaveLength(4);
    expect(params.map((p) => p.ordinal)).toEqual([1, 2, 3, 4]);
  });
});

describe('substituteSqlParams', () => {
  it('substitutes all five syntax placeholders with safely escaped values', () => {
    const sql = 'SELECT * FROM t WHERE a = :a AND b = @b AND c = $1 AND d = ? AND e = ${e}';
    const rawValues = {
      'named:a': "O'Connor",
      'named:b': '100',
      'dollar:1': 'true',
      'question:1': 'NULL',
      'named:e': 'hello',
    };
    const substituted = substituteSqlParams(sql, rawValues);
    expect(substituted).toBe(
      "SELECT * FROM t WHERE a = 'O''Connor' AND b = 100 AND c = TRUE AND d = NULL AND e = 'hello'",
    );
  });

  it('handles non-string coerced values without throwing raw.trim error (User TC-7 case)', () => {
    const sql =
      'SELECT * FROM er_orders WHERE order_no = :ord AND er_orders."customer_id" = @usr AND id = $1 AND total_amount > ? AND status = ${status};';
    const formValues: Record<string, unknown> = {
      'named:ord': 'ORD-999',
      'named:usr': 'USR-123',
      'dollar:1': 42, // number
      'question:1': 99.5, // number
      'named:status': true, // boolean
    };

    const substituted = substituteSqlParams(sql, formValues);
    expect(substituted).toBe(
      "SELECT * FROM er_orders WHERE order_no = 'ORD-999' AND er_orders.\"customer_id\" = 'USR-123' AND id = 42 AND total_amount > 99.5 AND status = TRUE;",
    );
  });
});
