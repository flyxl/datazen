import { useEffect, useState } from 'react';
import { FolderOpen, Plus, Sparkles } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { PathInput } from '../../components/ui/PathInput';
import { useSettingsStore } from '../../stores/settingsStore';
import { useAiStore } from '../../stores/aiStore';
import { useI18n } from '../../hooks/useI18n';
import { settingsCommands } from '../../commands/settings';
import type { AiModelProfile } from '../../types';
import { SectionTitle, ToggleRow } from './settingsUi';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { ModelProfileListItem } from './ai/ModelProfileListItem';
import { ModelProfileDialog } from './ai/ModelProfileDialog';
import { tid } from '../../lib/tid';

function ContextDirSetting() {
  const { t } = useI18n();
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);
  const [defaultDir, setDefaultDir] = useState('');
  const [saved, setSaved] = useState(false);
  const [localDir, setLocalDir] = useState(settings.contextDir);

  useEffect(() => {
    import('../../commands/context').then(({ contextCommands }) => {
      void contextCommands
        .getDir()
        .then(setDefaultDir)
        .catch(() => {});
    });
  }, []);

  useEffect(() => {
    setLocalDir(settings.contextDir);
  }, [settings.contextDir]);

  const handleSave = async () => {
    await updateSettings({ ...settings, contextDir: localDir });
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  return (
    <div className="flex items-center gap-2">
      <PathInput
        value={localDir}
        onChange={setLocalDir}
        placeholder={defaultDir || t('context.dirSettingDesc')}
        dialogOptions={{ directory: true }}
        className="flex-1"
      />
      <Button variant="secondary" className="shrink-0 h-9" onClick={() => void handleSave()}>
        {saved ? t('common.success') : t('common.save')}
      </Button>
      <Button
        variant="ghost"
        className="shrink-0 h-9 w-9 px-0"
        onClick={() => void settingsCommands.openContextDir()}
        title={t('context.openDir')}
        aria-label={t('context.openDir')}
      >
        <FolderOpen className="h-4 w-4" />
      </Button>
    </div>
  );
}

export function AiSettingsSection() {
  const { t } = useI18n();
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);
  const [confirmRelaxedEgress, confirmRelaxedEgressDialog] = useConfirmDialog();

  const {
    settingsConfig,
    isConfigured,
    configError,
    loadConfig,
    loadProviders,
    saveProfile,
    deleteProfile,
    setActiveProfile,
  } = useAiStore();

  const [dialogOpen, setDialogOpen] = useState(false);
  const [editingProfile, setEditingProfile] = useState<AiModelProfile | null>(null);

  useEffect(() => {
    void loadProviders();
    void loadConfig();
  }, [loadProviders, loadConfig]);

  const profiles = settingsConfig?.profiles ?? [];
  const activeProfileId = settingsConfig?.activeProfileId ?? '';

  const handleAddModel = () => {
    setEditingProfile(null);
    setDialogOpen(true);
  };

  const handleEditModel = (profile: AiModelProfile) => {
    setEditingProfile(profile);
    setDialogOpen(true);
  };

  const handleDuplicateModel = async (profile: AiModelProfile) => {
    const duplicated: AiModelProfile = {
      ...profile,
      id: crypto.randomUUID(),
      name: `${profile.name || profile.model} (Copy)`,
      isDefault: false,
    };
    await saveProfile(duplicated);
  };

  const handleSetDefault = async (profile: AiModelProfile) => {
    await saveProfile({ ...profile, isDefault: true });
    await setActiveProfile(profile.id);
  };

  const handleDeleteModel = async (profileId: string) => {
    await deleteProfile(profileId);
  };

  return (
    <div className="space-y-6">
      <SectionTitle>{t('context.openDir')}</SectionTitle>
      <ContextDirSetting />

      <div className="flex items-center justify-between border-b border-edge/60 pb-3">
        <div>
          <SectionTitle>{t('settings.ai.modelsTitle')}</SectionTitle>
          <p className="mt-1 text-xs text-fg-muted">{t('settings.ai.description')}</p>
        </div>
        <div className="flex items-center gap-3">
          <span className="text-xs text-fg-muted">
            {isConfigured ? t('settings.ai.configured') : t('settings.ai.notConfigured')}
          </span>
          <Button
            variant="primary"
            size="sm"
            onClick={handleAddModel}
            className="flex items-center gap-1.5"
            {...tid('add-ai-model-button')}
          >
            <Plus className="h-4 w-4" />
            {t('settings.ai.addModel')}
          </Button>
        </div>
      </div>

      {configError && <p className="select-text text-xs text-red-500">{configError}</p>}

      {profiles.length === 0 ? (
        <div
          className="flex flex-col items-center justify-center rounded-xl border border-dashed border-edge/80 p-8 text-center bg-surface/50"
          {...tid('ai-models-empty')}
        >
          <Sparkles className="h-8 w-8 text-fg-muted mb-3" />
          <p className="text-sm text-fg-muted mb-4">{t('settings.ai.noModels')}</p>
          <Button variant="secondary" size="sm" onClick={handleAddModel}>
            <Plus className="h-4 w-4 mr-1.5" />
            {t('settings.ai.addModel')}
          </Button>
        </div>
      ) : (
        <div
          className="flex flex-col divide-y divide-edge rounded-xl border border-edge bg-surface overflow-hidden shadow-sm"
          {...tid('ai-models-list')}
        >
          {profiles.map((p) => (
            <ModelProfileListItem
              key={p.id}
              profile={p}
              isDefault={p.isDefault || p.id === activeProfileId}
              isActive={p.id === activeProfileId}
              onSetDefault={() => void handleSetDefault(p)}
              onEdit={() => handleEditModel(p)}
              onDuplicate={() => void handleDuplicateModel(p)}
              onDelete={() => void handleDeleteModel(p.id)}
            />
          ))}
        </div>
      )}

      {/* Global Safety Egress Toggle (legacy / global safeguard) */}
      <div className="border-t border-edge/60 pt-4">
        <ToggleRow
          label={t('settings.ai.strictEgress')}
          hint={t('settings.ai.strictEgressHint')}
          checked={settings.aiStrictEgress !== false}
          onChange={(enabled) => {
            void (async () => {
              if (!enabled && settings.aiStrictEgress) {
                const ok = await confirmRelaxedEgress({
                  title: t('settings.ai.strictEgressDisableTitle'),
                  message: t('settings.ai.strictEgressDisableMessage'),
                  kind: 'warning',
                });
                if (!ok) return;
              }
              await updateSettings({ ...settings, aiStrictEgress: enabled });
            })();
          }}
        />
      </div>

      {dialogOpen && (
        <ModelProfileDialog
          open={dialogOpen}
          onClose={() => setDialogOpen(false)}
          profile={editingProfile}
          onSave={async (profile) => {
            const ok = await saveProfile(profile);
            return ok;
          }}
        />
      )}

      {confirmRelaxedEgressDialog}
    </div>
  );
}
