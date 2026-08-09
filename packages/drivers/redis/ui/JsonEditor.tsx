import {
  useCallback,
  useEffect,
  useState,
} from 'react';
import {
  ChevronDown,
  ChevronRight,
  Loader2,
  Plus,
  RefreshCw,
  Trash2,
} from 'lucide-react';
import { Button } from '../../../../src/components/ui/Button';
import { Input } from '../../../../src/components/ui/Input';
import { useI18n } from '../../../../src/hooks/useI18n';
import { cn } from '../../../../src/lib/cn';
import { pluginInvoke } from '../../../../src/plugins/generated';
import { hasRedisJson } from './hasRedisJson';

export interface JsonEditorProps {
  connectionId: string;
  dbIndex: number;
  redisKey: string;
}

interface JsonGetResult {
  value: unknown | null;
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
  return typeof segment === 'number'
    ? `${parentPath}[${segment}]`
    : `${parentPath}.${segment}`;
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
  connectionId: string,
  dbIndex: number,
  key: string,
  path = '$',
): Promise<JsonGetResult> {
  return pluginInvoke('redis', 'json_get', {
    connectionId,
    dbIndex,
    key,
    path,
  }) as Promise<JsonGetResult>;
}

export async function invokeJsonSet(
  connectionId: string,
  dbIndex: number,
  key: string,
  path: string,
  value: JsonValue,
): Promise<void> {
  await pluginInvoke('redis', 'json_set', {
    connectionId,
    dbIndex,
    key,
    path,
    value: JSON.stringify(value),
  });
}

export async function invokeJsonDel(
  connectionId: string,
  dbIndex: number,
  key: string,
  path: string,
): Promise<JsonDelResult> {
  return pluginInvoke('redis', 'json_del', {
    connectionId,
    dbIndex,
    key,
    path,
  }) as Promise<JsonDelResult>;
}

export async function invokeModulesList(connectionId: string): Promise<string[]> {
  return pluginInvoke('redis', 'modules_list', { connectionId }) as Promise<string[]>;
}

function JsonTreeNode({
  connectionId,
  dbIndex,
  redisKey,
  path,
  name,
  value,
  depth,
  onChanged,
}: {
  connectionId: string;
  dbIndex: number;
  redisKey: string;
  path: string;
  name: string;
  value: JsonValue;
  depth: number;
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

  const saveScalar = () => {
    void run(async () => {
      const parsed = parseScalarInput(draft, kind);
      await invokeJsonSet(connectionId, dbIndex, redisKey, path, parsed);
      setEditing(false);
    });
  };

  const deleteNode = () => {
    void run(async () => {
      await invokeJsonDel(connectionId, dbIndex, redisKey, path);
    });
  };

  const addChild = () => {
    void run(async () => {
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
        await invokeJsonSet(connectionId, dbIndex, redisKey, child, parsed);
      } else {
        const child = `${path}[-]`;
        let parsed: JsonValue = addValue;
        try {
          parsed = JSON.parse(addValue) as JsonValue;
        } catch {
          parsed = addValue;
        }
        await invokeJsonSet(connectionId, dbIndex, redisKey, child, parsed);
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
            {expanded ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
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
              {t('common.edit')}
            </Button>
          )}
          {!isContainer && editing && (
            <Button variant="primary" className="h-6 px-1.5 text-[10px]" disabled={busy} onClick={saveScalar}>
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
            connectionId={connectionId}
            dbIndex={dbIndex}
            redisKey={redisKey}
            path={child.path}
            name={child.label}
            value={child.value}
            depth={depth + 1}
            onChanged={onChanged}
          />
        ))}
    </div>
  );
}

export function JsonEditor({ connectionId, dbIndex, redisKey }: JsonEditorProps) {
  const { t } = useI18n();
  const [modules, setModules] = useState<string[] | null>(null);
  const [root, setRoot] = useState<JsonValue | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [initBusy, setInitBusy] = useState(false);

  const capable = modules !== null && hasRedisJson(modules);

  const reload = useCallback(async () => {
    if (!capable) return;
    setLoading(true);
    setError(null);
    try {
      const result = await invokeJsonGet(connectionId, dbIndex, redisKey, '$');
      setRoot((result.value as JsonValue | null) ?? null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setRoot(null);
    } finally {
      setLoading(false);
    }
  }, [capable, connectionId, dbIndex, redisKey]);

  useEffect(() => {
    let cancelled = false;
    setModules(null);
    setRoot(null);
    setLoading(true);
    setError(null);

    void (async () => {
      try {
        const list = await invokeModulesList(connectionId);
        if (cancelled) return;
        setModules(list);
        if (!hasRedisJson(list)) {
          setLoading(false);
          return;
        }
        const result = await invokeJsonGet(connectionId, dbIndex, redisKey, '$');
        if (cancelled) return;
        setRoot((result.value as JsonValue | null) ?? null);
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
  }, [connectionId, dbIndex, redisKey]);

  const initRoot = () => {
    setInitBusy(true);
    setError(null);
    void invokeJsonSet(connectionId, dbIndex, redisKey, '$', {})
      .then(() => reload())
      .catch((e) => setError(e instanceof Error ? e.message : String(e)))
      .finally(() => setInitBusy(false));
  };

  if (modules !== null && !capable) {
    return (
      <p className="text-xs text-fg-muted">{t('redis.jsonModuleMissing')}</p>
    );
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

      {error && <p className="text-danger">{error}</p>}

      {loading ? (
        <div className="flex items-center gap-2 text-fg-muted">
          <Loader2 className="h-4 w-4 animate-spin" />
          {t('redis.monitorLoading')}
        </div>
      ) : root === null ? (
        <div className="space-y-2 rounded-md border border-dashed border-edge p-3">
          <p className="text-fg-muted">{t('redis.jsonEmpty')}</p>
          <Button variant="primary" className="h-7 px-2 text-xs" disabled={initBusy} onClick={initRoot}>
            {initBusy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : t('redis.jsonInitObject')}
          </Button>
        </div>
      ) : (
        <div className="max-h-[480px] overflow-auto rounded-md border border-edge bg-surface-alt p-2">
          <JsonTreeNode
            connectionId={connectionId}
            dbIndex={dbIndex}
            redisKey={redisKey}
            path="$"
            name={redisKey}
            value={root}
            depth={0}
            onChanged={() => void reload()}
          />
        </div>
      )}
    </div>
  );
}
