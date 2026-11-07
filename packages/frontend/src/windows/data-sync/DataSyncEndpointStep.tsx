import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { Select } from '../../components/ui/Select';
import type { SyncProfile } from '../../commands/sync';
import { useI18n } from '../../hooks/useI18n';
import { EndpointsBar, type EndpointsBarProps } from './EndpointsBar';

export interface DataSyncEndpointStepProps {
  /** An unknown history run could not restore its endpoint scope. */
  scopeReconfirmationRequired: boolean;
  onConfirmScope: () => void;
  profileName: string;
  onProfileNameChange: (name: string) => void;
  profiles: SyncProfile[];
  selectedProfileId: string;
  onSelectProfile: (id: string) => void;
  onSaveProfile: () => void;
  onLoadProfile: () => void;
  onDeleteProfile: () => void;
  endpoints: EndpointsBarProps;
}

/**
 * First step of the Data Sync wizard: the endpoint selection and the saved
 * profiles that restore a previously reviewed endpoint scope.
 */
export function DataSyncEndpointStep({
  scopeReconfirmationRequired,
  onConfirmScope,
  profileName,
  onProfileNameChange,
  profiles,
  selectedProfileId,
  onSelectProfile,
  onSaveProfile,
  onLoadProfile,
  onDeleteProfile,
  endpoints,
}: DataSyncEndpointStepProps) {
  const { t } = useI18n();

  return (
    <div className="space-y-4">
      {scopeReconfirmationRequired && (
        <div
          className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-warning/40 bg-warning/10 p-3"
          data-testid="data-sync-recovery-scope-review"
          role="status"
        >
          <p className="text-xs text-fg-secondary">{t('migrationHistory.syncReconcileNeedsScope')}</p>
          <Button
            variant="secondary"
            size="sm"
            data-testid="data-sync-reconfirm-scope"
            onClick={onConfirmScope}
          >
            {t('migrationHistory.syncReconcileConfirmScope')}
          </Button>
        </div>
      )}
      <div
        data-testid="data-sync-profile-controls"
        className="flex flex-wrap items-end gap-2 rounded-lg border border-edge bg-surface-alt p-3"
      >
        <label className="min-w-48 flex-1 text-xs text-fg-muted">
          <span className="mb-1 block">{t('sync.profile.name')}</span>
          <Input
            data-testid="data-sync-profile-name"
            value={profileName}
            onChange={(event) => onProfileNameChange(event.target.value)}
            placeholder={t('sync.profile.namePlaceholder')}
            className="h-8 w-full px-2 text-sm"
          />
        </label>
        <label className="min-w-48 text-xs text-fg-muted">
          <span className="mb-1 block">{t('sync.profile.load')}</span>
          <Select
            className="w-full"
            triggerDataAttrs={{ 'data-testid': 'data-sync-profile-select' }}
            value={selectedProfileId}
            placeholder={t('sync.profile.select')}
            options={profiles.map((profile) => ({
              value: profile.id,
              label: profile.name,
            }))}
            onChange={onSelectProfile}
          />
        </label>
        <Button size="sm" data-testid="data-sync-profile-save" onClick={onSaveProfile}>
          {t('sync.profile.save')}
        </Button>
        <Button
          variant="secondary"
          size="sm"
          data-testid="data-sync-profile-load"
          disabled={!selectedProfileId}
          onClick={onLoadProfile}
        >
          {t('sync.profile.load')}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          data-testid="data-sync-profile-delete"
          disabled={!selectedProfileId}
          onClick={onDeleteProfile}
        >
          {t('sync.profile.delete')}
        </Button>
      </div>
      <EndpointsBar {...endpoints} />
    </div>
  );
}
