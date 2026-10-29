import { Select } from '../../components/ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { useSettingsStore } from '../../stores/settingsStore';
import type { DefaultTableSort, GridDensity, RowCountMode } from '../../types/settings';
import { SettingRow, ToggleRow } from './settingsUi';

const ROW_COUNT_MODES: readonly RowCountMode[] = ['auto', 'exact', 'estimated', 'none'];

const DEFAULT_SORTS: readonly DefaultTableSort[] = ['primaryKeyAsc', 'primaryKeyDesc', 'none'];

const GRID_DENSITIES: readonly GridDensity[] = ['compact', 'standard', 'relaxed'];

function isRowCountMode(value: string): value is RowCountMode {
  return value === 'auto' || value === 'exact' || value === 'estimated' || value === 'none';
}

function isDefaultTableSort(value: string): value is DefaultTableSort {
  return value === 'primaryKeyAsc' || value === 'primaryKeyDesc' || value === 'none';
}

function isGridDensity(value: string): value is GridDensity {
  return value === 'compact' || value === 'standard' || value === 'relaxed';
}

/**
 * The settings a table read consults, and the settings a grid looks like.
 *
 * They live in the "Data Browsing" section because that is where the user
 * already expects the page size — the row count, the order of a table and the
 * look of its grid are the same kind of decision.
 *
 * The threshold only shows in the `auto` mode: the other modes do not read
 * it, and a field that does nothing is worse than no field.
 *
 * The appearance block (DB-17) carries the grid's defaults on every control:
 * `standard` density, striped rows, the `#` gutter and visible scrollbars are
 * what the grids did before the settings existed, so a fresh install sees
 * exactly the grid it always saw.
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

      {/* Grid appearance (DB-17): density is a delta each grid applies to its
          own row height, so "standard" is not "28px" — it is "unchanged". */}
      <SettingRow label={t('settings.gridDensity')} hint={t('settings.gridDensity.hint')}>
        <Select
          value={settings.gridDensity}
          options={GRID_DENSITIES.map((density) => ({
            value: density,
            label: t(`settings.gridDensity.${density}`),
          }))}
          onChange={(value) => {
            if (isGridDensity(value)) void updateSettings({ gridDensity: value });
          }}
          data-testid="settings-grid-density"
        />
      </SettingRow>

      <ToggleRow
        label={t('settings.gridAlternatingRows')}
        hint={t('settings.gridAlternatingRows.hint')}
        checked={settings.gridAlternatingRows}
        onChange={(v) => void updateSettings({ gridAlternatingRows: v })}
        testId="settings-grid-alternating-rows"
      />

      <ToggleRow
        label={t('settings.gridRowNumbers')}
        hint={t('settings.gridRowNumbers.hint')}
        checked={settings.gridRowNumbers}
        onChange={(v) => void updateSettings({ gridRowNumbers: v })}
        testId="settings-grid-row-numbers"
      />

      <ToggleRow
        label={t('settings.gridAutoHideScrollbar')}
        hint={t('settings.gridAutoHideScrollbar.hint')}
        checked={settings.gridAutoHideScrollbar}
        onChange={(v) => void updateSettings({ gridAutoHideScrollbar: v })}
        testId="settings-grid-auto-hide-scrollbar"
      />
    </>
  );
}
