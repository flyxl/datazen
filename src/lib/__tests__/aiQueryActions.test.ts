import { describe, expect, it, vi } from 'vitest';
import {
  buildExplainAction,
  buildFixSqlAction,
  buildQueryDiagnosisContext,
  buildRetryAction,
  type QueryActionInput,
} from '../aiQueryActions';

function completeInput(overrides: Partial<QueryActionInput> = {}): QueryActionInput {
  return {
    sql: "SELECT * FROM users WHERE password = 'do-not-send'",
    errorMessage: 'password=do-not-send; apiKey: hidden-key',
    connectionId: 'connection-1',
    dbSessionId: 'session-1',
    databaseType: 'postgresql',
    database: 'app',
    schema: 'public',
    connectionContext: {
      id: 'connection-1',
      dbSessionId: 'session-1',
      name: 'Production',
      host: 'postgres://user:secret@db.example.test',
      port: 5432,
      password: 'do-not-send',
      apiKey: 'also-do-not-send',
    },
    schemaContext: {
      tables: [{ name: 'users', columns: ['id', 'password'] }],
      rows: [['1', 'do-not-send']],
      resultSet: { rows: [['2']] },
      apiKey: 'schema-secret',
    },
    ...overrides,
  };
}

describe('aiQueryActions', () => {
  it('builds one redacted diagnosis context and excludes secrets/results', () => {
    const result = buildQueryDiagnosisContext(completeInput());

    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.context.sql).toContain('do-not-send');
    expect(result.context.safeSql).not.toContain('do-not-send');
    expect(result.context.errorMessage).not.toContain('hidden-key');
    expect(result.context.diagnosisParams).toEqual({
      dbSessionId: 'session-1',
      database: 'app',
      sql: expect.not.stringContaining('do-not-send'),
      errorMessage: expect.not.stringContaining('hidden-key'),
    });
    expect(result.context.connectionContext).toEqual({
      connectionId: 'connection-1',
      dbSessionId: 'session-1',
      name: 'Production',
      host: 'postgres://[REDACTED]@db.example.test',
      port: 5432,
      serverVersion: null,
      readOnly: null,
    });
    expect(result.context.promptContext.connectionContext).not.toHaveProperty('dbSessionId');
    expect(result.context.schemaContext).toEqual({
      tables: [{ name: 'users', columns: ['id', 'password'] }],
    });
    expect(result.context.contextFingerprint).toMatch(/^qctx-[0-9a-f]{8}$/);
  });

  it('returns explicit failures for empty and incomplete contexts', () => {
    expect(buildQueryDiagnosisContext(undefined)).toEqual({
      ok: false,
      error: { code: 'empty-context', message: 'Query action context is empty.' },
    });
    expect(buildQueryDiagnosisContext({ sql: 'SELECT 1' })).toMatchObject({
      ok: false,
      error: { code: 'missing-error' },
    });
    expect(
      buildQueryDiagnosisContext(
        completeInput({
          dbSessionId: undefined,
          connectionContext: { dbSessionId: undefined },
        }),
      ),
    ).toMatchObject({ ok: false, error: { code: 'missing-session' } });
    expect(buildQueryDiagnosisContext(completeInput({ database: undefined }))).toMatchObject({
      ok: false,
      error: { code: 'missing-database' },
    });
  });

  it('explain reuses the existing diagnosis params and waits for the callback', () => {
    const context = buildQueryDiagnosisContext(completeInput());
    const onExplain = vi.fn();
    const action = buildExplainAction(context);

    expect(action.enabled).toBe(true);
    expect(onExplain).not.toHaveBeenCalled();
    expect(action.invoke(onExplain)).toBe(true);
    expect(onExplain).toHaveBeenCalledWith({
      dbSessionId: 'session-1',
      database: 'app',
      sql: expect.not.stringContaining('do-not-send'),
      errorMessage: expect.not.stringContaining('hidden-key'),
    });
  });

  it('returns a Fix SQL draft with original SQL and diff without executing', () => {
    const context = buildQueryDiagnosisContext(completeInput({ sql: 'SELECT bad FROM users' }));
    const action = buildFixSqlAction(context, 'SELECT id FROM users');
    const onApply = vi.fn();

    expect(action.enabled).toBe(true);
    expect(action.draft).toMatchObject({
      originalSql: 'SELECT bad FROM users',
      sql: 'SELECT id FROM users',
      draftSql: 'SELECT id FROM users',
      diff: {
        changed: true,
        removedLines: ['SELECT bad FROM users'],
        addedLines: ['SELECT id FROM users'],
      },
    });
    expect(onApply).not.toHaveBeenCalled();
    const applied = action.applyToEditor(onApply);
    expect(applied?.originalSql).toBe('SELECT bad FROM users');
    expect(onApply).toHaveBeenCalledWith(applied);
  });

  it('disables Fix SQL for a failed context or empty suggestion', () => {
    const failed = buildFixSqlAction(buildQueryDiagnosisContext(undefined), 'SELECT 1');
    const empty = buildFixSqlAction(buildQueryDiagnosisContext(completeInput()), '  ');
    const onApply = vi.fn();

    expect(failed).toMatchObject({ enabled: false, draft: null, error: { code: 'empty-context' } });
    expect(empty).toMatchObject({
      enabled: false,
      draft: null,
      error: { code: 'missing-suggested-sql' },
    });
    expect(empty.applyToEditor(onApply)).toBeNull();
    expect(onApply).not.toHaveBeenCalled();
  });

  it('retries only the current SQL and bound params after fingerprint validation', () => {
    const context = buildQueryDiagnosisContext(completeInput({ sql: 'SELECT * FROM users WHERE id = :id' }));
    if (!context.ok) throw new Error('test context should be valid');
    const action = buildRetryAction(context, { id: 7, active: true });
    const onRetry = vi.fn();
    const current = {
      sql: 'SELECT * FROM users WHERE id = :id',
      boundParams: { active: true, id: 7 },
      contextFingerprint: context.context.contextFingerprint,
    };

    expect(action.validate(current)).toMatchObject({ ok: true });
    expect(action.invoke(current, onRetry)).toMatchObject({ ok: true });
    expect(onRetry).toHaveBeenCalledWith({
      sql: current.sql,
      boundParams: { id: 7, active: true },
      contextFingerprint: context.context.contextFingerprint,
    });
  });

  it('rejects retry when context, SQL, or params changed and does not callback', () => {
    const context = buildQueryDiagnosisContext(completeInput({ sql: 'SELECT :id' }));
    if (!context.ok) throw new Error('test context should be valid');
    const action = buildRetryAction(context, { id: 7 });
    const onRetry = vi.fn();
    const base = {
      sql: 'SELECT :id',
      boundParams: { id: 7 },
      contextFingerprint: context.context.contextFingerprint,
    };

    expect(action.invoke({ ...base, contextFingerprint: 'qctx-changed' }, onRetry)).toMatchObject({
      ok: false,
      error: { code: 'context-changed' },
    });
    expect(action.invoke({ ...base, sql: 'SELECT :other' }, onRetry)).toMatchObject({
      ok: false,
      error: { code: 'sql-changed' },
    });
    expect(action.invoke({ ...base, boundParams: { id: 8 } }, onRetry)).toMatchObject({
      ok: false,
      error: { code: 'params-changed' },
    });
    expect(onRetry).not.toHaveBeenCalled();
  });

  it('changes fingerprint when database routing or schema context changes', () => {
    const base = buildQueryDiagnosisContext(completeInput());
    const databaseChanged = buildQueryDiagnosisContext(completeInput({ database: 'other' }));
    const schemaChanged = buildQueryDiagnosisContext(completeInput({ schema: 'private' }));
    const schemaContextChanged = buildQueryDiagnosisContext(
      completeInput({ schemaContext: { tables: [{ name: 'orders' }] } }),
    );

    if (!base.ok || !databaseChanged.ok || !schemaChanged.ok || !schemaContextChanged.ok) {
      throw new Error('test contexts should be valid');
    }
    expect(databaseChanged.context.contextFingerprint).not.toBe(base.context.contextFingerprint);
    expect(schemaChanged.context.contextFingerprint).not.toBe(base.context.contextFingerprint);
    expect(schemaContextChanged.context.contextFingerprint).not.toBe(base.context.contextFingerprint);
  });
});
