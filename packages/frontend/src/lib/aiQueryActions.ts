/**
 * Pure adapters for the query error AI actions.
 *
 * This module deliberately does not import a store or an IPC command. The
 * future QueryErrorPanel integration can use the payloads and callbacks here
 * without giving an AI action permission to execute SQL or create a workflow.
 */

export type BoundParamValue = string | number | boolean | null;
export type BoundParams = Record<string, BoundParamValue>;

export type SanitizedJson =
  | null
  | boolean
  | number
  | string
  | SanitizedJson[]
  | { [key: string]: SanitizedJson };

export interface QueryActionInput {
  /** The editor SQL. It is preserved byte-for-byte for Fix SQL and Retry. */
  sql?: unknown;
  /** Either an Error/string or an object with a message property. */
  error?: unknown;
  /** Alias accepted by callers that already use the AI command field name. */
  errorMessage?: unknown;
  connectionId?: unknown;
  dbSessionId?: unknown;
  databaseType?: unknown;
  driverType?: unknown;
  database?: unknown;
  currentDatabase?: unknown;
  schema?: unknown;
  currentSchema?: unknown;
  connectionName?: unknown;
  host?: unknown;
  port?: unknown;
  serverVersion?: unknown;
  readOnly?: unknown;
  /** Current connection metadata. Only a safe allowlist is retained. */
  connectionContext?: unknown;
  /** Schema/table metadata; result rows and secret-shaped fields are removed. */
  schemaContext?: unknown;
}

export interface SanitizedConnectionContext {
  /** Identifiers are retained for local routing/fingerprint checks only. */
  connectionId: string;
  dbSessionId: string;
  name: string | null;
  host: string | null;
  port: number | null;
  serverVersion: string | null;
  readOnly: boolean | null;
}

export interface QueryPromptContext {
  sql: string;
  errorMessage: string;
  databaseType: string;
  database: string;
  schema: string | null;
  schemaContext: SanitizedJson | null;
  /** Deliberately excludes connection/session identifiers. */
  connectionContext: Omit<SanitizedConnectionContext, 'connectionId' | 'dbSessionId'>;
}

/** The existing `ai_diagnose_error` input; do not add a new command shape here. */
export interface QueryDiagnosisParams {
  dbSessionId: string;
  database: string;
  sql: string;
  errorMessage: string;
}

export interface QueryDiagnosisContext {
  /** Original editor text used by Fix SQL and Retry. */
  sql: string;
  /** Redacted SQL that is safe to pass to an AI command. */
  safeSql: string;
  /** Redacted error text that is safe to pass to an AI command. */
  safeErrorMessage: string;
  /** @deprecated Use safeErrorMessage for AI-bound error text. */
  errorMessage: string;
  databaseType: string;
  database: string;
  schema: string | null;
  schemaContext: SanitizedJson | null;
  connectionContext: SanitizedConnectionContext;
  /** Safe, prompt-ready representation of the complete diagnosis context. */
  promptContext: QueryPromptContext;
  /** Existing diagnosis command parameters, with safe SQL/error fields. */
  diagnosisParams: QueryDiagnosisParams;
  /** Changes when the database/session/namespace/schema context changes. */
  contextFingerprint: string;
}

export type QueryActionErrorCode =
  | 'empty-context'
  | 'missing-sql'
  | 'missing-error'
  | 'missing-connection'
  | 'missing-session'
  | 'missing-database-type'
  | 'missing-database'
  | 'missing-suggested-sql'
  | 'context-changed'
  | 'sql-changed'
  | 'params-changed';

export interface QueryActionError {
  code: QueryActionErrorCode;
  message: string;
}

export interface QueryActionBuildSuccess<T> {
  ok: true;
  context: T;
}

export interface QueryActionBuildFailure {
  ok: false;
  error: QueryActionError;
}

export type QueryActionBuildResult<T> = QueryActionBuildSuccess<T> | QueryActionBuildFailure;

const MAX_SAFE_TEXT_LENGTH = 4_000;
const MAX_SCHEMA_DEPTH = 4;
const MAX_SCHEMA_ARRAY_ITEMS = 100;
const MAX_SCHEMA_OBJECT_KEYS = 100;

const SECRET_KEY_WORDS = new Set([
  'password',
  'passwd',
  'pwd',
  'secret',
  'token',
  'authorization',
  'bearer',
  'credential',
  'credentials',
  'passphrase',
  'key',
]);
const SECRET_KEY_PATTERN =
  /^(?:api|auth|oauth|access|refresh|client|private|encryption|signing)?(?:token|key|secret|password|passwd|pwd|credential|credentials|authorization|bearer|passphrase)$/i;
const RESULT_KEY_NAMES = new Set([
  'data',
  'row',
  'rows',
  'record',
  'records',
  'result',
  'results',
  'resultset',
  'resultsets',
  'resultsset',
  'resultssets',
  'resultrow',
  'resultrows',
  'resultsrow',
  'resultsrows',
  'resultdata',
  'resultsdata',
  'queryresult',
  'queryresults',
  'queryresultset',
  'queryresultsets',
  'queryresultsset',
  'queryresultssets',
  'queryresultrow',
  'queryresultrows',
  'queryresultsrow',
  'queryresultsrows',
  'queryresultdata',
  'queryresultsdata',
  'samplerow',
  'samplerows',
  'rawoutput',
  'executionoutput',
  'payload',
]);

function keyWords(key: string): string[] {
  return key
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .split(/[^a-zA-Z0-9]+/)
    .filter(Boolean)
    .map((word) => word.toLowerCase());
}

function isSensitiveKey(key: string): boolean {
  const words = keyWords(key);
  return (
    words.some((word) => SECRET_KEY_WORDS.has(word)) ||
    SECRET_KEY_PATTERN.test(words.join(''))
  );
}

function isResultKey(key: string): boolean {
  return RESULT_KEY_NAMES.has(keyWords(key).join(''));
}

const SENSITIVE_ASSIGNMENT_PATTERN =
  /(?<![\w])(?:(?:\\)?["'])?([A-Za-z][\w.-]*)(?:(?:\\)?["'])?\s*(?:\\?:|=)\s*/gi;

function asRecord(value: unknown): Record<string, unknown> | null {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return null;
  return value as Record<string, unknown>;
}

function nonEmptyString(value: unknown): string | null {
  return typeof value === 'string' && value.trim() ? value.trim() : null;
}

function firstString(...values: unknown[]): string | null {
  for (const value of values) {
    const result = nonEmptyString(value);
    if (result) return result;
  }
  return null;
}

function readRecordString(record: Record<string, unknown> | null, ...keys: string[]): string | null {
  if (!record) return null;
  return firstString(...keys.map((key) => record[key]));
}

function readRecordNumber(record: Record<string, unknown> | null, ...keys: string[]): number | null {
  if (!record) return null;
  for (const key of keys) {
    const value = record[key];
    if (typeof value === 'number' && Number.isFinite(value)) return value;
  }
  return null;
}

function readRecordBoolean(
  record: Record<string, unknown> | null,
  ...keys: string[]
): boolean | null {
  if (!record) return null;
  for (const key of keys) {
    const value = record[key];
    if (typeof value === 'boolean') return value;
  }
  return null;
}

function consumeAssignedValue(value: string, start: number): number {
  let index = start;
  while (index < value.length && /\s/.test(value[index] ?? '')) index += 1;
  let quote = value[index];
  if (quote === '\\' && (value[index + 1] === '"' || value[index + 1] === "'")) {
    quote = value[index + 1];
    index += 2;
  } else if (quote === '"' || quote === "'" || quote === '`') {
    index += 1;
  }
  if (quote === '"' || quote === "'" || quote === '`') {
    while (index < value.length) {
      if (value[index] === '\\') {
        index += 2;
      } else if (value[index] === quote) {
        return index + 1;
      } else {
        index += 1;
      }
    }
    return index;
  }
  while (index < value.length && !/[\s,;,)\]}]/.test(value[index] ?? '')) index += 1;
  return index;
}

function redactSensitiveAssignments(value: string): string {
  let result = '';
  let cursor = 0;
  for (const match of value.matchAll(SENSITIVE_ASSIGNMENT_PATTERN)) {
    const matchIndex = match.index ?? 0;
    const key = match[1] ?? '';
    if (!isSensitiveKey(key)) continue;

    if (matchIndex < cursor) continue;
    const valueStart = matchIndex + match[0].length;
    const valueEnd = consumeAssignedValue(value, valueStart);
    result += value.slice(cursor, matchIndex) + '[REDACTED]';
    cursor = valueEnd;
  }
  return result + value.slice(cursor);
}

/** Remove obvious URI credentials and key/value secrets from non-JSON text. */
function redactSensitivePlainText(value: string): string {
  return redactSensitiveAssignments(
    value
    .replace(
      /([a-z][\w+.-]*:\/\/)(?:[^/\s:@]+)(?::[^@\s/]*)?@/gi,
      '$1[REDACTED]@',
    )
    .replace(
      /(?:\bBearer\s+)[^\s,;)]+/gi,
      'Bearer [REDACTED]',
    ),
  );
}

function sanitizeSerializedJsonValue(value: unknown, depth = 0): SanitizedJson | null {
  if (value === null) return null;
  if (typeof value === 'string') return redactSensitivePlainText(value);
  if (typeof value === 'boolean') return value;
  if (typeof value === 'number') return Number.isFinite(value) ? value : null;
  if (typeof value !== 'object') return null;
  if (depth >= MAX_SCHEMA_DEPTH) return '[truncated]';

  if (Array.isArray(value)) {
    return value
      .slice(0, MAX_SCHEMA_ARRAY_ITEMS)
      .map((item) => sanitizeSerializedJsonValue(item, depth + 1));
  }

  const result: { [key: string]: SanitizedJson } = {};
  for (const key of Object.keys(value as Record<string, unknown>).slice(0, MAX_SCHEMA_OBJECT_KEYS)) {
    if (isSensitiveKey(key) || isResultKey(key)) continue;
    result[key] = sanitizeSerializedJsonValue(
      (value as Record<string, unknown>)[key],
      depth + 1,
    );
  }
  return result;
}

function redactSerializedJson(value: string): string | null {
  const trimmed = value.trim();
  if (
    !(
      (trimmed.startsWith('{') && trimmed.endsWith('}')) ||
      (trimmed.startsWith('[') && trimmed.endsWith(']')) ||
      (trimmed.startsWith('"') && trimmed.endsWith('"'))
    )
  ) {
    return null;
  }

  try {
    const parsed = JSON.parse(trimmed) as unknown;
    return JSON.stringify(sanitizeSerializedJsonValue(parsed));
  } catch {
    return null;
  }
}

/** Remove secrets at the structured-JSON boundary before text reaches an AI prompt. */
export function redactSensitiveText(value: string): string {
  return (redactSerializedJson(value) ?? redactSensitivePlainText(value)).slice(
    0,
    MAX_SAFE_TEXT_LENGTH,
  );
}

function sanitizeSchemaValue(
  value: unknown,
  depth = 0,
  seen = new Set<object>(),
): SanitizedJson | null {
  if (value === null) return null;
  if (typeof value === 'string') return redactSensitiveText(value);
  if (typeof value === 'boolean') return value;
  if (typeof value === 'number') return Number.isFinite(value) ? value : null;
  if (typeof value !== 'object') return null;
  if (seen.has(value)) return null;
  if (depth >= MAX_SCHEMA_DEPTH) return '[truncated]';

  seen.add(value);
  if (Array.isArray(value)) {
    const result = value
      .slice(0, MAX_SCHEMA_ARRAY_ITEMS)
      .map((item) => sanitizeSchemaValue(item, depth + 1, seen));
    seen.delete(value);
    return result;
  }

  const record = value as Record<string, unknown>;
  const result: { [key: string]: SanitizedJson } = {};
  for (const key of Object.keys(record).slice(0, MAX_SCHEMA_OBJECT_KEYS)) {
    if (isSensitiveKey(key) || isResultKey(key)) continue;
    const sanitized = sanitizeSchemaValue(record[key], depth + 1, seen);
    if (sanitized !== null || record[key] === null) result[key] = sanitized;
  }
  seen.delete(value);
  return result;
}

/** Sanitize arbitrary schema metadata without ever carrying result rows. */
export function sanitizeSchemaContext(value: unknown): SanitizedJson | null {
  return sanitizeSchemaValue(value);
}

function sanitizeHost(value: string | null): string | null {
  if (!value) return null;
  return value
    .replace(/(^|\/\/)([^/\s:@]+):([^@\s/]+)@/g, '$1[REDACTED]@')
    .slice(0, 512);
}

function normalizeErrorMessage(value: unknown): string | null {
  if (typeof value === 'string') return nonEmptyString(value);
  if (value instanceof Error) return nonEmptyString(value.message);
  const record = asRecord(value);
  return readRecordString(record, 'message', 'error', 'detail');
}

function buildConnectionContext(
  input: Record<string, unknown>,
  rawConnectionContext: Record<string, unknown> | null,
  connectionId: string,
  dbSessionId: string,
): SanitizedConnectionContext {
  const name = firstString(
    input.connectionName,
    rawConnectionContext?.connectionName,
    rawConnectionContext?.name,
  );
  const host = sanitizeHost(firstString(input.host, rawConnectionContext?.host));
  const port =
    readRecordNumber(input, 'port') ?? readRecordNumber(rawConnectionContext, 'port');
  const serverVersion = firstString(
    input.serverVersion,
    rawConnectionContext?.serverVersion,
  );
  const readOnly =
    readRecordBoolean(input, 'readOnly') ?? readRecordBoolean(rawConnectionContext, 'readOnly');

  return {
    connectionId,
    dbSessionId,
    name,
    host,
    port,
    serverVersion,
    readOnly,
  };
}

function canonicalize(value: unknown): string {
  if (value === null) return 'null';
  if (typeof value === 'string' || typeof value === 'boolean') return JSON.stringify(value);
  if (typeof value === 'number') return Number.isFinite(value) ? String(value) : 'null';
  if (Array.isArray(value)) return `[${value.map(canonicalize).join(',')}]`;
  if (typeof value === 'object') {
    const record = value as Record<string, unknown>;
    return `{${Object.keys(record)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalize(record[key])}`)
      .join(',')}}`;
  }
  return 'null';
}

function fingerprint(value: unknown): string {
  let hash = 0x811c9dc5;
  for (const character of canonicalize(value)) {
    hash ^= character.codePointAt(0) ?? 0;
    hash = Math.imul(hash, 0x01000193);
  }
  return `qctx-${(hash >>> 0).toString(16).padStart(8, '0')}`;
}

function failure(code: QueryActionErrorCode, message: string): QueryActionBuildFailure {
  return { ok: false, error: { code, message } };
}

/** Build one bounded, redacted context for all three query error actions. */
export function buildQueryDiagnosisContext(
  value: QueryActionInput | null | undefined,
): QueryActionBuildResult<QueryDiagnosisContext> {
  const input = asRecord(value);
  if (!input || Object.keys(input).length === 0) {
    return failure('empty-context', 'Query action context is empty.');
  }

  const rawConnectionContext = asRecord(input.connectionContext);
  const sql = typeof input.sql === 'string' ? input.sql : null;
  if (!sql?.trim()) return failure('missing-sql', 'Query action context has no SQL.');

  const rawError = input.errorMessage ?? input.error;
  const errorMessage = normalizeErrorMessage(rawError);
  if (!errorMessage) return failure('missing-error', 'Query action context has no error message.');

  const connectionId = firstString(
    input.connectionId,
    rawConnectionContext?.connectionId,
    rawConnectionContext?.id,
  );
  if (!connectionId) return failure('missing-connection', 'Query action context has no connection.');

  const dbSessionId = firstString(input.dbSessionId, rawConnectionContext?.dbSessionId);
  if (!dbSessionId) return failure('missing-session', 'Query action context has no database session.');

  const databaseType = firstString(
    input.databaseType,
    input.driverType,
    rawConnectionContext?.databaseType,
    rawConnectionContext?.driverType,
  );
  if (!databaseType) return failure('missing-database-type', 'Query action context has no database type.');

  const database = firstString(
    input.database,
    input.currentDatabase,
    rawConnectionContext?.database,
    rawConnectionContext?.currentDatabase,
  );
  if (!database) return failure('missing-database', 'Query action context has no database.');

  const schema = firstString(
    input.schema,
    input.currentSchema,
    rawConnectionContext?.schema,
    rawConnectionContext?.currentSchema,
  );
  const safeSql = redactSensitiveText(sql);
  const safeErrorMessage = redactSensitiveText(errorMessage);
  const schemaContext = sanitizeSchemaContext(input.schemaContext);
  const connectionContext = buildConnectionContext(
    input,
    rawConnectionContext,
    connectionId,
    dbSessionId,
  );
  const promptConnectionContext = {
    name: connectionContext.name,
    host: connectionContext.host,
    port: connectionContext.port,
    serverVersion: connectionContext.serverVersion,
    readOnly: connectionContext.readOnly,
  };
  const promptContext: QueryPromptContext = {
    sql: safeSql,
    errorMessage: safeErrorMessage,
    databaseType,
    database,
    schema,
    schemaContext,
    connectionContext: promptConnectionContext,
  };
  const contextFingerprint = fingerprint({
    connectionId,
    dbSessionId,
    databaseType,
    database,
    schema,
    schemaContext,
  });

  return {
    ok: true,
    context: {
      sql,
      safeSql,
      safeErrorMessage,
      errorMessage: safeErrorMessage,
      databaseType,
      database,
      schema,
      schemaContext,
      connectionContext,
      promptContext,
      diagnosisParams: {
        dbSessionId,
        database,
        sql: safeSql,
        errorMessage: safeErrorMessage,
      },
      contextFingerprint,
    },
  };
}

type QueryDiagnosisContextLike =
  | QueryDiagnosisContext
  | QueryActionBuildResult<QueryDiagnosisContext>
  | null
  | undefined;

function resolveContext(value: QueryDiagnosisContextLike): {
  context: QueryDiagnosisContext | null;
  error: QueryActionError | null;
} {
  if (!value) {
    return {
      context: null,
      error: { code: 'empty-context', message: 'Query action context is empty.' },
    };
  }
  if ('ok' in value) {
    return value.ok ? { context: value.context, error: null } : { context: null, error: value.error };
  }
  return { context: value, error: null };
}

export interface ExplainActionDescriptor {
  type: 'explain';
  enabled: boolean;
  diagnosisParams: QueryDiagnosisParams | null;
  contextFingerprint: string | null;
  error: QueryActionError | null;
  /** Calls only the caller-owned diagnosis adapter; it never invokes IPC here. */
  invoke: (onExplain: (params: QueryDiagnosisParams) => void) => boolean;
}

/** Create an Explain descriptor that reuses the existing diagnosis command input. */
export function buildExplainAction(
  value: QueryDiagnosisContextLike,
): ExplainActionDescriptor {
  const { context, error } = resolveContext(value);
  return {
    type: 'explain',
    enabled: context !== null,
    diagnosisParams: context?.diagnosisParams ?? null,
    contextFingerprint: context?.contextFingerprint ?? null,
    error,
    invoke: (onExplain) => {
      if (!context) return false;
      onExplain({ ...context.diagnosisParams });
      return true;
    },
  };
}

export interface SqlDiff {
  changed: boolean;
  removedLines: string[];
  addedLines: string[];
  diffText: string;
}

export interface SqlEditorDraft {
  /** The failed SQL is retained for comparison and undo/revert UI. */
  originalSql: string;
  /** The suggested SQL to place in the editor. */
  sql: string;
  /** Explicit alias for adapters that call the field draftSql. */
  draftSql: string;
  diff: SqlDiff;
}

export interface FixSqlActionDescriptor {
  type: 'fixSql';
  enabled: boolean;
  draft: SqlEditorDraft | null;
  contextFingerprint: string | null;
  error: QueryActionError | null;
  /** Apply is an editor callback only; it cannot execute SQL. */
  applyToEditor: (onApply: (draft: SqlEditorDraft) => void) => SqlEditorDraft | null;
}

function buildSqlDiff(originalSql: string, suggestedSql: string): SqlDiff {
  if (originalSql === suggestedSql) {
    return { changed: false, removedLines: [], addedLines: [], diffText: '' };
  }
  const originalLines = originalSql.split(/\r?\n/);
  const suggestedLines = suggestedSql.split(/\r?\n/);
  let prefix = 0;
  while (
    prefix < originalLines.length &&
    prefix < suggestedLines.length &&
    originalLines[prefix] === suggestedLines[prefix]
  ) {
    prefix += 1;
  }
  let suffix = 0;
  while (
    suffix < originalLines.length - prefix &&
    suffix < suggestedLines.length - prefix &&
    originalLines[originalLines.length - suffix - 1] ===
      suggestedLines[suggestedLines.length - suffix - 1]
  ) {
    suffix += 1;
  }
  const removedLines = originalLines.slice(prefix, originalLines.length - suffix);
  const addedLines = suggestedLines.slice(prefix, suggestedLines.length - suffix);
  const diffText = [
    ...removedLines.map((line) => `- ${line}`),
    ...addedLines.map((line) => `+ ${line}`),
  ].join('\n');
  return { changed: true, removedLines, addedLines, diffText };
}

/** Create a Fix SQL descriptor. Building it never writes to the editor or executes SQL. */
export function buildFixSqlAction(
  value: QueryDiagnosisContextLike,
  suggestedSql: string | { suggestedSql?: unknown } | null | undefined,
): FixSqlActionDescriptor {
  const resolved = resolveContext(value);
  const suggestion =
    typeof suggestedSql === 'string'
      ? suggestedSql
      : nonEmptyString(asRecord(suggestedSql)?.suggestedSql);
  const error = resolved.error ??
    (!suggestion?.trim()
      ? { code: 'missing-suggested-sql' as const, message: 'No suggested SQL was returned.' }
      : null);
  const draft =
    resolved.context && suggestion?.trim()
      ? {
          originalSql: resolved.context.sql,
          sql: suggestion,
          draftSql: suggestion,
          diff: buildSqlDiff(resolved.context.sql, suggestion),
        }
      : null;

  return {
    type: 'fixSql',
    enabled: draft !== null,
    draft,
    contextFingerprint: resolved.context?.contextFingerprint ?? null,
    error,
    applyToEditor: (onApply) => {
      if (!draft) return null;
      const copy: SqlEditorDraft = {
        ...draft,
        diff: {
          ...draft.diff,
          removedLines: [...draft.diff.removedLines],
          addedLines: [...draft.diff.addedLines],
        },
      };
      onApply(copy);
      return copy;
    },
  };
}

export interface RetryRequest {
  sql: string;
  boundParams: BoundParams;
  contextFingerprint: string;
}

export interface RetryValidationInput {
  sql: string;
  contextFingerprint: string | null | undefined;
  boundParams?: BoundParams | null;
  /** Alias for callers using the QueryPanel terminology. */
  params?: BoundParams | null;
}

export interface RetryValidationSuccess {
  ok: true;
  request: RetryRequest;
}

export interface RetryValidationFailure {
  ok: false;
  error: QueryActionError;
}

export type RetryValidation = RetryValidationSuccess | RetryValidationFailure;

export interface RetryActionDescriptor {
  type: 'retry';
  enabled: boolean;
  request: RetryRequest | null;
  /** Alias for adapters that pass a request object named input. */
  input: RetryRequest | null;
  error: QueryActionError | null;
  validate: (current: RetryValidationInput) => RetryValidation;
  /** Only after validation does this pass the current SQL/params to a callback. */
  invoke: (
    current: RetryValidationInput,
    onRetry: (request: RetryRequest) => void,
  ) => RetryValidation;
}

function cloneBoundParams(value: BoundParams): BoundParams {
  return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, item]));
}

function equalBoundParams(left: BoundParams, right: BoundParams): boolean {
  return canonicalize(left) === canonicalize(right);
}

/** Create a guarded Retry descriptor; no database command is called by this module. */
export function buildRetryAction(
  value: QueryDiagnosisContextLike,
  boundParams: BoundParams | null = {},
): RetryActionDescriptor {
  const resolved = resolveContext(value);
  const safeBoundParams = boundParams ?? {};
  const request = resolved.context
    ? {
        sql: resolved.context.sql,
        boundParams: cloneBoundParams(safeBoundParams),
        contextFingerprint: resolved.context.contextFingerprint,
      }
    : null;
  const error = resolved.error;

  const validate = (current: RetryValidationInput): RetryValidation => {
    if (!request) {
      return {
        ok: false,
        error: error ?? { code: 'empty-context', message: 'Query action context is empty.' },
      };
    }
    if (current.contextFingerprint !== request.contextFingerprint) {
      return {
        ok: false,
        error: {
          code: 'context-changed',
          message: 'Database or schema context changed; review the query before retrying.',
        },
      };
    }
    if (current.sql !== request.sql) {
      return {
        ok: false,
        error: { code: 'sql-changed', message: 'SQL changed; retry uses the current SQL only.' },
      };
    }
    const currentParams = current.boundParams ?? current.params ?? {};
    if (!equalBoundParams(currentParams, request.boundParams)) {
      return {
        ok: false,
        error: { code: 'params-changed', message: 'Bound parameters changed; retry was cancelled.' },
      };
    }
    return { ok: true, request: { ...request, boundParams: cloneBoundParams(request.boundParams) } };
  };

  return {
    type: 'retry',
    enabled: request !== null,
    request,
    input: request,
    error,
    validate,
    invoke: (current, onRetry) => {
      const result = validate(current);
      if (result.ok) onRetry(result.request);
      return result;
    },
  };
}
