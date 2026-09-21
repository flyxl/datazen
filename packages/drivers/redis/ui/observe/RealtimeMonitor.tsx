/**
 * Real-time MONITOR panel — captures Redis commands on a dedicated connection
 * and streams them to the UI with a circular buffer and auto-scroll.
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { useI18n } from '@datazen/ui';
import {
  invokeMonitorStart,
  invokeMonitorStop,
  invokeMonitorGetBuffer,
  MonitorEvent,
} from './monitorInvoke';

interface Props {
  dbSessionId: string;
}

const BUFFER_OPTIONS = [100, 500, 1000, 5000];

export function RealtimeMonitor({ dbSessionId }: Props) {
  const { t } = useI18n();
  const [monitorId, setMonitorId] = useState<string | null>(null);
  const [events, setEvents] = useState<MonitorEvent[]>([]);
  const [bufferSize, setBufferSize] = useState(1000);
  const [paused, setPaused] = useState(false);
  const [autoScroll, setAutoScroll] = useState(true);
  const listRef = useRef<HTMLDivElement>(null);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Poll for new events
  const poll = useCallback(async () => {
    if (!monitorId || paused) return;
    try {
      const newEvents = await invokeMonitorGetBuffer(dbSessionId, monitorId);
      setEvents(newEvents);
    } catch {
      // Connection may have dropped — stop monitoring
      setMonitorId(null);
    }
  }, [monitorId, paused, dbSessionId]);

  // Start polling
  useEffect(() => {
    if (monitorId && !paused) {
      intervalRef.current = setInterval(poll, 500);
    }
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, [monitorId, paused, poll]);

  // Auto-scroll to bottom
  useEffect(() => {
    if (autoScroll && listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
  }, [events, autoScroll]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (monitorId) {
        invokeMonitorStop(dbSessionId, monitorId).catch(() => {});
      }
    };
  }, [monitorId, dbSessionId]);

  const handleStart = async () => {
    try {
      const id = await invokeMonitorStart(dbSessionId, bufferSize);
      setMonitorId(id);
      setEvents([]);
      setPaused(false);
    } catch (err) {
      console.error('Failed to start MONITOR:', err);
    }
  };

  const handleStop = async () => {
    if (monitorId) {
      await invokeMonitorStop(dbSessionId, monitorId).catch(() => {});
      setMonitorId(null);
    }
  };

  const formatTimestamp = (ts: number) => {
    const d = new Date(ts * 1000);
    return d.toLocaleTimeString('en-US', {
      hour12: false,
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      fractionalSecondDigits: 3,
    } as Intl.DateTimeFormatOptions);
  };

  return (
    <div className="flex flex-col h-full gap-2 p-2">
      {/* Controls */}
      <div className="flex items-center gap-3 flex-wrap">
        {!monitorId ? (
          <button
            onClick={handleStart}
            className="px-3 py-1.5 bg-green-600 hover:bg-green-500 text-white rounded text-sm font-medium"
          >
            ▶ {t('redis.monitorStart')}
          </button>
        ) : (
          <>
            <button
              onClick={handleStop}
              className="px-3 py-1.5 bg-red-600 hover:bg-red-500 text-white rounded text-sm font-medium"
            >
              ■ {t('redis.monitorStop')}
            </button>
            <button
              onClick={() => setPaused(!paused)}
              className="px-3 py-1.5 bg-zinc-600 hover:bg-zinc-500 text-white rounded text-sm"
            >
              {paused ? '▶ ' + t('redis.monitorResume') : '⏸ ' + t('redis.monitorPause')}
            </button>
          </>
        )}

        <select
          value={bufferSize}
          onChange={(e) => setBufferSize(Number(e.target.value))}
          disabled={!!monitorId}
          className="bg-zinc-800 border border-zinc-600 rounded px-2 py-1 text-sm text-zinc-200"
        >
          {BUFFER_OPTIONS.map((n) => (
            <option key={n} value={n}>
              {n}
            </option>
          ))}
        </select>

        <label className="flex items-center gap-1 text-sm text-zinc-400">
          <input
            type="checkbox"
            checked={autoScroll}
            onChange={(e) => setAutoScroll(e.target.checked)}
            className="accent-green-500"
          />
          Auto-scroll
        </label>

        <span className="text-sm text-zinc-500 ml-auto">
          {events.length} {t('redis.monitorEventCount')}
        </span>
      </div>

      {/* Status */}
      {monitorId && (
        <div className="flex items-center gap-2 text-xs text-green-400">
          <span className="inline-block w-2 h-2 rounded-full bg-green-400 animate-pulse" />
          {t('redis.monitorRunning')}
          {paused && <span className="text-yellow-400"> (paused)</span>}
        </div>
      )}

      {/* Event list */}
      <div
        ref={listRef}
        className="flex-1 overflow-y-auto bg-zinc-900 rounded border border-zinc-700 font-mono text-xs"
      >
        {events.length === 0 ? (
          <div className="p-4 text-zinc-500 text-center">
            {monitorId ? t('redis.monitorWaiting') : t('redis.monitorEmpty')}
          </div>
        ) : (
          <table className="w-full">
            <thead className="sticky top-0 bg-zinc-800">
              <tr className="text-zinc-400 border-b border-zinc-700">
                <th className="px-2 py-1 text-left font-medium">Time</th>
                <th className="px-2 py-1 text-left font-medium">DB</th>
                <th className="px-2 py-1 text-left font-medium">Client</th>
                <th className="px-2 py-1 text-left font-medium">Command</th>
              </tr>
            </thead>
            <tbody>
              {events.map((ev, i) => (
                <tr
                  key={`${ev.timestamp}-${i}`}
                  className="border-b border-zinc-800 hover:bg-zinc-800/50"
                >
                  <td className="px-2 py-0.5 text-zinc-500 whitespace-nowrap">
                    {formatTimestamp(ev.timestamp)}
                  </td>
                  <td className="px-2 py-0.5 text-zinc-400">{ev.db}</td>
                  <td className="px-2 py-0.5 text-zinc-400">{ev.clientAddr}</td>
                  <td className="px-2 py-0.5 text-zinc-200">
                    <span className="text-blue-400 font-semibold">{ev.command}</span>
                    {ev.args.length > 0 && (
                      <span className="text-zinc-300"> {ev.args.join(' ')}</span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
