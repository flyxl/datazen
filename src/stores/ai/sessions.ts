/**
 * Session isolation for AI chat.
 *
 * Each unique (connectionId|dbSessionId, database) pair maps to a
 * `SessionKey` string. Chat history is persisted to localStorage with
 * LRU eviction to stay within quota limits.
 */

export type SessionKey = string;

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

// ── localStorage persistence ──

const STORAGE_KEY = 'datazen.aiSessions.v1';
const MAX_SESSIONS = 20; // max distinct session keys
const MAX_MESSAGES_PER_SESSION = 50;
const MAX_MESSAGE_BYTES = 4096;

interface StoredMessage {
  role: 'user' | 'assistant' | 'tool' | 'system';
  content: string;
  reasoning?: string;
  toolCalls?: { id: string; name: string; arguments?: string }[];
  toolCallId?: string;
}

interface StoredSession {
  messages: StoredMessage[];
  /** Timestamp for LRU ordering */
  lastAccess: number;
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
