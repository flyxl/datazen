/**
 * 屏 A data sourcing (PRD §3.0 "状态内行为" / §3.1 关键前提).
 *
 * Issues **exactly four** Redis Driver Commands and nothing else:
 *
 *   info          → 横幅 pill + 卡 1 Server 概览 + 卡 2 内存量表
 *   db_sizes      → 卡 3 Key Space 网格
 *   memory_sample → 卡 2 Top5 大 key（可选源，权限位 `redis:allow-memory-sample`）
 *   slowlog_get   → 卡 4 慢查询 Top5（可选源，权限位 `redis:allow-slowlog-get`）
 *
 * Every request goes through `execute_driver_command` via `redisCommandInvoke`;
 * the host never learns this is Redis. The two optional sources fail **independently**
 * so a locked-down server still renders a complete landing screen (I-11), and the
 * whole screen stays free of `scan_keys` / `list_children` / `scan_values` — that
 * is the invariant that makes 屏 A safe as the default landing screen.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  redisCommandInvoke,
  invokeDbSizes,
  type DbSize,
  type RedisInvokeFn,
} from '../shared/redisInvoke';
import { parseInfoSections, type InfoSection } from '../observe/infoParse';
import {
  classifyOverviewError,
  flattenInfoFields,
  BIG_KEY_LIMIT,
  SLOWLOG_LIMIT,
  type OverviewMemorySampleResult,
  type OverviewSlowlogEntry,
} from './overviewModel';

/** Command names this hook is allowed to issue — also the invariant's whitelist. */
export const OVERVIEW_COMMANDS = {
  info: 'info',
  dbSizes: 'db_sizes',
  memorySample: 'memory_sample',
  slowlogGet: 'slowlog_get',
} as const;

export type OverviewSourceStatus = 'loading' | 'ready' | 'unauthorized' | 'failed';

export interface OverviewSource<T> {
  status: OverviewSourceStatus;
  data: T | null;
  /** Raw transport message; only kept for `failed` (never rendered for `unauthorized`). */
  message: string | null;
}

/** Parsed INFO reply: raw text plus the flat field table every card derives from. */
export interface OverviewInfoPayload {
  raw: string;
  sections: InfoSection[];
  fields: Record<string, string>;
}

export interface OverviewData {
  info: OverviewSource<OverviewInfoPayload>;
  dbSizes: OverviewSource<DbSize[]>;
  memory: OverviewSource<OverviewMemorySampleResult>;
  slowlog: OverviewSource<OverviewSlowlogEntry[]>;
  /** Re-issue all four commands (the header's refresh affordance). */
  refresh: () => void;
  /** Re-issue only the memory sample command. */
  refreshMemory: () => void;
}

export interface UseOverviewDataArgs {
  dbSessionId: string;
  /** Logical database `memory_sample` samples — 屏 A has no panel, so this is the connection's configured db. */
  dbIndex: number;
  /** Test seam; defaults to the real `execute_driver_command` gateway. */
  invoke?: RedisInvokeFn;
}

function loading<T>(): OverviewSource<T> {
  return { status: 'loading', data: null, message: null };
}

function failed<T>(error: unknown): OverviewSource<T> {
  const message = error instanceof Error ? error.message : String(error ?? '');
  const status = classifyOverviewError(error) === 'unauthorized' ? 'unauthorized' : 'failed';
  return { status, data: null, message: status === 'failed' ? message : null };
}

export function useOverviewData({
  dbSessionId,
  dbIndex,
  invoke = redisCommandInvoke,
}: UseOverviewDataArgs): OverviewData {
  const [info, setInfo] = useState<OverviewSource<OverviewInfoPayload>>(() => loading());
  const [dbSizes, setDbSizes] = useState<OverviewSource<DbSize[]>>(() => loading());
  const [memory, setMemory] = useState<OverviewSource<OverviewMemorySampleResult>>(() => loading());
  const [slowlog, setSlowlog] = useState<OverviewSource<OverviewSlowlogEntry[]>>(() => loading());
  // Session switches must not let an in-flight reply land on the new session.
  const tokenRef = useRef(0);

  const load = useCallback(() => {
    if (!dbSessionId) {
      setInfo(failed(new Error('missing dbSessionId')));
      setDbSizes(failed(new Error('missing dbSessionId')));
      setMemory(failed(new Error('missing dbSessionId')));
      setSlowlog(failed(new Error('missing dbSessionId')));
      return;
    }
    const token = tokenRef.current + 1;
    tokenRef.current = token;
    const stale = () => tokenRef.current !== token;

    setInfo(loading());
    setDbSizes(loading());
    setMemory(loading());
    setSlowlog(loading());

    // 1) INFO — everything card 1 / card 2 / the banner derives from this one reply.
    void (async () => {
      try {
        const raw = (await invoke('redis', OVERVIEW_COMMANDS.info, {
          dbSessionId,
          section: null,
        })) as string | null | undefined;
        if (stale()) return;
        const text = typeof raw === 'string' ? raw : String(raw ?? '');
        const sections = parseInfoSections(text);
        setInfo({
          status: 'ready',
          data: { raw: text, sections, fields: flattenInfoFields(sections) },
          message: null,
        });
      } catch (error) {
        if (stale()) return;
        setInfo(failed(error));
      }
    })();

    // 2) db_sizes — the only key-space source (INFO keyspace would be a second round trip for nothing).
    void (async () => {
      try {
        const sizes = await invokeDbSizes(dbSessionId, invoke);
        if (stale()) return;
        setDbSizes({ status: 'ready', data: Array.isArray(sizes) ? sizes : [], message: null });
      } catch (error) {
        if (stale()) return;
        setDbSizes(failed(error));
      }
    })();

    // 3) memory_sample — optional; a NOPERM reply becomes the named 未授权 state.
    void (async () => {
      try {
        const result = (await invoke('redis', OVERVIEW_COMMANDS.memorySample, {
          dbSessionId,
          dbIndex,
          // 屏 A 只要 Top5，把后端采样窗口一起收窄（它内部按 limit 早停）。
          limit: BIG_KEY_LIMIT,
        })) as OverviewMemorySampleResult | null | undefined;
        if (stale()) return;
        setMemory({
          status: 'ready',
          data: {
            samples: Array.isArray(result?.samples) ? result.samples : [],
            truncated: result?.truncated === true,
          },
          message: null,
        });
      } catch (error) {
        if (stale()) return;
        setMemory(failed(error));
      }
    })();

    // 4) slowlog_get — optional, same degradation contract.
    void (async () => {
      try {
        const entries = (await invoke('redis', OVERVIEW_COMMANDS.slowlogGet, {
          dbSessionId,
          count: SLOWLOG_LIMIT,
        })) as OverviewSlowlogEntry[] | null | undefined;
        if (stale()) return;
        setSlowlog({ status: 'ready', data: Array.isArray(entries) ? entries : [], message: null });
      } catch (error) {
        if (stale()) return;
        setSlowlog(failed(error));
      }
    })();
  }, [dbSessionId, dbIndex, invoke]);

  /** Refresh only the memory sample data (used by the MEMORY card's Refresh button). */
  const refreshMemory = useCallback(() => {
    if (!dbSessionId) {
      setMemory(failed(new Error('missing dbSessionId')));
      return;
    }
    const token = tokenRef.current + 1;
    tokenRef.current = token;
    const stale = () => tokenRef.current !== token;

    setMemory(loading());
    void (async () => {
      try {
        const result = (await invoke('redis', OVERVIEW_COMMANDS.memorySample, {
          dbSessionId,
          dbIndex,
          limit: BIG_KEY_LIMIT,
        })) as OverviewMemorySampleResult | null | undefined;
        if (stale()) return;
        setMemory({
          status: 'ready',
          data: {
            samples: Array.isArray(result?.samples) ? result.samples : [],
            truncated: result?.truncated === true,
          },
          message: null,
        });
      } catch (error) {
        if (stale()) return;
        setMemory(failed(error));
      }
    })();
  }, [dbSessionId, dbIndex, invoke]);

  useEffect(() => {
    load();
    return () => {
      // Invalidate anything still in flight when the screen unmounts.
      tokenRef.current += 1;
    };
  }, [load]);

  return useMemo(
    () => ({ info, dbSizes, memory, slowlog, refresh: load, refreshMemory }),
    [info, dbSizes, memory, slowlog, load, refreshMemory],
  );
}
