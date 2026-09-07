import { Shield, Star, Copy, Edit2, Trash2, CheckCircle2 } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { useI18n } from '../../../hooks/useI18n';
import { detectSafetyPreset } from '../../../lib/aiSafetyPresets';
import type { AiModelProfile } from '../../../types';
import { tid } from '../../../lib/tid';

interface ModelProfileCardProps {
  profile: AiModelProfile;
  isDefault: boolean;
  isActive: boolean;
  onSetDefault: () => void;
  onEdit: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
}

export function ModelProfileCard({
  profile,
  isDefault,
  isActive,
  onSetDefault,
  onEdit,
  onDuplicate,
  onDelete,
}: Readonly<ModelProfileCardProps>) {
  const { t } = useI18n();
  const preset = detectSafetyPreset(profile.safetyGate);

  const getPresetBadge = () => {
    switch (preset) {
      case 'cloud_strict':
        return (
          <span className="inline-flex items-center gap-1 rounded bg-blue-500/10 px-2 py-0.5 text-xs font-medium text-blue-400">
            <Shield className="h-3 w-3" />
            {t('settings.ai.presetCloudStrict')}
          </span>
        );
      case 'enterprise_balanced':
        return (
          <span className="inline-flex items-center gap-1 rounded bg-amber-500/10 px-2 py-0.5 text-xs font-medium text-amber-400">
            <Shield className="h-3 w-3" />
            {t('settings.ai.presetEnterpriseBalanced')}
          </span>
        );
      case 'local_trust':
        return (
          <span className="inline-flex items-center gap-1 rounded bg-emerald-500/10 px-2 py-0.5 text-xs font-medium text-emerald-400">
            <Shield className="h-3 w-3" />
            {t('settings.ai.presetLocalTrust')}
          </span>
        );
      default:
        return (
          <span className="inline-flex items-center gap-1 rounded bg-purple-500/10 px-2 py-0.5 text-xs font-medium text-purple-400">
            <Shield className="h-3 w-3" />
            {t('settings.ai.presetCustom')}
          </span>
        );
    }
  };

  return (
    <div
      className={`group relative flex flex-col justify-between rounded-lg border p-4 transition-all ${
        isActive
          ? 'border-accent/60 bg-surface-raised shadow-sm'
          : 'border-edge bg-surface hover:border-edge-focus'
      }`}
      {...tid(`model-profile-card-${profile.id}`)}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h4 className="truncate text-sm font-semibold text-fg">
              {profile.name || profile.model || 'Unnamed Model'}
            </h4>
            {isDefault && (
              <span className="inline-flex items-center gap-0.5 rounded bg-amber-500/20 px-1.5 py-0.5 text-[11px] font-semibold text-amber-400">
                <Star className="h-2.5 w-2.5 fill-current" />
                {t('settings.ai.defaultBadge')}
              </span>
            )}
            {isActive && !isDefault && (
              <span className="inline-flex items-center gap-0.5 rounded bg-accent/20 px-1.5 py-0.5 text-[11px] font-medium text-accent">
                <CheckCircle2 className="h-2.5 w-2.5" />
                Active
              </span>
            )}
          </div>
          <div className="mt-1 flex flex-wrap items-center gap-2 text-xs text-fg-muted">
            <span className="font-mono text-fg-subtle">{profile.providerType}</span>
            <span>•</span>
            <span className="font-mono truncate max-w-[200px]" title={profile.model}>
              {profile.model}
            </span>
            {profile.endpoint && (
              <>
                <span>•</span>
                <span className="truncate max-w-[220px]" title={profile.endpoint}>
                  {profile.endpoint}
                </span>
              </>
            )}
          </div>
        </div>

        <div className="shrink-0">{getPresetBadge()}</div>
      </div>

      <div className="mt-4 flex items-center justify-between border-t border-edge/40 pt-3">
        <div className="text-xs text-fg-subtle">
          {profile.safetyGate.dataEgressLevel === 'strict' && 'Strict Egress'}
          {profile.safetyGate.dataEgressLevel === 'sample_masked' &&
            `Sample Masked (≤ ${profile.safetyGate.maxSampleRows} rows)`}
          {profile.safetyGate.dataEgressLevel === 'unrestricted' && 'Unrestricted Egress'}
        </div>

        <div className="flex items-center gap-1">
          {!isDefault && (
            <Button
              variant="ghost"
              size="sm"
              onClick={onSetDefault}
              className="h-7 px-2 text-xs text-fg-muted hover:text-amber-400"
              title={t('settings.ai.setDefault')}
            >
              <Star className="h-3 w-3 mr-1" />
              {t('settings.ai.setDefault')}
            </Button>
          )}
          <Button
            variant="ghost"
            size="sm"
            onClick={onDuplicate}
            className="h-7 w-7 p-0 text-fg-muted hover:text-fg"
            title={t('settings.ai.duplicate')}
          >
            <Copy className="h-3.5 w-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={onEdit}
            className="h-7 w-7 p-0 text-fg-muted hover:text-accent"
            title={t('settings.ai.editModel')}
          >
            <Edit2 className="h-3.5 w-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={onDelete}
            className="h-7 w-7 p-0 text-fg-muted hover:text-red-400"
            title={t('common.delete')}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>
    </div>
  );
}
