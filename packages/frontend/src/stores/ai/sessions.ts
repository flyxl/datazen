import { aiCommands } from '../../commands/ai';
import type {
  AiChatMessage,
  AiChatSession,
  AiMessageStatus,
  AiSessionDetail,
  AiSessionMessage,
  AiSessionSummary,
  AiSessionUpsertInput,
} from '../../types';

/**
 * Session isolation for AI chat.
 *
 * Each unique (connectionId|dbSessionId, database) pair maps to a
 * `SessionKey` string. The backend AppDb is authoritative when available;
 * localStorage remains the offline/legacy fallback.
 */

export type SessionKey = string;

export interface SessionScope {
  connectionId?: string;
  dbSessionId?: string;
  database?: string;
}

export interface StoredSession {
  id?: string;
  title?: string;
  metadata?: Record<string, unknown>;
  connectionId?: string;
  database?: string;
  messages: AiChatMessage[];
  /** Timestamp for LRU ordering. */
  lastAccess: number;
}

export interface SessionRestoreResult {
  session: AiChatSession | null;
  summaries: AiSessionSummary[];
  /** A migration or backend failure that must stay visible to the user. */
  migrationError: string | null;
  /** Number of legacy conversations moved into AppDb on this load. */
  migrationSuccess: number;
  backendAvailable: boolean;
}

export interface SessionPersistResult {
  ok: boolean;
  error?: string;
  detail?: AiSessionDetail;
}

export interface SessionExport {
  content: string;
  filename: string;
  mime: string;
}

/**
 * Compute a deterministic session key from connection identity and database.
 * Priority: connectionId > dbSessionId > 'default'.
 */
export function computeSessionKey(
  connectionId?: string,
  dbSessionId?: string,
  database?: string,
): SessionKey {
  return `${connectionId || dbSessionId || 'default'}::${database || ''}`;
}

// ── localStorage persistence (offline fallback and legacy migration) ──

const STORAGE_KEY = 'datazen.aiSessions.v1';
const MIGRATION_MARKER_KEY = 'datazen.aiSessions.v1.migratedAt';
const MAX_SESSIONS = 20; // max distinct session keys
const MAX_MESSAGES_PER_SESSION = 50;
const MAX_MESSAGE_BYTES = 4096;

export function localSessionsMigrated(): boolean {
  try {
    return localStorage.getItem(MIGRATION_MARKER_KEY) !== null;
  } catch {
    return false;
  }
}

function markLocalSessionsMigrated(): void {
  try {
    localStorage.setItem(MIGRATION_MARKER_KEY, new Date().toISOString());
  } catch {
    // quota or security error — best effort
  }
}

/**
 * Load all persisted sessions from localStorage.
 * Returns a plain object keyed by SessionKey.
 */
export function loadSessions(): Record<SessionKey, StoredSession> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return parsed as Record<SessionKey, StoredSession>;
    }
    return {};
  } catch {
    // Corrupted data — clear and start fresh
    try {
      localStorage.removeItem(STORAGE_KEY);
    } catch {
      // quota or security error — best effort
    }
    return {};
  }
}

/**
 * Persist sessions to localStorage, evicting oldest LRU entries when
 * exceeding the session or message limits.
 */
export function saveSessions(sessions: Record<SessionKey, StoredSession>): void {
  try {
    // 1. Truncate each session's messages
    const trimmed: Record<SessionKey, StoredSession> = {};
    for (const [key, session] of Object.entries(sessions)) {
      const msgs = session.messages.slice(-MAX_MESSAGES_PER_SESSION);
      trimmed[key] = { ...session, messages: msgs };
    }

    // 2. LRU eviction: sort by lastAccess ascending, keep at most MAX_SESSIONS
    const keys = Object.keys(trimmed);
    if (keys.length > MAX_SESSIONS) {
      const sorted = keys.sort(
        (a, b) => (trimmed[a].lastAccess ?? 0) - (trimmed[b].lastAccess ?? 0),
      );
      const toRemove = sorted.slice(0, keys.length - MAX_SESSIONS);
      for (const k of toRemove) {
        delete trimmed[k];
      }
    }

    // 3. Write to localStorage, truncating individual large messages
    const toStore: Record<string, StoredSession> = {};
    for (const [key, session] of Object.entries(trimmed)) {
      toStore[key] = {
        ...session,
        messages: session.messages.map((m) => ({
          ...m,
          content: truncateBytes(m.content, MAX_MESSAGE_BYTES),
        })),
      };
    }

    localStorage.setItem(STORAGE_KEY, JSON.stringify(toStore));
  } catch {
    // Quota exceeded or security error — best effort
  }
}

/**
 * Touch a session to update its LRU timestamp, then persist.
 */
export function touchSession(
  sessions: Record<SessionKey, StoredSession>,
  key: SessionKey,
): Record<SessionKey, StoredSession> {
  const existing = sessions[key];
  return {
    ...sessions,
    [key]: {
      ...existing,
      lastAccess: Date.now(),
    },
  };
}

function truncateBytes(str: string, maxBytes: number): string {
  if (new TextEncoder().encode(str).length <= maxBytes) return str;
  // Simple approach: truncate characters until under limit
  let result = str;
  while (result.length > 0 && new TextEncoder().encode(result).length > maxBytes) {
    result = result.slice(0, -1);
  }
  return result;
}

// ── Mapping between AppDb sessions and the chat UI ──

function splitLegacyKey(key: SessionKey): { connectionId?: string; database?: string } {
  const [connection, database = ''] = key.split('::');
  return {
    connectionId: connection === 'default' ? undefined : connection,
    database: database || undefined,
  };
}

function messageStatus(message: AiChatMessage): AiMessageStatus {
  if (message.status) return message.status;
  if (message.cancelled) return 'cancelled';
  if (message.role === 'assistant' && message.content.startsWith('Error:')) return 'failed';
  return 'complete';
}

function messageMetadata(message: AiChatMessage): Record<string, unknown> | undefined {
  const metadata: Record<string, unknown> = {};
  if (message.reasoning !== undefined) metadata.reasoning = message.reasoning;
  if (message.questions !== undefined) metadata.questions = message.questions;
  if (message.toolCalls !== undefined) metadata.toolCalls = message.toolCalls;
  if (message.toolCallId !== undefined) metadata.toolCallId = message.toolCallId;
  if (message.cancelled !== undefined) metadata.cancelled = message.cancelled;
  if (message.memoryUsed !== undefined) metadata.memoryUsed = message.memoryUsed;
  return Object.keys(metadata).length > 0 ? metadata : undefined;
}

function titleFor(session: AiChatSession): string {
  if (session.title?.trim()) return session.title.trim();
  const firstUser = session.messages.find((message) => message.role === 'user');
  return firstUser?.content.trim().slice(0, 80) ?? '';
}

/** Convert the live chat session into the explicit AppDb replace payload. */
export function chatSessionToUpsertInput(session: AiChatSession): AiSessionUpsertInput {
  return {
    id: session.id,
    sessionKey: session.sessionKey,
    connectionId: session.connectionId ?? undefined,
    database: session.database ?? undefined,
    title: titleFor(session),
    metadata: session.metadata ?? undefined,
    lastAccessAt: new Date().toISOString(),
    append: false,
    messages: session.messages.map((message, index) => ({
      id: message.id ?? `${session.id}:m${index}`,
      role: message.role,
      content: message.content,
      status: messageStatus(message),
      createdAt: message.createdAt ?? new Date().toISOString(),
      metadata: messageMetadata(message),
    })),
  };
}

function messageFromStored(message: AiChatMessage): AiChatMessage {
  return {
    ...message,
    id: message.id ?? crypto.randomUUID(),
    status: messageStatus(message),
    createdAt: message.createdAt ?? new Date().toISOString(),
  };
}

/** Convert a backend detail into the live chat session shape. */
export function chatSessionFromDetail(detail: AiSessionDetail): AiChatSession {
  const metadata = (detail.metadata ?? null) as Record<string, unknown> | null;
  return {
    id: detail.id,
    sessionKey: detail.sessionKey,
    connectionId: detail.connectionId ?? null,
    database: detail.database ?? null,
    title: detail.title,
    metadata,
    messages: detail.messages.map((message) => {
      const extra = (message.metadata ?? {}) as Partial<AiChatMessage>;
      return {
        ...extra,
        id: message.id,
        role: message.role,
        content: message.content,
        status: message.status,
        createdAt: message.createdAt,
        cancelled: message.status === 'cancelled' || extra.cancelled === true,
      };
    }),
    isStreaming: false,
    streamContent: '',
    streamReasoning: '',
    streamMcpToolName: null,
    egressSummary: null,
    streamMemoryUsed: null,
    memoryUsed: null,
    requestId: null,
    attribution: null,
  };
}

function chatSessionFromStored(
  key: SessionKey,
  stored: StoredSession,
  scope: SessionScope,
): AiChatSession {
  return {
    id: stored.id ?? crypto.randomUUID(),
    sessionKey: key,
    connectionId: stored.connectionId ?? scope.connectionId ?? null,
    database: stored.database ?? scope.database ?? null,
    title: stored.title,
    metadata: stored.metadata ?? null,
    messages: stored.messages.map(messageFromStored),
    isStreaming: false,
    streamContent: '',
    streamReasoning: '',
    streamMcpToolName: null,
    egressSummary: null,
    streamMemoryUsed: null,
    memoryUsed: null,
    requestId: null,
    attribution: null,
  };
}

/** Synchronous localStorage view used to paint immediately while AppDb loads. */
export function localChatSessionForScope(
  key: SessionKey,
  scope: SessionScope,
): AiChatSession | null {
  const stored = loadSessions()[key];
  return stored ? chatSessionFromStored(key, stored, scope) : null;
}

/** New unsaved session for the current connection/database scope. */
export function newChatSession(scope: SessionScope, key?: SessionKey): AiChatSession {
  return {
    id: crypto.randomUUID(),
    sessionKey: key ?? computeSessionKey(scope.connectionId, scope.dbSessionId, scope.database),
    connectionId: scope.connectionId ?? null,
    database: scope.database ?? null,
    metadata: { startedAt: new Date().toISOString() },
    messages: [],
    isStreaming: false,
    streamContent: '',
    streamReasoning: '',
    streamMcpToolName: null,
    egressSummary: null,
    streamMemoryUsed: null,
    memoryUsed: null,
    requestId: null,
    attribution: null,
  };
}

// ── Backend-first session operations ──

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

async function listForScope(key: SessionKey, scope: SessionScope): Promise<AiSessionSummary[]> {
  return aiCommands.sessionsList({
    connectionId: scope.connectionId ?? '',
    database: scope.database ?? '',
    sessionKey: key,
  });
}

/**
 * Migrate every legacy localStorage session into AppDb once. Deterministic
 * ids make a partially failed/retried migration idempotent; the old key is
 * retained and only marked after every row succeeded (so a failed migration
 * stays readable in the fallback path).
 */
export async function migrateLocalSessionsToBackend(): Promise<{
  migrated: number;
  error: string | null;
}> {
  if (localSessionsMigrated()) return { migrated: 0, error: null };
  const sessions = loadSessions();
  const entries = Object.entries(sessions);
  if (entries.length === 0) return { migrated: 0, error: null };
  let migrated = 0;
  try {
    for (const [key, stored] of entries) {
      const scope = splitLegacyKey(key);
      const lastAccess = Number.isFinite(stored.lastAccess)
        ? new Date(stored.lastAccess).toISOString()
        : new Date().toISOString();
      await aiCommands.sessionsUpsert({
        id: `legacy:${key}`,
        sessionKey: key,
        connectionId: scope.connectionId,
        database: scope.database,
        title: stored.title,
        metadata: stored.metadata,
        lastAccessAt: lastAccess,
        append: false,
        messages: stored.messages.map((message, index) => ({
          id: message.id ?? `legacy:${key}:${index}`,
          role: message.role,
          content: message.content,
          status: messageStatus(message),
          createdAt: message.createdAt ?? lastAccess,
          metadata: messageMetadata(message),
        })),
      });
      migrated += 1;
    }
    markLocalSessionsMigrated();
    return { migrated, error: null };
  } catch (error) {
    return {
      migrated,
      error: `AI session migration failed; using local history: ${errorMessage(error)}`,
    };
  }
}

export async function restoreChatSession(
  key: SessionKey,
  scope: SessionScope,
): Promise<SessionRestoreResult> {
  let migrationError: string | null = null;
  let migrationSuccess = 0;
  try {
    const migration = await migrateLocalSessionsToBackend();
    migrationError = migration.error;
    if (!migration.error) migrationSuccess = migration.migrated;
  } catch (error) {
    migrationError = `AI session migration failed; using local history: ${errorMessage(error)}`;
  }

  try {
    const summaries = await listForScope(key, scope);
    if (summaries.length > 0) {
      const detail = await aiCommands.sessionsGet(summaries[0].id);
      return {
        session: chatSessionFromDetail(detail),
        summaries,
        migrationError,
        migrationSuccess,
        backendAvailable: true,
      };
    }
    // Backend is reachable but has no row yet: keep a local-only conversation
    // (for example one written while the backend was unavailable) instead of
    // discarding it. A successful delete removes the local mirror first.
    const local = loadSessions()[key];
    return {
      session: local ? chatSessionFromStored(key, local, scope) : newChatSession(scope, key),
      summaries: [],
      migrationError,
      migrationSuccess,
      backendAvailable: true,
    };
  } catch (error) {
    const local = loadSessions()[key];
    return {
      session: local ? chatSessionFromStored(key, local, scope) : newChatSession(scope, key),
      summaries: [],
      migrationError:
        migrationError ??
        `AI session backend unavailable; using local history: ${errorMessage(error)}`,
      migrationSuccess,
      backendAvailable: false,
    };
  }
}

export async function listChatSessions(
  scope: SessionScope,
  key?: SessionKey,
): Promise<AiSessionSummary[]> {
  return listForScope(
    key ?? computeSessionKey(scope.connectionId, scope.dbSessionId, scope.database),
    scope,
  );
}

export async function loadChatSession(id: string): Promise<AiSessionDetail> {
  return aiCommands.sessionsGet(id);
}

/**
 * Serialize AppDb writes so a slow user-message upsert cannot land after the
 * terminal assistant-message upsert and overwrite it with an older snapshot.
 */
let sessionWriteQueue: Promise<unknown> = Promise.resolve();

/**
 * Persist backend-first, then mirror into localStorage as the offline
 * fallback. A backend failure never discards the visible conversation.
 */
export function persistChatSession(session: AiChatSession): Promise<SessionPersistResult> {
  const write = sessionWriteQueue.then(
    () => persistChatSessionNow(session),
    () => persistChatSessionNow(session),
  );
  sessionWriteQueue = write.then(
    () => undefined,
    () => undefined,
  );
  return write;
}

async function persistChatSessionNow(session: AiChatSession): Promise<SessionPersistResult> {
  let backendError: string | undefined;
  let detail: AiSessionDetail | undefined;
  try {
    detail = await aiCommands.sessionsUpsert(chatSessionToUpsertInput(session));
  } catch (error) {
    backendError = errorMessage(error);
  }
  try {
    const sessions = loadSessions();
    sessions[session.sessionKey] = {
      id: session.id,
      title: session.title,
      metadata: session.metadata ?? undefined,
      connectionId: session.connectionId ?? undefined,
      database: session.database ?? undefined,
      messages: session.messages,
      lastAccess: Date.now(),
    };
    saveSessions(sessions);
  } catch {
    // localStorage is best-effort; the AppDb write above is authoritative.
  }
  return backendError === undefined
    ? { ok: true, detail }
    : { ok: false, error: `AI session was saved locally only: ${backendError}`, detail };
}

export async function deleteChatSession(
  id: string,
  key: SessionKey,
): Promise<{ ok: boolean; error?: string }> {
  let error: string | undefined;
  try {
    await aiCommands.sessionsDelete(id);
  } catch (cause) {
    error = errorMessage(cause);
  }
  try {
    const sessions = loadSessions();
    if (sessions[key]?.id === id || sessions[key]?.id === undefined) {
      delete sessions[key];
      saveSessions(sessions);
    }
  } catch {
    // best effort
  }
  return error === undefined ? { ok: true } : { ok: false, error };
}

function localDetail(session: AiChatSession): AiSessionDetail {
  return {
    id: session.id,
    sessionKey: session.sessionKey,
    connectionId: session.connectionId ?? null,
    database: session.database ?? null,
    title: titleFor(session),
    metadata: session.metadata ?? null,
    messageCount: session.messages.length,
    preview:
      session.messages.find((message) => message.role === 'user')?.content.slice(0, 120) ?? '',
    createdAt: session.messages[0]?.createdAt ?? new Date().toISOString(),
    updatedAt: new Date().toISOString(),
    lastAccessAt: new Date().toISOString(),
    messages: session.messages.map(
      (message, index): AiSessionMessage => ({
        id: message.id ?? `${session.id}:m${index}`,
        role: message.role,
        content: message.content,
        status: messageStatus(message),
        metadata: messageMetadata(message) ?? null,
        createdAt: message.createdAt ?? new Date().toISOString(),
      }),
    ),
  };
}

function markdownFor(detail: AiSessionDetail): string {
  const lines = [
    `# ${detail.title || 'AI conversation'}`,
    '',
    `- Session: \`${detail.sessionKey}\``,
  ];
  if (detail.connectionId) lines.push(`- Connection: \`${detail.connectionId}\``);
  if (detail.database) lines.push(`- Database: \`${detail.database}\``);
  lines.push(`- Created: ${detail.createdAt}`, `- Updated: ${detail.updatedAt}`, '');
  for (const message of detail.messages) {
    const role = message.role.charAt(0).toUpperCase() + message.role.slice(1);
    const suffix = message.status === 'complete' ? '' : ` (${message.status})`;
    lines.push(`## ${role}${suffix}`, '', message.content, '');
  }
  return lines.join('\n');
}

export async function exportChatSession(
  session: AiChatSession,
  format: 'json' | 'markdown',
): Promise<SessionExport> {
  let detail: AiSessionDetail;
  try {
    const content = await aiCommands.sessionsExport(session.id, format);
    detail = localDetail(session);
    return {
      content,
      filename: exportFilename(detail, format),
      mime: format === 'json' ? 'application/json' : 'text/markdown',
    };
  } catch {
    detail = localDetail(session);
    return {
      content: format === 'json' ? JSON.stringify(detail, null, 2) : markdownFor(detail),
      filename: exportFilename(detail, format),
      mime: format === 'json' ? 'application/json' : 'text/markdown',
    };
  }
}

function exportFilename(detail: AiSessionDetail, format: 'json' | 'markdown'): string {
  const base =
    (detail.title || detail.id)
      .trim()
      .replace(/[^\w\u4e00-\u9fff.-]+/g, '-')
      .replace(/^-+|-+$/g, '')
      .slice(0, 60) || 'ai-session';
  return `${base}.${format === 'json' ? 'json' : 'md'}`;
}

/** Trigger a browser download for an exported session (no-op outside DOM). */
export function downloadSessionExport(exported: SessionExport): void {
  if (typeof document === 'undefined' || typeof URL === 'undefined') return;
  const blob = new Blob([exported.content], { type: `${exported.mime};charset=utf-8` });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = exported.filename;
  anchor.style.display = 'none';
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  URL.revokeObjectURL(url);
}
