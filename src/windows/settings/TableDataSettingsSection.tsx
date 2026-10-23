import { Select } from '../../components/ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { useSettingsStore } from '../../stores/settingsStore';
import type { DefaultTableSort, RowCountMode } from '../../types/settings';
import { SettingRow } from './settingsUi';

const ROW_COUNT_MODES: readonly RowCountMode[] = ['auto', 'exact', 'estimated', 'none'];

const DEFAULT_SORTS: readonly DefaultTableSort[] = ['primaryKeyAsc', 'primaryKeyDesc', 'none'];

function isRowCountMode(value: string): value is RowCountMode {
  return value === 'auto' || value === 'exact' || value === 'estimated' || value === 'none';
}

function isDefaultTableSort(value: string): value is DefaultTableSort {
  return value === 'primaryKeyAsc' || value === 'primaryKeyDesc' || value === 'none';
}

/**
 * The three settings a table read consults.
 *
 * They live in the "Data Browsing" section because that is where the user
 * already expects the page size — the row count and the order of a table are
 * the same kind of decision.
 *
 * The threshold only shows in the `auto` mode: the other modes do not read
 * it, and a field that does nothing is worse than no field.
 */
export function TableDataSettingsSection() {
  const { t } = useI18n();
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  return (
    <>
      <SettingRow label={t('settings.rowCountMode')}>
        <Select
          value={settings.rowCountMode}
          options={ROW_COUNT_MODES.map((mode) => ({
            value: mode,
            label: t(`settings.rowCountMode.${mode}`),
          }))}
          onChange={(value) => {
            if (isRowCountMode(value)) void updateSettings({ rowCountMode: value });
          }}
          data-testid="settings-row-count-mode"
        />
      </SettingRow>

      {settings.rowCountMode === 'auto' && (
        <SettingRow label={t('settings.rowCountEstimateThreshold')}>
          <input
            type="number"
            min={0}
            step={1000}
            value={settings.rowCountEstimateThreshold}
            onChange={(event) => {
              const next = Number(event.target.value);
              if (!Number.isFinite(next) || next < 0) return;
              void updateSettings({ rowCountEstimateThreshold: Math.round(next) });
            }}
            className="h-9 w-32 rounded-md border border-edge bg-surface px-3 text-sm tabular-nums text-fg outline-none focus:border-accent focus:ring-2 focus:ring-accent/25"
            data-testid="settings-row-count-threshold"
          />
        </SettingRow>
      )}

      <SettingRow label={t('settings.defaultTableSort')} hint={t('settings.defaultTableSort.hint')}>
        <Select
          value={settings.defaultTableSort}
          options={DEFAULT_SORTS.map((sort) => ({
            value: sort,
            label: t(`settings.defaultTableSort.${sort}`),
          }))}
          onChange={(value) => {
            if (isDefaultTableSort(value)) void updateSettings({ defaultTableSort: value });
          }}
          data-testid="settings-default-table-sort"
        />
      </SettingRow>
    </>
  );
}
