import { Input } from '../ui/Input';
import { Select } from '../ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import { Label } from './shared';
import { CatalogConnectionFields } from './CatalogConnectionFields';
import { ConnectionAdvancedSettings } from './ConnectionAdvancedSettings';
import { FileConnectionFields } from './FileConnectionFields';
import { IndexConnectionFields } from './IndexConnectionFields';
import { KiwiConnectionFields } from './KiwiConnectionFields';
import { StandardConnectionFields } from './StandardConnectionFields';
import type { ConnectionFormState } from './useConnectionForm';
import type { DatabaseType } from '../../types';

export interface ConnectionFormBodyProps {
  form: ConnectionFormState;
  showDbTypeSelect?: boolean;
  groupOptions?: { value: string; label: string }[];
  variant?: 'dialog' | 'window';
}

export function ConnectionFormBody({
  form,
  showDbTypeSelect = false,
  groupOptions,
  variant = 'dialog',
}: ConnectionFormBodyProps) {
  const { t } = useI18n();
  const isWindow = variant === 'window';

  return (
    <>
      <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
        <div className="md:col-span-2">
          <Label required>{t('newConn.connName')}</Label>
          <Input
            value={form.name}
            onChange={(e) => form.setName(e.target.value)}
            placeholder={t('newConn.namePlaceholder')}
            autoFocus={isWindow}
          />
        </div>

        {showDbTypeSelect && (
          <>
            <div>
              <Label required>{t('newConn.dbType')}</Label>
              <Select
                value={form.databaseType}
                options={Object.entries(DB_REGISTRY).map(([value, meta]) => ({
                  value,
                  label: meta.label,
                }))}
                onChange={(v) => form.handleDatabaseTypeChange(v as DatabaseType)}
              />
            </div>
            <div>
              <Label>{t('newConn.group')}</Label>
              <Select
                value={form.group}
                options={[
                  { value: '生产环境', label: t('newConn.groupProd') },
                  { value: '开发环境', label: t('newConn.groupDev') },
                  { value: '测试环境', label: t('newConn.groupTest') },
                ]}
                onChange={form.setGroup}
              />
            </div>
          </>
        )}

        {form.formVariant === 'file' && <FileConnectionFields form={form} />}
        {form.formVariant === 'kiwi' && <KiwiConnectionFields form={form} />}
        {form.formVariant === 'index' && (
          <StandardConnectionFields
            form={form}
            databaseField={<IndexConnectionFields form={form} />}
            hostPlaceholder={isWindow ? 'prod-db.example.com' : '127.0.0.1'}
          />
        )}
        {form.formVariant === 'catalog' && (
          <CatalogConnectionFields
            form={form}
            hostPlaceholder={isWindow ? 'presto.example.com' : '127.0.0.1'}
          />
        )}
        {form.formVariant === 'standard' && (
          <StandardConnectionFields
            form={form}
            hostPlaceholder={isWindow ? 'prod-db.example.com' : '127.0.0.1'}
            databasePlaceholder={isWindow ? 'myapp_production' : undefined}
          />
        )}
      </div>

      <ConnectionAdvancedSettings form={form} groupOptions={groupOptions} variant={variant} />

      <div ref={form.testResultRef}>
        {form.testOk && (
          <div className="mt-4 rounded-md border border-green-500/20 bg-green-500/10 p-3 text-xs text-green-400">
            {t('newConn.testSuccess')}{form.testOk}
          </div>
        )}
        {form.testErr && (
          <div className="mt-4 rounded-md border border-red-500/20 bg-red-500/10 p-3 text-xs text-red-400 break-all">
            {form.testErr}
          </div>
        )}
      </div>
    </>
  );
}
