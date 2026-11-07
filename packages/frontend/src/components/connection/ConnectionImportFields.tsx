import { Input } from '../ui/Input';
import { Button } from '../ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { importFileDisplayName } from '../../lib/importConnectionFile';
import { cn } from '../../lib/cn';
import type { ConnectionImportState } from './useConnectionImport';

interface ConnectionImportFieldsProps {
  import: ConnectionImportState;
  /** `inline` is the first-run journey form; `dialog` matches the share dialog. */
  variant?: 'dialog' | 'inline';
}

/**
 * Body of the connection import flow (source path / chosen file / password /
 * inline error). Rendered by `ConnectionShareDialog` and by the onboarding
 * wizard's inline import step so both stay in lockstep.
 */
export function ConnectionImportFields({
  import: imp,
  variant = 'dialog',
}: Readonly<ConnectionImportFieldsProps>) {
  const { t } = useI18n();
  const inline = variant === 'inline';
  const fieldGap = inline ? 'space-y-2.5' : 'space-y-2';
  const labelClass = cn(
    'block font-medium text-fg-secondary',
    inline ? 'mb-1.5 text-[12.5px]' : 'mb-1 text-xs',
  );

  return (
    <div className={inline ? 'space-y-4' : 'space-y-4'}>
      {!imp.appImport && (
        <p className="text-xs leading-relaxed text-fg-muted">{t('connShare.importFormatsHint')}</p>
      )}

      {imp.appImport && (
        <div className={fieldGap}>
          <p className="text-xs leading-relaxed text-fg-muted">
            {imp.pathFound ? t('connShare.dataPathFoundHint') : t('connShare.dataPathMissingHint')}
          </p>
          <label className={labelClass}>{t('connShare.dataPath')}</label>
          <Input
            data-testid="import-data-path"
            value={imp.dataPath}
            onChange={(e) => imp.setDataPath(e.target.value)}
            disabled={imp.busy}
            placeholder={t('connShare.dataPathPlaceholder')}
          />
          <div className="flex flex-wrap gap-2">
            <Button
              variant="secondary"
              onClick={() => void imp.browsePath('folder')}
              disabled={imp.busy}
            >
              {t('connShare.browseFolder')}
            </Button>
            <Button
              variant="secondary"
              onClick={() => void imp.browsePath('file')}
              disabled={imp.busy}
            >
              {t('connShare.browseFile')}
            </Button>
          </div>
        </div>
      )}

      {!imp.appImport && imp.selectedFile && (
        <div className={fieldGap}>
          <label className={labelClass}>{t('connShare.selectedImportFile')}</label>
          <div className="flex flex-wrap items-center gap-2">
            <span
              className="min-w-0 flex-1 truncate text-sm text-fg-primary"
              data-testid="import-selected-file"
              title={imp.selectedFile}
            >
              {importFileDisplayName(imp.selectedFile)}
            </span>
            <Button
              variant="secondary"
              onClick={() => void imp.pickImportFile()}
              disabled={imp.submitting}
            >
              {t('connShare.changeImportFile')}
            </Button>
          </div>
        </div>
      )}

      {(imp.appImport || imp.selectedFile !== null) && (
        <div>
          <label className={labelClass}>
            {t('connShare.password')}
            {imp.importPasswordPolicy !== 'required' ? (
              <span className="ml-1 font-normal text-fg-muted">
                ({t('connShare.passwordOptional')})
              </span>
            ) : null}
          </label>
          <Input
            type="password"
            value={imp.password}
            onChange={(e) => imp.setPassword(e.target.value)}
            autoComplete="new-password"
            disabled={imp.submitting}
            placeholder={t('connShare.passwordImportPlaceholder')}
          />
        </div>
      )}

      {imp.localError && (
        <div
          className="rounded-md border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-400"
          data-testid="connection-import-error"
        >
          {imp.localError}
        </div>
      )}
    </div>
  );
}
