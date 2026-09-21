import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { REDIS_COMMAND_META, type CommandMeta } from './commandMeta';
import { invokeScanKeys } from '../redisInvoke';

export interface CommandCompletionItem {
  kind: 'command';
  insertText: string;
  label: string;
  meta: CommandMeta;
}

export interface KeyCompletionItem {
  kind: 'key';
  insertText: string;
  label: string;
}

export type CompletionItem = CommandCompletionItem | KeyCompletionItem;

export interface CompletionResult {
  open: boolean;
  items: CompletionItem[];
  /** Offset where the token being completed starts (replace range start). */
  tokenStart: number;
  /** Cursor offset (replace range end). */
  tokenEnd: number;
  loading: boolean;
}

const COMMAND_LIMIT = 50;
const KEY_LIMIT = 50;
const KEY_FETCH_COUNT = 200;
const DEBOUNCE_MS = 150;
const CACHE_TTL_MS = 3000;

interface KeyCacheEntry {
  items: string[];
  ts: number;
}

function currentToken(text: string, cursor: number): { token: string; start: number } {
  const lineStart = text.lastIndexOf('\n', Math.max(0, cursor - 1)) + 1;
  const linePrefix = text.slice(lineStart, cursor);
  const match = linePrefix.match(/(\S*)$/);
  const token = match?.[0] ?? '';
  return { token, start: cursor - token.length };
}

/**
 * Completion context state machine (R11/R12). Command mode at the first token,
 * key mode at the second; anything else closes the popup. Enter/exit is driven
 * only by text + cursor so there is no stuck state.
 */
export function useCompletion({
  text,
  cursor,
  dbSessionId,
  dbIndex,
  prewarmKeys = [],
  enabled = true,
}: {
  text: string;
  cursor: number;
  dbSessionId: string;
  dbIndex: number;
  prewarmKeys?: readonly string[];
  enabled?: boolean;
}): CompletionResult & { items: CompletionItem[] } {
  const { token, start: tokenStart } = useMemo(() => currentToken(text, cursor), [text, cursor]);

  // Determine mode from tokens before the current one on the same line.
  const mode = useMemo<'command' | 'key' | 'none'>(() => {
    if (!enabled) return 'none';
    const lineStart = text.lastIndexOf('\n', Math.max(0, cursor - 1)) + 1;
    const before = text.slice(lineStart, tokenStart).trimStart();
    if (before.length === 0) return 'command';
    const completedTokens = before.split(/\s+/).filter(Boolean);
    // Key mode only while filling the first argument of the command.
    if (completedTokens.length === 1) return 'key';
    return 'none';
  }, [text, cursor, tokenStart, enabled]);

  const commandItems = useMemo<CompletionItem[]>(() => {
    if (mode !== 'command') return [];
    const needle = token.toUpperCase();
    return REDIS_COMMAND_META.filter((m) => m.name.startsWith(needle))
      .slice(0, COMMAND_LIMIT)
      .map<CompletionItem>((meta) => ({
        kind: 'command',
        insertText: meta.name,
        label: meta.name,
        meta,
      }));
  }, [mode, token]);

  const [keyItems, setKeyItems] = useState<string[]>([]);
  const [keyLoading, setKeyLoading] = useState(false);
  const cacheRef = useRef<Map<string, KeyCacheEntry>>(new Map());
  const reqSeqRef = useRef(0);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Clear the cache when the target db/session changes.
  useEffect(() => {
    cacheRef.current.clear();
  }, [dbSessionId, dbIndex]);

  useEffect(() => {
    if (mode !== 'key') {
      setKeyItems([]);
      setKeyLoading(false);
      return;
    }
    const prefix = token;
    const cache = cacheRef.current;
    const hit = cache.get(prefix);
    if (hit && Date.now() - hit.ts < CACHE_TTL_MS) {
      setKeyItems(hit.items);
      setKeyLoading(false);
      return;
    }
    setKeyLoading(true);
    if (debounceRef.current) clearTimeout(debounceRef.current);
    const seq = ++reqSeqRef.current;
    debounceRef.current = setTimeout(() => {
      void (async () => {
        try {
          const res = await invokeScanKeys(dbSessionId, dbIndex, `${prefix}*`, 0, KEY_FETCH_COUNT);
          if (seq !== reqSeqRef.current) return; // stale response
          const names = res.keys.map((entry) => entry.key).slice(0, KEY_LIMIT);
          cache.set(prefix, { items: names, ts: Date.now() });
          setKeyItems(names);
        } catch {
          if (seq === reqSeqRef.current) setKeyItems([]);
        } finally {
          if (seq === reqSeqRef.current) setKeyLoading(false);
        }
      })();
    }, DEBOUNCE_MS);
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [mode, token, dbSessionId, dbIndex]);

  const items = useMemo<CompletionItem[]>(() => {
    if (mode === 'command') return commandItems;
    if (mode === 'key') {
      const seen = new Set<string>();
      const merged: CompletionItem[] = [];
      for (const key of [...keyItems, ...prewarmKeys]) {
        if (seen.has(key)) continue;
        seen.add(key);
        merged.push({ kind: 'key', insertText: key, label: key });
        if (merged.length >= KEY_LIMIT) break;
      }
      return merged;
    }
    return [];
  }, [mode, commandItems, keyItems, prewarmKeys]);

  const open = mode !== 'none' && token.length > 0 && items.length > 0;

  const reset = useCallback(() => {
    setKeyItems([]);
    setKeyLoading(false);
  }, []);
  useEffect(() => {
    if (mode !== 'key') reset();
  }, [mode, reset]);

  return {
    open,
    items,
    tokenStart,
    tokenEnd: cursor,
    loading: mode === 'key' && keyLoading,
  };
}
