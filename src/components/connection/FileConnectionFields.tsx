import { useState, useCallback } from 'react';
import { Input } from '../ui/Input';
import { PathInput } from '../ui/PathInput';
import { Button } from '../ui/Button';
import { Select } from '../ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { Label } from './shared';
import type { ConnectionFormState } from './useConnectionForm';
import {
  adbListPackages,
  adbListDatabases,
  adbPullDatabase,
  type AdbPackage,
  type AdbDatabaseFile,
} from '../../commands/adb';

export function FileConnectionFields({ form }: { form: ConnectionFormState }) {
  const { t } = useI18n();
  const [adbMode, setAdbMode] = useState(false);
  const [packages, setPackages] = useState<AdbPackage[]>([]);
  const [databases, setDatabases] = useState<AdbDatabaseFile[]>([]);
  const [selectedPackage, setSelectedPackage] = useState('');
  const [selectedDbPath, setSelectedDbPath] = useState('');
  const [localSavePath, setLocalSavePath] = useState('');
  const [packageFilter, setPackageFilter] = useState('');
  const [loadingPackages, setLoadingPackages] = useState(false);
  const [loadingDatabases, setLoadingDatabases] = useState(false);
  const [pulling, setPulling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  const loadPackages = useCallback(async () => {
    setLoadingPackages(true);
    setError(null);
    try {
      const pkgs = await adbListPackages();
      setPackages(pkgs);
      if (pkgs.length === 0) {
        setError(t('newConn.adbNoPackages'));
      }
    } catch (e) {
      const msg = typeof e === 'string' ? e : e instanceof Error ? e.message : String(e);
      setError(msg);
    } finally {
      setLoadingPackages(false);
    }
  }, [t]);

  const handlePackageChange = useCallback(
    async (pkg: string) => {
      setSelectedPackage(pkg);
      setSelectedDbPath('');
      setDatabases([]);
      if (!pkg) return;

      setLoadingDatabases(true);
      setError(null);
      try {
        const dbs = await adbListDatabases(pkg);
        setDatabases(dbs);
        if (dbs.length === 0) {
          setError(t('newConn.adbNoDatabases'));
        }
      } catch (e) {
        const msg = typeof e === 'string' ? e : e instanceof Error ? e.message : String(e);
        setError(msg);
      } finally {
        setLoadingDatabases(false);
      }
    },
    [t],
  );

  const handlePull = useCallback(async () => {
    if (!selectedPackage || !selectedDbPath || !localSavePath) return;

    setPulling(true);
    setError(null);
    setSuccess(null);
    try {
      const saved = await adbPullDatabase(selectedPackage, selectedDbPath, localSavePath);
      setSuccess(t('newConn.adbPullSuccess'));
      form.setDatabase(saved);
    } catch (e) {
      const msg = typeof e === 'string' ? e : e instanceof Error ? e.message : String(e);
      setError(`${t('newConn.adbPullFailed')}: ${msg}`);
    } finally {
      setPulling(false);
    }
  }, [selectedPackage, selectedDbPath, localSavePath, form, t]);

  const handleToggleAdb = useCallback(() => {
    const next = !adbMode;
    setAdbMode(next);
    if (next && packages.length === 0) {
      void loadPackages();
    }
  }, [adbMode, packages.length, loadPackages]);

  const filteredPackages = packageFilter
    ? packages.filter((p) =>
        p.package_name.toLowerCase().includes(packageFilter.toLowerCase()),
      )
    : packages;

  return (
    <div className="md:col-span-2 space-y-3">
      {/* File path input (always visible) */}
      <div>
        <Label required>{t('newConn.dbFilePath')}</Label>
        <PathInput
          value={form.database}
          onChange={form.setDatabase}
          placeholder="/path/to/db.sqlite"
          error={!!form.validationErrors.database}
          dialogOptions={{
            filters: [{ name: 'SQLite Database', extensions: ['db', 'sqlite', 'sqlite3'] }],
          }}
        />
        {form.validationErrors.database && (
          <p className="mt-1 text-xs text-red-400">{form.validationErrors.database}</p>
        )}
      </div>

      {/* ADB mode toggle */}
      <label className="flex items-center gap-2 text-sm cursor-pointer select-none text-fg-muted hover:text-fg-secondary">
        <input
          type="checkbox"
          checked={adbMode}
          onChange={handleToggleAdb}
          className="rounded border-edge bg-surface-raised"
        />
        {t('newConn.adbMode')}
      </label>

      {adbMode && (
        <div className="space-y-3 p-3 rounded-lg border border-edge bg-surface-alt/50">
          {/* Package selector with filter */}
          <div>
            <Label>{t('newConn.adbSelectPackage')}</Label>
            <Input
              value={packageFilter}
              onChange={(e) => setPackageFilter(e.target.value)}
              placeholder={t('newConn.adbSearchPackage')}
              className="mb-1"
            />
            {loadingPackages ? (
              <p className="text-xs text-fg-muted">
                {t('newConn.adbLoadingPackages')}
              </p>
            ) : (
              <Select
                value={selectedPackage}
                onChange={(val) => void handlePackageChange(val)}
                placeholder="--"
                options={filteredPackages.map((p) => ({
                  value: p.package_name,
                  label: p.package_name,
                }))}
              />
            )}
          </div>

          {/* Database file selector */}
          {selectedPackage && (
            <div>
              <Label>{t('newConn.adbSelectDatabase')}</Label>
              {loadingDatabases ? (
                <p className="text-xs text-fg-muted">
                {t('newConn.adbLoadingDatabases')}
              </p>
              ) : (
                <Select
                  value={selectedDbPath}
                  onChange={setSelectedDbPath}
                  placeholder="--"
                  options={databases.map((db) => ({
                    value: db.path,
                    label: `${db.name} (${db.path})`,
                  }))}
                />
              )}
            </div>
          )}

          {/* Local save path */}
          {selectedDbPath && (
            <div>
              <Label required>{t('newConn.adbLocalPath')}</Label>
              <PathInput
                value={localSavePath}
                onChange={setLocalSavePath}
                placeholder={t('newConn.adbLocalPathPlaceholder')}
                dialogOptions={{ directory: true }}
              />
            </div>
          )}

          {/* Pull button */}
          {selectedPackage && selectedDbPath && localSavePath && (
            <Button
              onClick={handlePull}
              disabled={pulling}
              className="w-full"
            >
              {pulling ? t('newConn.adbPulling') : t('newConn.adbPull')}
            </Button>
          )}

          {/* Status messages */}
          {error && (
            <p className="text-xs text-[var(--c-danger)]">{error}</p>
          )}
          {success && (
            <p className="text-xs text-[var(--c-success)]">{success}</p>
          )}
        </div>
      )}
    </div>
  );
}
