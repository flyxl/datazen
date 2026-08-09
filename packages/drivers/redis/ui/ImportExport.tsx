import { useCallback, useEffect, useState } from 'react';
import { Download, Loader2, Upload } from 'lucide-react';
import { Button } from '../../../../src/components/ui/Button';
import { Dialog } from '../../../../src/components/ui/Dialog';
import { Input } from '../../../../src/components/ui/Input';
import { fileCommands } from '../../../../src/commands/file';
import { databaseCommands } from '../../../../src/commands/database';
import { useI18n } from '../../../../src/hooks/useI18n';
import { pluginInvoke } from '../../../../src/plugins/generated';
import { invokeCountMatching } from './BatchBar';
import {
  base64ToZip,
  buildJsonExport,
  packDumpZip,
  parseDumpZip,
  zipToBase64,
  type DumpKeyEntry,
  type RestoreKeyEntry,
} from './importExportZip';

export interface DumpKeysResult {
  entries: DumpKeyEntry[];
  errors: { key: string; error: string }[];
}

export interface RestoreKeysResult {
  restored: number;
  errors: { key: string; error: string }[];
}

export interface ImportExportProps {
  connectionId: string;
  dbIndex: number;
  selectedKeys: string[];
  searchPattern: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onRefresh: () => void | Promise<void>;
  onSummary?: (message: string) => void;
}

type ExportMode = 'selected' | 'pattern';

async function scanKeysForPattern(
  connectionId: string,
  dbIndex: number,
  pattern: string,
): Promise<string[]> {
  const keys: string[] = [];
  let cursor = 0;
  do {
    const page = await databaseCommands.kvScanKeys(connectionId, dbIndex, pattern, cursor, 500);
    keys.push(...page.keys.map((entry) => entry.key));
    cursor = page.nextCursor;
  } while (cursor !== 0);
  return keys;
}

async function invokeDumpKeys(
  connectionId: string,
  dbIndex: number,
  keys: string[],
): Promise<DumpKeysResult> {
  return (await pluginInvoke('redis', 'dump_keys', {
    connectionId,
    dbIndex,
    keys,
  })) as DumpKeysResult;
}

async function invokeRestoreKeys(
  connectionId: string,
  dbIndex: number,
  entries: RestoreKeyEntry[],
  replace: boolean,
): Promise<RestoreKeysResult> {
  return (await pluginInvoke('redis', 'restore_keys', {
    connectionId,
    dbIndex,
    entries,
    replace,
  })) as RestoreKeysResult;
}

export function ImportExport({
  connectionId,
  dbIndex,
  selectedKeys,
  searchPattern,
  open,
  onOpenChange,
  onRefresh,
  onSummary,
}: ImportExportProps) {
  const { t } = useI18n();
  const [exportMode, setExportMode] = useState<ExportMode>('selected');
  const [patternInput, setPatternInput] = useState(searchPattern);
  const [matchCount, setMatchCount] = useState<number | null>(null);
  const [replaceExisting, setReplaceExisting] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setPatternInput(searchPattern);
      setError(null);
    }
  }, [open, searchPattern]);

  const showSummary = useCallback(
    (parts: string[]) => {
      onSummary?.(parts.filter(Boolean).join(' · '));
    },
    [onSummary],
  );

  const resolveExportKeys = useCallback(async (): Promise<string[]> => {
    if (exportMode === 'selected') {
      if (selectedKeys.length === 0) {
        throw new Error(t('redis.importExportNeedKeys'));
      }
      return selectedKeys;
    }
    const pattern = patternInput.trim() || '*';
    return scanKeysForPattern(connectionId, dbIndex, pattern);
  }, [connectionId, dbIndex, exportMode, patternInput, selectedKeys, t]);

  const loadPatternCount = async () => {
    try {
      const count = await invokeCountMatching(connectionId, dbIndex, patternInput.trim() || '*');
      setMatchCount(count);
    } catch {
      setMatchCount(null);
    }
  };

  const handleExportZip = async () => {
    setBusy(true);
    setError(null);
    try {
      const keys = await resolveExportKeys();
      if (keys.length === 0) {
        throw new Error(t('redis.importExportNoKeys'));
      }
      const result = await invokeDumpKeys(connectionId, dbIndex, keys);
      if (result.entries.length === 0) {
        throw new Error(t('redis.importExportDumpFailed'));
      }
      const zip = packDumpZip(dbIndex, result.entries);
      const saved = await fileCommands.saveBase64WithDialog(
        zipToBase64(zip),
        `redis-db${dbIndex}-dump.zip`,
        'Redis dump',
        ['zip'],
      );
      if (!saved) {
        return;
      }
      const errCount = result.errors.length;
      showSummary([
        t('redis.importExportExported').replace('{count}', String(result.entries.length)),
        errCount > 0 ? t('redis.errorsCount').replace('{count}', String(errCount)) : '',
      ]);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleExportJson = async () => {
    setBusy(true);
    setError(null);
    try {
      const keys = await resolveExportKeys();
      if (keys.length === 0) {
        throw new Error(t('redis.importExportNoKeys'));
      }
      const result = await invokeDumpKeys(connectionId, dbIndex, keys);
      if (result.entries.length === 0) {
        throw new Error(t('redis.importExportDumpFailed'));
      }
      const saved = await fileCommands.saveTextWithDialog(
        buildJsonExport(result.entries),
        `redis-db${dbIndex}-dump.json`,
        'JSON',
        ['json'],
      );
      if (!saved) {
        return;
      }
      showSummary([
        t('redis.importExportExported').replace('{count}', String(result.entries.length)),
      ]);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleImportZip = async () => {
    setBusy(true);
    setError(null);
    try {
      const opened = await fileCommands.openBase64WithDialog('Redis dump', ['zip']);
      if (!opened) {
        return;
      }
      const { manifest, restoreEntries } = parseDumpZip(base64ToZip(opened.dataBase64));
      if (manifest.dbIndex !== dbIndex) {
        throw new Error(
          t('redis.importExportDbMismatch')
            .replace('{expected}', String(dbIndex))
            .replace('{actual}', String(manifest.dbIndex)),
        );
      }
      if (restoreEntries.length === 0) {
        throw new Error(t('redis.importExportEmptyArchive'));
      }
      const result = await invokeRestoreKeys(connectionId, dbIndex, restoreEntries, replaceExisting);
      const errCount = result.errors.length;
      showSummary([
        t('redis.importExportRestored').replace('{count}', String(result.restored)),
        errCount > 0 ? t('redis.errorsCount').replace('{count}', String(errCount)) : '',
      ]);
      onOpenChange(false);
      await onRefresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={open}
      title={t('redis.importExportTitle')}
      description={t('redis.importExportDescription')}
      onClose={() => onOpenChange(false)}
      className="max-w-2xl"
      footer={
        <Button variant="ghost" onClick={() => onOpenChange(false)} disabled={busy}>
          {t('common.close')}
        </Button>
      }
    >
      <div className="space-y-6">
        <section className="space-y-3">
          <h3 className="text-sm font-medium text-fg">{t('redis.importExportExport')}</h3>
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              size="sm"
              variant={exportMode === 'selected' ? 'primary' : 'ghost'}
              onClick={() => setExportMode('selected')}
              disabled={busy}
            >
              {t('redis.importExportSelected')}
              {selectedKeys.length > 0 ? ` (${selectedKeys.length})` : ''}
            </Button>
            <Button
              type="button"
              size="sm"
              variant={exportMode === 'pattern' ? 'primary' : 'ghost'}
              onClick={() => setExportMode('pattern')}
              disabled={busy}
            >
              {t('redis.importExportPattern')}
            </Button>
          </div>

          {exportMode === 'pattern' ? (
            <div className="space-y-2">
              <Input
                value={patternInput}
                onChange={(e) => {
                  setPatternInput(e.target.value);
                  setMatchCount(null);
                }}
                placeholder={t('redis.pattern')}
                disabled={busy}
              />
              <div className="flex items-center gap-2">
                <Button type="button" size="sm" variant="ghost" onClick={() => void loadPatternCount()} disabled={busy}>
                  {t('redis.matchCount').replace('{count}', matchCount == null ? '…' : String(matchCount))}
                </Button>
              </div>
            </div>
          ) : null}

          <div className="flex flex-wrap gap-2">
            <Button type="button" size="sm" onClick={() => void handleExportZip()} disabled={busy}>
              {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <Download className="h-4 w-4" />}
              {t('redis.importExportZip')}
            </Button>
            <Button type="button" size="sm" variant="ghost" onClick={() => void handleExportJson()} disabled={busy}>
              {t('redis.importExportJson')}
            </Button>
          </div>
        </section>

        <section className="space-y-3 border-t border-edge pt-4">
          <h3 className="text-sm font-medium text-fg">{t('redis.importExportImport')}</h3>
          <label className="flex items-center gap-2 text-sm text-fg-muted">
            <input
              type="checkbox"
              checked={replaceExisting}
              onChange={(e) => setReplaceExisting(e.target.checked)}
              disabled={busy}
            />
            {t('redis.importExportReplace')}
          </label>
          <p className="text-xs text-fg-muted">{t('redis.importExportReplaceHint')}</p>
          <Button type="button" size="sm" onClick={() => void handleImportZip()} disabled={busy}>
            {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <Upload className="h-4 w-4" />}
            {t('redis.importExportImportZip')}
          </Button>
        </section>

        {error ? <p className="text-sm text-danger">{error}</p> : null}
      </div>
    </Dialog>
  );
}
