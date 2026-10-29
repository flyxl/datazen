import { useState } from 'react';
import { useI18n } from '../../hooks/useI18n';
import { Button } from '../../components/ui/Button';
import { Checkbox } from '../../components/ui/Checkbox';
import { Input } from '../../components/ui/Input';
import { Select } from '../../components/ui/Select';
import type {
  DataSyncRecordset,
  DataSyncRecordsetBound,
  DataSyncRecordsetTupleBound,
} from '../../commands/sync';

interface RecordsetEditorProps {
  primaryKeys: string[];
  recordset?: DataSyncRecordset;
  onChange: (recordset: DataSyncRecordset | undefined) => void;
}

export function RecordsetEditor({ primaryKeys, recordset, onChange }: RecordsetEditorProps) {
  const { t } = useI18n();
  const [limitError, setLimitError] = useState(false);
  const defaultOrder = primaryKeys.length === 1 ? primaryKeys[0] : '';
  const orderBy = recordset?.orderBy ?? defaultOrder;
  const tupleMode = Boolean(
    recordset?.tupleRange ||
      (primaryKeys.length > 1 && !recordset?.orderBy && !recordset?.start && !recordset?.end),
  );
  const update = (patch: Partial<DataSyncRecordset>) =>
    onChange({ ...(recordset ?? {}), ...patch });
  const updateBound = (name: 'start' | 'end', patch: Partial<DataSyncRecordsetBound>) => {
    const current = recordset?.[name];
    update({ [name]: { ...(current ?? { value: '', inclusive: true }), ...patch } });
  };
  const updateTupleBound = (name: 'start' | 'end', patch: Partial<DataSyncRecordsetTupleBound>) => {
    const tupleRange = recordset?.tupleRange ?? { columns: primaryKeys };
    const current = tupleRange[name];
    const next = {
      ...tupleRange,
      [name]: { ...(current ?? { values: [], inclusive: true }), ...patch },
    };
    update({ tupleRange: next, orderBy: undefined, start: undefined, end: undefined });
  };
  const setTupleComponent = (name: 'start' | 'end', index: number, value: string) => {
    const tupleRange = recordset?.tupleRange ?? { columns: primaryKeys };
    const current = tupleRange[name];
    const values = Array.from({ length: primaryKeys.length }, (_, component) =>
      component === index ? value : (current?.values[component] ?? ''),
    );
    if (values.every((component) => component === '')) {
      const next = { ...tupleRange };
      delete next[name];
      update({ tupleRange: next });
    } else {
      updateTupleBound(name, { values });
    }
  };
  const setLimit = (value: string) => {
    if (!value) {
      setLimitError(false);
      update({ limit: undefined });
      return;
    }
    const parsed = Number(value);
    if (!/^\d+$/.test(value) || !Number.isSafeInteger(parsed) || parsed < 1) {
      setLimitError(true);
      return;
    }
    setLimitError(false);
    update({ limit: parsed });
  };

  return (
    <div
      className="space-y-2 rounded-lg border border-edge bg-surface-alt p-3"
      data-testid="data-sync-recordset"
    >
      <div className="flex items-center justify-between gap-2">
        <div>
          <div className="text-sm font-medium">{t('sync.recordset')}</div>
          <div className="text-xs text-fg-muted">{t('sync.recordsetHint')}</div>
        </div>
        <Button
          variant="ghost"
          size="sm"
          data-testid="data-sync-recordset-toggle"
          onClick={() =>
            onChange(
              recordset
                ? undefined
                : primaryKeys.length > 1
                  ? { tupleRange: { columns: [...primaryKeys] } }
                  : { orderBy: defaultOrder || undefined },
            )
          }
          disabled={primaryKeys.length === 0}
        >
          {recordset ? t('sync.recordsetDisable') : t('sync.recordsetEnable')}
        </Button>
      </div>

      {recordset && (
        <div className="space-y-2" data-testid="data-sync-recordset-editor">
          {tupleMode ? (
            <div className="space-y-2" data-testid="data-sync-recordset-tuple-editor">
              <div className="text-xs text-fg-muted">{primaryKeys.join(' → ')}</div>
              {(['start', 'end'] as const).map((name) => {
                const bound = recordset.tupleRange?.[name];
                return (
                  <div key={name} className="space-y-1 rounded border border-edge p-2">
                    <div className="text-xs text-fg-muted">
                      {name === 'start' ? t('sync.recordsetStart') : t('sync.recordsetEnd')}
                    </div>
                    {primaryKeys.map((column, index) => (
                      <label key={column} className="flex items-center gap-2 text-xs">
                        <span className="w-24 truncate text-fg-muted">{column}</span>
                        <Input
                          className="h-7 min-w-36 flex-1 px-2 text-xs"
                          value={bound?.values[index] ?? ''}
                          placeholder={t('sync.recordsetUnbounded')}
                          data-testid={`data-sync-recordset-${name}-${index}`}
                          onChange={(event) => setTupleComponent(name, index, event.target.value)}
                        />
                      </label>
                    ))}
                    {bound && (
                      <label className="flex items-center gap-1 text-xs text-fg-muted">
                        <Checkbox
                          checked={bound.inclusive !== false}
                          data-testid={`data-sync-recordset-${name}-inclusive`}
                          onChange={(event) =>
                            updateTupleBound(name, { inclusive: event.target.checked })
                          }
                        />
                        {t('sync.recordsetInclusive')}
                      </label>
                    )}
                  </div>
                );
              })}
            </div>
          ) : (
            <>
              <label className="flex items-center gap-2 text-xs text-fg-muted">
                <span>{t('sync.recordsetOrder')}</span>
                <Select
                  value={orderBy}
                  options={primaryKeys.map((column) => ({ value: column, label: column }))}
                  onChange={(value) => update({ orderBy: value || undefined })}
                  className="!h-7 min-w-36 !text-xs"
                  triggerDataAttrs={{ 'data-testid': 'data-sync-recordset-order' }}
                />
              </label>

              {(['start', 'end'] as const).map((name) => {
                const bound = recordset[name];
                return (
                  <div key={name} className="flex flex-wrap items-center gap-2">
                    <span className="w-16 text-xs text-fg-muted">
                      {name === 'start' ? t('sync.recordsetStart') : t('sync.recordsetEnd')}
                    </span>
                    <Input
                      className="h-7 min-w-36 flex-1 px-2 text-xs"
                      value={bound?.value ?? ''}
                      placeholder={t('sync.recordsetUnbounded')}
                      data-testid={`data-sync-recordset-${name}`}
                      onChange={(event) => {
                        const value = event.target.value;
                        if (!value) {
                          const next = { ...(recordset ?? {}) };
                          delete next[name];
                          onChange(next);
                        } else {
                          updateBound(name, { value });
                        }
                      }}
                    />
                    {bound && (
                      <label className="flex items-center gap-1 text-xs text-fg-muted">
                        <Checkbox
                          checked={bound.inclusive !== false}
                          data-testid={`data-sync-recordset-${name}-inclusive`}
                          onChange={(event) =>
                            updateBound(name, { inclusive: event.target.checked })
                          }
                        />
                        {t('sync.recordsetInclusive')}
                      </label>
                    )}
                  </div>
                );
              })}
            </>
          )}

          <label className="flex items-center gap-2 text-xs text-fg-muted">
            <span className="w-16">{t('sync.recordsetLimit')}</span>
            <Input
              type="number"
              min={1}
              value={recordset.limit ?? ''}
              placeholder={t('sync.recordsetUnbounded')}
              data-testid="data-sync-recordset-limit"
              onChange={(event) => setLimit(event.target.value)}
              className="h-7 w-36 px-2 text-xs"
            />
          </label>
          {limitError && (
            <p className="text-xs text-warning" data-testid="data-sync-recordset-limit-error">
              {t('sync.recordsetLimitInvalid')}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
