/**
 * Structured result renderer for Redis Console output.
 * Renders different result types (scalar, array, map, ok, nil, error) with
 * appropriate visual treatment.
 */

import React from 'react';

export type ResultType = 'scalar' | 'array' | 'map' | 'ok' | 'nil' | 'error';

export interface ConsoleResultItem {
  command: string;
  ok: boolean;
  value?: string;
  error?: string;
  resultType: ResultType;
  dangerLevel?: string;
}

/**
 * Render a single console result item with structured output.
 */
export function ConsoleResultView({ item }: { item: ConsoleResultItem }) {
  if (!item.ok && item.error) {
    return (
      <div className="rounded border border-red-500/40 bg-red-500/10 px-3 py-2 text-sm text-red-400 font-mono">
        {item.error}
      </div>
    );
  }

  if (item.resultType === 'nil') {
    return <span className="text-zinc-500 italic font-mono text-sm">(nil)</span>;
  }

  if (item.resultType === 'ok') {
    return (
      <span className="text-green-400 font-mono text-sm font-semibold">{item.value ?? 'OK'}</span>
    );
  }

  if (item.resultType === 'array') {
    return <ArrayResult value={item.value ?? '[]'} />;
  }

  if (item.resultType === 'map') {
    return <MapResult value={item.value ?? '{}'} />;
  }

  // scalar
  return <span className="text-zinc-200 font-mono text-sm">{item.value}</span>;
}

/**
 * Parse a formatted Redis array string like `[a, b, c]` into items.
 */
function parseArrayItems(value: string): string[] {
  const inner = value.startsWith('[') && value.endsWith(']') ? value.slice(1, -1) : value;
  if (!inner.trim()) return [];
  // Simple split — Redis format is predictable
  return inner.split(', ').map((s) => s.trim());
}

function ArrayResult({ value }: { value: string }) {
  const items = parseArrayItems(value);
  if (items.length === 0) {
    return <span className="text-zinc-500 font-mono text-sm">(empty array)</span>;
  }
  return (
    <div className="rounded border border-zinc-700 bg-zinc-900/50 overflow-hidden text-sm font-mono">
      <table className="w-full">
        <tbody>
          {items.map((item, i) => (
            <tr key={i} className="border-t border-zinc-800 first:border-t-0">
              <td className="px-3 py-1 text-zinc-500 w-10 text-right select-none">{i}</td>
              <td className="px-3 py-1 text-zinc-200">{item}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/**
 * Parse a formatted Redis map string like `{a => 1, b => 2}` into entries.
 */
function parseMapEntries(value: string): Array<{ key: string; val: string }> {
  const inner = value.startsWith('{') && value.endsWith('}') ? value.slice(1, -1) : value;
  if (!inner.trim()) return [];
  return inner.split(', ').map((pair) => {
    const [k, ...rest] = pair.split(' => ');
    return { key: k ?? pair, val: rest.join(' => ') };
  });
}

function MapResult({ value }: { value: string }) {
  const entries = parseMapEntries(value);
  if (entries.length === 0) {
    return <span className="text-zinc-500 font-mono text-sm">(empty map)</span>;
  }
  return (
    <div className="rounded border border-zinc-700 bg-zinc-900/50 overflow-hidden text-sm font-mono">
      <table className="w-full">
        <tbody>
          {entries.map((e, i) => (
            <tr key={i} className="border-t border-zinc-800 first:border-t-0">
              <td className="px-3 py-1 text-zinc-400 font-medium">{e.key}</td>
              <td className="px-3 py-1 text-zinc-200">{e.val}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/**
 * Classify a Redis value string as a result type hint.
 */
export function inferResultType(value: string | undefined): ResultType {
  if (value === undefined) return 'ok';
  if (value === '(nil)') return 'nil';
  if (value === 'OK') return 'ok';
  if (value.startsWith('[')) return 'array';
  if (value.startsWith('{')) return 'map';
  return 'scalar';
}
