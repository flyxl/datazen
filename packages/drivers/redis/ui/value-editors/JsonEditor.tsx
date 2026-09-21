import { useCallback, useEffect, useRef, useState } from 'react';
import { ChevronDown, ChevronRight, Loader2, Plus, RefreshCw, Trash2 } from 'lucide-react';
import { Button, cn } from '@datazen/ui';
import { Input } from '@datazen/ui';
import { useI18n } from '@datazen/ui';
import { redisCommandInvoke } from '../shared/redisInvoke';
import { hasRedisJson } from './hasRedisJson';
import { JsonModeBar } from './JsonModeBar';
import {
  formatJson,
  isValidJson,
  JSON_DISPLAY_MODES,
  type JsonDisplayMode,
  type JsonTextMode,
} from './jsonModes';
import type { GateWriteFn } from '../shared/useRedisGate';

export interface JsonEditorProps {
  dbSessionId: string;
  dbIndex: number;
  redisKey: string;
  gateWrite?: GateWriteFn;
}

interface JsonGetResult {
  value: unknown | null;
  rawText?: string | null;
}

interface JsonDelResult {
  deleted: number;
}

type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };

function jsonType(value: JsonValue): string {
  if (value === null) return 'null';
  if (Array.isArray(value)) return 'array';
  return typeof value;
}

function childPath(parentPath: string, segment: string | number): string {
  if (parentPath === '$') {
    return typeof segment === 'number' ? `$[${segment}]` : `$.${segment}`;
  }
  return typeof segment === 'number' ? `${parentPath}[${segment}]` : `${parentPath}.${segment}`;
}

function formatScalar(value: JsonValue): string {
  if (value === null) return 'null';
  if (typeof value === 'string') return value;
  return String(value);
}

function parseScalarInput(raw: string, kind: string): JsonValue {
  const trimmed = raw.trim();
  if (kind === 'null') return null;
  if (kind === 'boolean') {
    if (trimmed === 'true') return true;
    if (trimmed === 'false') return false;
    throw new Error('Invalid boolean');
  }
  if (kind === 'number') {
    const n = Number(trimmed);
    if (Number.isNaN(n)) throw new Error('Invalid number');
    return n;
  }
  return raw;
}

export async function invokeJsonGet(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  path = '$',
  raw = false,
): Promise<JsonGetResult> {
  return redisCommandInvoke('redis', 'json_get', {
    dbSessionId,
    dbIndex,
    key,
    path,
    raw,
  }) as Promise<JsonGetResult>;
}

export async function invokeJsonSet(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  path: string,
  value: JsonValue,
): Promise<void> {
  await redisCommandInvoke('redis', 'json_set', {
    dbSessionId,
    dbIndex,
    key,
    path,
    value: JSON.stringify(value),
  });
}

export async function invokeJsonDel(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  path: string,
): Promise<JsonDelResult> {
  return redisCommandInvoke('redis', 'json_del', {
    dbSessionId,
    dbIndex,
    key,
    path,
  }) as Promise<JsonDelResult>;
}

export async function invokeModulesList(dbSessionId: string): Promise<string[]> {
  return redisCommandInvoke('redis', 'modules_list', { dbSessionId }) as Promise<string[]>;
}

function JsonTreeNode({
  dbSessionId,
  dbIndex,
  redisKey,
  path,
  name,
  value,
  depth,
  gateWrite,
  onChanged,
}: {
  dbSessionId: string;
  dbIndex: number;
  redisKey: string;
  path: string;
  name: string;
  value: JsonValue;
  depth: number;
  gateWrite?: GateWriteFn;
  onChanged: () => void;
}) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(depth < 2);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(formatScalar(value));
  const [busy, setBusy] = useState(false);
  const [addOpen, setAddOpen] = useState(false);
  const [addName, setAddName] = useState('');
  const [addValue, setAddValue] = useState('');
  const [error, setError] = useState<string | null>(null);

  const kind = jsonType(value);
  const isContainer = kind === 'object' || kind === 'array';

  const run = useCallback(
    async (fn: () => Promise<void>) => {
      setBusy(true);
      setError(null);
      try {
        await fn();
        onChanged();
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    },
    [onChanged],
  );

  const runWrite = useCallback(
    async (fn: () => Promise<void>) => {
      if (gateWrite && !(await gateWrite('write-op'))) return;
      await run(fn);
    },
    [gateWrite, run],
  );

  const saveScalar = () => {
    void runWrite(async () => {
      const parsed = parseScalarInput(draft, kind);
      await invokeJsonSet(dbSessionId, dbIndex, redisKey, path, parsed);
      setEditing(false);
    });
  };

  const deleteNode = () => {
    void runWrite(async () => {
      await invokeJsonDel(dbSessionId, dbIndex, redisKey, path);
    });
  };

  const addChild = () => {
    void runWrite(async () => {
      if (kind === 'object') {
        const field = addName.trim();
        if (!field) throw new Error(t('redis.jsonFieldRequired'));
        const child = childPath(path, field);
        let parsed: JsonValue = addValue;
        try {
          parsed = JSON.parse(addValue) as JsonValue;
        } catch {
          parsed = addValue;
        }
        await invokeJsonSet(dbSessionId, dbIndex, redisKey, child, parsed);
      } else {
        const child = `${path}[-]`;
        let parsed: JsonValue = addValue;
        try {
          parsed = JSON.parse(addValue) as JsonValue;
        } catch {
          parsed = addValue;
        }
        await invokeJsonSet(dbSessionId, dbIndex, redisKey, child, parsed);
      }
      setAddOpen(false);
      setAddName('');
      setAddValue('');
    });
  };

  const children: Array<{ label: string; path: string; value: JsonValue }> = [];
  if (kind === 'object' && value && typeof value === 'object' && !Array.isArray(value)) {
    for (const [k, v] of Object.entries(value)) {
      children.push({ label: k, path: childPath(path, k), value: v as JsonValue });
    }
  } else if (kind === 'array' && Array.isArray(value)) {
    value.forEach((v, i) => {
      children.push({ label: String(i), path: childPath(path, i), value: v as JsonValue });
    });
  }

  return (
    <div className="select-none">
      <div
        className={cn(
          'group flex flex-wrap items-center gap-1 rounded px-1 py-0.5 hover:bg-surface-alt/80',
          busy && 'opacity-60',
        )}
        style={{ paddingLeft: depth * 12 }}
      >
        {isContainer ? (
          <button
            type="button"
            className="flex h-5 w-5 items-center justify-center text-fg-muted"
            onClick={() => setExpanded((v) => !v)}
            aria-label={expanded ? t('redis.jsonCollapse') : t('redis.jsonExpand')}
          >
            {expanded ? (
              <ChevronDown className="h-3.5 w-3.5" />
            ) : (
              <ChevronRight className="h-3.5 w-3.5" />
            )}
          </button>
        ) : (
          <span className="inline-block h-5 w-5" />
        )}

        <span className="font-mono text-fg-secondary">{name}</span>
        <span className="rounded bg-surface px-1 text-[10px] uppercase text-fg-muted">{kind}</span>

        {!isContainer && !editing && (
          <span className="min-w-0 flex-1 truncate font-mono text-fg-muted">
            {formatScalar(value)}
          </span>
        )}

        {!isContainer && editing && (
          <Input
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            className="h-6 min-w-[120px] flex-1 font-mono text-xs"
          />
        )}

        <div className="ml-auto flex items-center gap-1 opacity-0 transition group-hover:opacity-100">
          {!isContainer && !editing && (
            <Button
              variant="ghost"
              className="h-6 px-1.5 text-[10px]"
              disabled={busy}
              onClick={() => {
                setDraft(formatScalar(value));
                setEditing(true);
              }}
            >
              {t('menu.edit')}
            </Button>
          )}
          {!isContainer && editing && (
            <Button
              variant="primary"
              className="h-6 px-1.5 text-[10px]"
              disabled={busy}
              onClick={saveScalar}
            >
              {t('common.save')}
            </Button>
          )}
          {isContainer && (
            <Button
              variant="ghost"
              className="h-6 px-1.5 text-[10px]"
              disabled={busy}
              onClick={() => setAddOpen((v) => !v)}
            >
              <Plus className="mr-0.5 h-3 w-3" />
              {t('redis.jsonAdd')}
            </Button>
          )}
          {path !== '$' && (
            <Button
              variant="ghost"
              className="h-6 px-1.5 text-[10px] text-danger"
              disabled={busy}
              onClick={deleteNode}
            >
              <Trash2 className="h-3 w-3" />
            </Button>
          )}
        </div>
      </div>

      {error && (
        <p className="text-[10px] text-danger" style={{ paddingLeft: depth * 12 + 24 }}>
          {error}
        </p>
      )}

      {addOpen && isContainer && (
        <div
          className="mb-1 mt-1 space-y-1 rounded border border-edge bg-surface-alt p-2"
          style={{ marginLeft: depth * 12 + 24 }}
        >
          {kind === 'object' && (
            <Input
              value={addName}
              onChange={(e) => setAddName(e.target.value)}
              placeholder={t('redis.jsonFieldName')}
              className="h-7 font-mono text-xs"
            />
          )}
          <Input
            value={addValue}
            onChange={(e) => setAddValue(e.target.value)}
            placeholder={t('redis.jsonValuePlaceholder')}
            className="h-7 font-mono text-xs"
          />
          <Button variant="primary" className="h-7 px-2 text-xs" disabled={busy} onClick={addChild}>
            {t('redis.jsonAddConfirm')}
          </Button>
        </div>
      )}

      {expanded &&
        children.map((child) => (
          <JsonTreeNode
            key={child.path}
            dbSessionId={dbSessionId}
            dbIndex={dbIndex}
            redisKey={redisKey}
            path={child.path}
            name={child.label}
            value={child.value}
            depth={depth + 1}
            gateWrite={gateWrite}
            onChanged={onChanged}
          />
        ))}
    </div>
  );
}

export function JsonEditor({ dbSessionId, dbIndex, redisKey, gateWrite }: JsonEditorProps) {
  const { t } = useI18n();
  const [modules, setModules] = useState<string[] | null>(null);
  const [root, setRoot] = useState<JsonValue | null>(null);
  const [rawText, setRawText] = useState<string>('');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [initBusy, setInitBusy] = useState(false);
  const [mode, setMode] = useState<JsonDisplayMode>('tree');
  const [text, setText] = useState<string>('');
  const [textError, setTextError] = useState<string | null>(null);
  const [savingText, setSavingText] = useState(false);
  const modeRef = useRef<JsonDisplayMode>('tree');
  useEffect(() => {
    modeRef.current = mode;
  }, [mode]);

  const capable = modules !== null && hasRedisJson(modules);

  const reload = useCallback(async () => {
    if (!capable) return;
    setLoading(true);
    setError(null);
    try {
      const result = await invokeJsonGet(dbSessionId, dbIndex, redisKey, '$', true);
      setRoot((result.value as JsonValue | null) ?? null);
      setRawText(result.rawText ?? (result.value == null ? '' : JSON.stringify(result.value)));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setRoot(null);
      setRawText('');
    } finally {
      setLoading(false);
    }
  }, [capable, dbSessionId, dbIndex, redisKey]);

  // Re-derive the editable buffer from freshly loaded raw text whenever the
  // server copy changes (load / save), but only while a text view is active.
  useEffect(() => {
    if (modeRef.current !== 'tree') setText(formatJson(rawText, modeRef.current as JsonTextMode));
  }, [rawText]);

  useEffect(() => {
    let cancelled = false;
    setModules(null);
    setRoot(null);
    setLoading(true);
    setError(null);

    void (async () => {
      try {
        const list = await invokeModulesList(dbSessionId);
        if (cancelled) return;
        setModules(list);
        if (!hasRedisJson(list)) {
          setLoading(false);
          return;
        }
        const result = await invokeJsonGet(dbSessionId, dbIndex, redisKey, '$', true);
        if (cancelled) return;
        setRoot((result.value as JsonValue | null) ?? null);
        setRawText(result.rawText ?? (result.value == null ? '' : JSON.stringify(result.value)));
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : String(e));
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [dbSessionId, dbIndex, redisKey]);

  const runWrite = useCallback(
    async (fn: () => Promise<void>) => {
      if (gateWrite && !(await gateWrite('write-op'))) return;
      await fn();
    },
    [gateWrite],
  );

  const initRoot = () => {
    void runWrite(async () => {
      setInitBusy(true);
      setError(null);
      await invokeJsonSet(dbSessionId, dbIndex, redisKey, '$', {})
        .then(() => reload())
        .catch((e) => setError(e instanceof Error ? e.message : String(e)))
        .finally(() => setInitBusy(false));
    });
  };

  const selectMode = (next: JsonDisplayMode) => {
    setTextError(null);
    if (next === 'tree') {
      setMode('tree');
      return;
    }
    // Entering text view from the tree seeds from the server copy; switching
    // between text views reformats the live buffer so edits survive (lossless).
    const base = mode === 'tree' || text === '' ? rawText : text;
    setText(formatJson(base, next));
    setMode(next);
  };

  const saveText = () => {
    if (!isValidJson(text)) {
      setTextError(t('redis.invalidJson'));
      return;
    }
    void runWrite(async () => {
      setSavingText(true);
      setTextError(null);
      try {
        await invokeJsonSet(dbSessionId, dbIndex, redisKey, '$', JSON.parse(text) as JsonValue);
        await reload();
      } catch (e) {
        setTextError(e instanceof Error ? e.message : String(e));
      } finally {
        setSavingText(false);
      }
    });
  };

  if (modules !== null && !capable) {
    return <p className="text-xs text-fg-muted">{t('redis.jsonModuleMissing')}</p>;
  }

  return (
    <div className="space-y-2 text-xs">
      <div className="flex items-center justify-between gap-2">
        <span className="font-medium text-fg-muted">{t('redis.jsonEditor')}</span>
        <Button
          variant="ghost"
          className="h-7 px-2 text-xs"
          disabled={loading || !capable}
          onClick={() => void reload()}
        >
          <RefreshCw className={cn('mr-1 h-3.5 w-3.5', loading && 'animate-spin')} />
          {t('redis.refresh')}
        </Button>
      </div>

      {!loading && root !== null && (
        <JsonModeBar modes={JSON_DISPLAY_MODES} active={mode} onSelect={selectMode} />
      )}

      {error && <p className="text-danger">{error}</p>}

      {loading ? (
        <div className="flex items-center gap-2 text-fg-muted">
          <Loader2 className="h-4 w-4 animate-spin" />
          {t('redis.monitorLoading')}
        </div>
      ) : root === null ? (
        <div className="space-y-2 rounded-md border border-dashed border-edge p-3">
          <p className="text-fg-muted">{t('redis.jsonEmpty')}</p>
          <Button
            variant="primary"
            className="h-7 px-2 text-xs"
            disabled={initBusy}
            onClick={initRoot}
          >
            {initBusy ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              t('redis.jsonInitObject')
            )}
          </Button>
        </div>
      ) : mode === 'tree' ? (
        <div className="max-h-[480px] overflow-auto rounded-md border border-edge bg-surface-alt p-2">
          <JsonTreeNode
            dbSessionId={dbSessionId}
            dbIndex={dbIndex}
            redisKey={redisKey}
            path="$"
            name={redisKey}
            value={root}
            depth={0}
            gateWrite={gateWrite}
            onChanged={() => void reload()}
          />
        </div>
      ) : (
        <div className="space-y-2">
          <textarea
            value={text}
            data-testid="redis-json-text"
            onChange={(e) => {
              setText(e.target.value);
              setTextError(null);
            }}
            spellCheck={false}
            className="min-h-[220px] w-full rounded-md border border-edge bg-surface-alt p-3 font-mono text-xs text-fg-secondary"
          />
          {textError && (
            <div className="rounded-md border border-danger/20 bg-danger/10 px-2 py-1.5 text-danger">
              {textError}
            </div>
          )}
          <div className="flex justify-end">
            <Button
              variant="primary"
              className="h-7 px-2 text-xs"
              disabled={savingText}
              data-testid="redis-json-text-save"
              onClick={saveText}
            >
              {t('redis.json.saveDocument')}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
