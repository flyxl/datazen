import { describe, expect, it, vi } from 'vitest';
import {
  buildQueryErrorChatPrompt,
  sendQueryErrorChatDraft,
  type BuildQueryErrorChatPromptInput,
} from '../queryErrorChatPrompt';
import type { QueryDiagnosisContext } from '../../../../lib/aiQueryActions';
import type { ContentViewCallbacks } from '../aiDraftBridge';

function makeContext(overrides: Partial<QueryDiagnosisContext> = {}): QueryDiagnosisContext {
  return {
    sql: "SELECT * FROM users WHERE password = 'do-not-send'",
    safeSql: 'SELECT * FROM users WHERE password = [REDACTED]',
    safeErrorMessage: 'password=[REDACTED]; apiKey: [REDACTED]',
    errorMessage: 'password=[REDACTED]; apiKey: [REDACTED]',
    databaseType: 'postgresql',
    database: 'app',
    schema: 'public',
    schemaContext: { tables: [{ name: 'users' }] },
    connectionContext: {
      connectionId: 'connection-1',
      dbSessionId: 'session-1',
      name: 'Production',
      host: 'postgres://[REDACTED]@db.example.test',
      port: 5432,
      serverVersion: null,
      readOnly: null,
    },
    promptContext: {
      sql: 'SELECT * FROM users WHERE password = [REDACTED]',
      errorMessage: 'password=[REDACTED]; apiKey: [REDACTED]',
      databaseType: 'postgresql',
      database: 'app',
      schema: 'public',
      schemaContext: { tables: [{ name: 'users' }] },
      connectionContext: {
        name: 'Production',
        host: 'postgres://[REDACTED]@db.example.test',
        port: 5432,
        serverVersion: null,
        readOnly: null,
      },
    },
    diagnosisParams: {
      dbSessionId: 'session-1',
      database: 'app',
      sql: 'SELECT * FROM users WHERE password = [REDACTED]',
      errorMessage: 'password=[REDACTED]; apiKey: [REDACTED]',
    },
    contextFingerprint: 'qctx-00000001',
    ...overrides,
  };
}

function makeInput(
  overrides: Partial<BuildQueryErrorChatPromptInput> = {},
): BuildQueryErrorChatPromptInput {
  return {
    panelId: 'panel-1',
    connectionId: 'connection-1',
    dbSessionId: 'session-1',
    database: 'app',
    schema: 'public',
    sql: "SELECT * FROM users WHERE password = 'do-not-send'",
    error: 'password=do-not-send; apiKey: hidden-key',
    databaseType: 'postgresql',
    connectionName: 'Production',
    ...overrides,
  };
}

describe('[tester] queryErrorChatPrompt', () => {
  describe('buildQueryErrorChatPrompt', () => {
    it('includes database type, database, schema, SQL, and error in the prompt', () => {
      const context = makeContext();
      const prompt = buildQueryErrorChatPrompt(context);

      expect(prompt).toContain('Database type: postgresql');
      expect(prompt).toContain('Database: app');
      expect(prompt).toContain('Schema: public');
      expect(prompt).toContain('```sql');
      expect(prompt).toContain('```');
      expect(prompt).toContain('Error:');
      expect(prompt).toContain('I need help understanding and fixing a database query error.');
    });

    it('uses safeSql and safeErrorMessage (redacted)', () => {
      const context = makeContext({
        safeSql: 'SELECT 1',
        safeErrorMessage: 'syntax error',
      });
      const prompt = buildQueryErrorChatPrompt(context);

      expect(prompt).toContain('SELECT 1');
      expect(prompt).toContain('syntax error');
      // Original unredacted values must not appear.
      expect(prompt).not.toContain('do-not-send');
      expect(prompt).not.toContain('hidden-key');
    });

    it('omits schema line when schema is null', () => {
      const context = makeContext({ schema: null });
      const prompt = buildQueryErrorChatPrompt(context);

      expect(prompt).not.toContain('Schema:');
      expect(prompt).toContain('Database type: postgresql');
    });

    it('caps SQL and error to 4,000 characters each', () => {
      const longSql = 'SELECT * FROM t WHERE a = '.padEnd(5_000, 'x');
      const longError = 'Error at line 1: '.padEnd(6_000, 'y');
      const context = makeContext({
        safeSql: longSql,
        safeErrorMessage: longError,
      });
      const prompt = buildQueryErrorChatPrompt(context);

      // The prompt should not contain the full 5,000-char SQL.
      expect(prompt.length).toBeLessThan(10_000);
    });

    it('does not include connectionId or dbSessionId', () => {
      const context = makeContext();
      const prompt = buildQueryErrorChatPrompt(context);

      expect(prompt).not.toContain('connection-1');
      expect(prompt).not.toContain('session-1');
    });
  });

  describe('sendQueryErrorChatDraft', () => {
    it('returns a valid AiChatDraftRequest with correct fields', () => {
      const input = makeInput();
      const request = sendQueryErrorChatDraft(input);

      expect(request).not.toBeNull();
      expect(request!.source).toBe('query-error');
      expect(request!.panelId).toBe('panel-1');
      expect(request!.connectionId).toBe('connection-1');
      expect(request!.dbSessionId).toBe('session-1');
      expect(request!.database).toBe('app');
      expect(request!.schema).toBe('public');
      expect(request!.contextFingerprint).toMatch(/^qctx-/);
      expect(request!.focus).toBe(true);
      expect(request!.content).toContain('I need help understanding');
      expect(request!.content).toContain('Database type: postgresql');
    });

    it('calls callbacks.openAiChatDraft when provided', () => {
      const openAiChatDraft = vi.fn();
      const callbacks: ContentViewCallbacks = {
        openRelation: vi.fn(),
        openAiChatDraft,
      };

      sendQueryErrorChatDraft(makeInput(), callbacks);

      expect(openAiChatDraft).toHaveBeenCalledTimes(1);
      expect(openAiChatDraft).toHaveBeenCalledWith(
        expect.objectContaining({
          source: 'query-error',
          panelId: 'panel-1',
          content: expect.stringContaining('Database type:'),
        }),
      );
    });

    it('returns null when input has no SQL', () => {
      const request = sendQueryErrorChatDraft(makeInput({ sql: '' }));
      expect(request).toBeNull();
    });

    it('returns null when input has no error', () => {
      const request = sendQueryErrorChatDraft(makeInput({ error: '' }));
      expect(request).toBeNull();
    });

    it('returns null when input has no connectionId', () => {
      const request = sendQueryErrorChatDraft(makeInput({ connectionId: '' }));
      expect(request).toBeNull();
    });

    it('returns null when input has no dbSessionId', () => {
      const request = sendQueryErrorChatDraft(makeInput({ dbSessionId: '' }));
      expect(request).toBeNull();
    });

    it('returns null when input has no databaseType', () => {
      const request = sendQueryErrorChatDraft(makeInput({ databaseType: undefined }));
      expect(request).toBeNull();
    });

    it('returns null when input has no database', () => {
      const request = sendQueryErrorChatDraft(makeInput({ database: undefined }));
      expect(request).toBeNull();
    });

    it('does not call callbacks when request cannot be built', () => {
      const openAiChatDraft = vi.fn();
      const callbacks: ContentViewCallbacks = {
        openRelation: vi.fn(),
        openAiChatDraft,
      };

      sendQueryErrorChatDraft(makeInput({ sql: '' }), callbacks);
      expect(openAiChatDraft).not.toHaveBeenCalled();
    });

    it('generates unique requestId for each call', () => {
      const request1 = sendQueryErrorChatDraft(makeInput());
      const request2 = sendQueryErrorChatDraft(makeInput());

      expect(request1).not.toBeNull();
      expect(request2).not.toBeNull();
      expect(request1!.requestId).not.toBe(request2!.requestId);
    });

    it('prompt does not contain parameter values or result rows', () => {
      const request = sendQueryErrorChatDraft(
        makeInput({
          schemaState: {
            tables: [{ name: 'users' }],
            views: [],
            columnMap: { users: ['id', 'password'] },
          },
        }),
      );

      expect(request).not.toBeNull();
      // The content should not include secret-shaped data.
      expect(request!.content).not.toContain('password');
      expect(request!.content).not.toContain('apiKey');
    });
  });
});
