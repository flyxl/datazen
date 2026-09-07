/**
 * Build a sanitized chat prompt from query error context and dispatch it as an
 * AiChatDraftRequest through the ContentView callback bridge.
 *
 * Reuses `buildQueryDiagnosisContext` from `src/lib/aiQueryActions.ts` for all
 * redaction/sanitisation — no secret values, parameter values, result rows, or
 * session identifiers are included in the prompt.
 *
 * §6.9 limits: SQL and error text are each capped at 4,000 characters.
 */

import {
  buildQueryDiagnosisContext,
  type QueryDiagnosisContext,
  type QueryActionBuildResult,
} from '../../../lib/aiQueryActions';
import type { AiChatDraftRequest, ContentViewCallbacks } from './aiDraftBridge';

// ── §6.9 character caps (matching aiQueryActions MAX_SAFE_TEXT_LENGTH) ─────
const MAX_PROMPT_SQL_LENGTH = 4_000;
const MAX_PROMPT_ERROR_LENGTH = 4_000;

export interface BuildQueryErrorChatPromptInput {
  panelId: string;
  connectionId: string;
  dbSessionId: string;
  database?: string | null;
  schema?: string | null;
  sql: string;
  error: string;
  databaseType?: string;
  connectionName?: string;
  serverVersion?: string;
  schemaState?: {
    currentDatabase?: string | null;
    currentSchema?: string | null;
    tables?: unknown[];
    views?: unknown[];
    columnMap?: Record<string, string[]>;
  };
}

/**
 * Build the plain-text prompt content that will be pre-filled into the AI Chat
 * textarea. Uses only the safe/redacted fields from QueryDiagnosisContext.
 */
export function buildQueryErrorChatPrompt(context: QueryDiagnosisContext): string {
  const lines: string[] = [];

  lines.push('I need help understanding and fixing a database query error.');
  lines.push('');
  lines.push(`Database type: ${context.databaseType}`);
  lines.push(`Database: ${context.database}`);
  if (context.schema) {
    lines.push(`Schema: ${context.schema}`);
  }
  lines.push('');
  lines.push('SQL:');
  lines.push('```sql');
  lines.push(context.safeSql.slice(0, MAX_PROMPT_SQL_LENGTH));
  lines.push('```');
  lines.push('');
  lines.push('Error:');
  lines.push('```');
  lines.push(context.safeErrorMessage.slice(0, MAX_PROMPT_ERROR_LENGTH));
  lines.push('```');

  return lines.join('\n');
}

/**
 * Build and send a query-error AI chat draft via the ContentView bridge.
 *
 * Returns the built request for caller inspection, or `null` when the
 * context cannot be constructed (missing SQL, error, connection, etc.).
 */
export function sendQueryErrorChatDraft(
  input: BuildQueryErrorChatPromptInput,
  callbacks?: ContentViewCallbacks,
): AiChatDraftRequest | null {
  const result: QueryActionBuildResult<QueryDiagnosisContext> = buildQueryDiagnosisContext({
    sql: input.sql,
    error: input.error,
    connectionId: input.connectionId,
    dbSessionId: input.dbSessionId,
    databaseType: input.databaseType,
    database: input.database,
    schema: input.schema,
    connectionContext: {
      connectionId: input.connectionId,
      dbSessionId: input.dbSessionId,
      name: input.connectionName,
      serverVersion: input.serverVersion,
    },
    schemaContext: input.schemaState
      ? {
          tables: input.schemaState.tables,
          views: input.schemaState.views,
          columns: input.schemaState.columnMap,
        }
      : undefined,
  });

  if (!result.ok) return null;

  const content = buildQueryErrorChatPrompt(result.context);

  const request: AiChatDraftRequest = {
    requestId: crypto.randomUUID(),
    source: 'query-error',
    panelId: input.panelId,
    connectionId: input.connectionId,
    dbSessionId: input.dbSessionId,
    database: input.database ?? undefined,
    schema: input.schema ?? undefined,
    contextFingerprint: result.context.contextFingerprint,
    content,
    focus: true,
  };

  callbacks?.openAiChatDraft(request);
  return request;
}
