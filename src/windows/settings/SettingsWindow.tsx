import { useCallback, useEffect, useState } from 'react';
import { TrafficLights } from '../../components/TrafficLights';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { useSettingsStore } from '../../stores/settingsStore';
import { useThemeListener } from '../../hooks/useThemeListener';
import type { AppSettings } from '../../types';

const PAGE_SIZE_OPTIONS = [25, 50, 100, 200, 500];
const RESULT_LIMIT_OPTIONS = [1000, 2000, 5000, 10000, 50000];
const THEME_OPTIONS: { value: AppSettings['theme']; label: string }[] = [
  { value: 'light', label: '浅色主题' },
  { value: 'dark', label: '深色主题' },
  { value: 'system', label: '跟随系统' },
];

export function SettingsWindow() {
  useThemeListener();

  const settings = useSettingsStore((s) => s.settings);
  const loadSettings = useSettingsStore((s) => s.loadSettings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  const [draft, setDraft] = useState<AppSettings>(settings);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  useEffect(() => {
    setDraft(settings);
  }, [settings]);

  const updateField = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
    setDraft((prev) => ({ ...prev, [key]: value }));
    setSaved(false);
  };

  const handleSave = useCallback(async () => {
    await updateSettings(draft);
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  }, [draft, updateSettings]);

  const handleClose = useCallback(async () => {
    if (!('__TAURI_INTERNALS__' in globalThis)) return;
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().close();
  }, []);

  const isDirty = JSON.stringify(draft) !== JSON.stringify(settings);

  return (
    <div className="flex h-screen flex-col bg-surface text-fg">
      {/* Title bar */}
      <header className="relative flex h-10 min-h-[40px] shrink-0 items-center bg-titlebar">
        <div className="absolute inset-0" data-tauri-drag-region />
        <div className="relative z-10 px-3">
          <TrafficLights />
        </div>
        <div className="pointer-events-none flex min-w-0 flex-1 justify-center">
          <span className="truncate text-xs font-medium text-fg-secondary">偏好设置 - DataZen</span>
        </div>
        <div className="w-[72px] shrink-0" />
      </header>

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-8 py-6">
        <h1 className="mb-6 text-lg font-semibold text-fg">偏好设置</h1>

        <div className="mx-auto max-w-lg space-y-6">
          {/* Theme */}
          <SettingRow label="主题">
            <Select
              value={draft.theme}
              options={THEME_OPTIONS}
              onChange={(v) => updateField('theme', v as AppSettings['theme'])}
            />
          </SettingRow>

          <Divider />

          {/* Data browsing section */}
          <SectionTitle>数据浏览</SectionTitle>

          <SettingRow label="默认每页行数">
            <Select
              value={draft.defaultPageSize}
              options={PAGE_SIZE_OPTIONS.map((v) => ({ value: String(v), label: `${v} 行` }))}
              onChange={(v) => updateField('defaultPageSize', Number(v))}
            />
          </SettingRow>

          <ToggleRow
            label="限制 SELECT 结果行数"
            description="开启后自动为无 LIMIT 的 SELECT 语句添加行数限制"
            checked={draft.limitSelectResults}
            onChange={(v) => updateField('limitSelectResults', v)}
          />

          {draft.limitSelectResults && (
            <SettingRow label="最大返回行数" hint="SELECT 查询的最大返回行数">
              <Select
                value={draft.queryResultLimit}
                options={RESULT_LIMIT_OPTIONS.map((v) => ({ value: String(v), label: `${v.toLocaleString()} 行` }))}
                onChange={(v) => updateField('queryResultLimit', Number(v))}
              />
            </SettingRow>
          )}

          <Divider />

          {/* Editor section */}
          <SectionTitle>编辑器</SectionTitle>

          <SettingRow label="字号">
            <div className="flex items-center gap-3">
              <input
                type="range"
                min={10}
                max={24}
                step={1}
                value={draft.editorFontSize}
                onChange={(e) => updateField('editorFontSize', Number(e.target.value))}
                className="flex-1 accent-accent"
              />
              <span className="w-12 text-right text-sm tabular-nums text-fg-secondary">{draft.editorFontSize}px</span>
            </div>
          </SettingRow>

          <SettingRow label="字体">
            <input
              type="text"
              value={draft.editorFontFamily}
              onChange={(e) => updateField('editorFontFamily', e.target.value)}
              className="h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-500/25"
            />
          </SettingRow>

          <Divider />

          {/* Behavior section */}
          <SectionTitle>行为</SectionTitle>

          <ToggleRow
            label="删除确认"
            description="删除行时弹出确认对话框"
            checked={draft.confirmOnDelete}
            onChange={(v) => updateField('confirmOnDelete', v)}
          />

          <ToggleRow
            label="自动提交"
            description="编辑数据后自动提交更改"
            checked={draft.autoCommit}
            onChange={(v) => updateField('autoCommit', v)}
          />
        </div>
      </div>

      {/* Footer */}
      <footer className="flex shrink-0 items-center justify-end gap-3 border-t border-edge px-8 py-3">
        {saved && <span className="text-xs text-green-500">已保存</span>}
        <Button variant="secondary" onClick={() => void handleClose()}>关闭</Button>
        <Button variant="primary" disabled={!isDirty} onClick={() => void handleSave()}>
          保存
        </Button>
      </footer>
    </div>
  );
}

function SectionTitle({ children }: { children: React.ReactNode }) {
  return <h2 className="text-[13px] font-semibold uppercase tracking-wider text-fg-muted">{children}</h2>;
}

function Divider() {
  return <div className="h-px bg-edge" />;
}

function SettingRow({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="flex items-start gap-4">
      <div className="w-32 shrink-0 pt-2">
        <div className="text-sm text-fg-secondary">{label}</div>
        {hint && <div className="mt-0.5 text-[11px] text-fg-muted">{hint}</div>}
      </div>
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}

function ToggleRow({ label, description, checked, onChange }: {
  label: string;
  description: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between">
      <div>
        <div className="text-sm text-fg-secondary">{label}</div>
        <div className="text-[11px] text-fg-muted">{description}</div>
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        onClick={() => onChange(!checked)}
        className={`relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full transition-colors ${
          checked ? 'bg-accent' : 'bg-edge'
        }`}
      >
        <span
          className={`inline-block h-3.5 w-3.5 rounded-full bg-white shadow transition-transform ${
            checked ? 'translate-x-[18px]' : 'translate-x-[3px]'
          }`}
        />
      </button>
    </div>
  );
}
