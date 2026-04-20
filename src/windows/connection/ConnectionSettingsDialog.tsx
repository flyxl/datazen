import { useCallback, useEffect, useState } from 'react';
import { Dialog } from '../../components/ui/Dialog';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { useSettingsStore } from '../../stores/settingsStore';
import type { AppSettings } from '../../types';

interface ConnectionSettingsDialogProps {
  open: boolean;
  onClose: () => void;
}

const PAGE_SIZE_OPTIONS = [25, 50, 100, 200, 500];
const RESULT_LIMIT_OPTIONS = [1000, 2000, 5000, 10000, 50000];
const THEME_OPTIONS: { value: AppSettings['theme']; label: string }[] = [
  { value: 'light', label: '浅色主题' },
  { value: 'dark', label: '深色主题' },
  { value: 'system', label: '跟随系统' },
];

export function ConnectionSettingsDialog({ open, onClose }: ConnectionSettingsDialogProps) {
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  const [draft, setDraft] = useState<AppSettings>(settings);

  useEffect(() => {
    if (open) setDraft(settings);
  }, [open, settings]);

  const handleSave = useCallback(async () => {
    await updateSettings(draft);
    onClose();
  }, [draft, updateSettings, onClose]);

  const updateField = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
    setDraft((prev) => ({ ...prev, [key]: value }));
  };

  return (
    <Dialog
      open={open}
      title="连接设置"
      description="配置当前连接窗口的显示和行为"
      onClose={onClose}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>取消</Button>
          <Button variant="primary" onClick={() => void handleSave()}>保存</Button>
        </>
      }
    >
      <div className="space-y-5">
        {/* Theme */}
        <div>
          <label className="mb-1 block text-xs font-medium text-fg-secondary">主题</label>
          <Select
            value={draft.theme}
            options={THEME_OPTIONS}
            onChange={(v) => updateField('theme', v as AppSettings['theme'])}
          />
        </div>

        {/* Default page size */}
        <div>
          <label className="mb-1 block text-xs font-medium text-fg-secondary">默认每页行数</label>
          <Select
            value={draft.defaultPageSize}
            options={PAGE_SIZE_OPTIONS.map((v) => ({ value: String(v), label: `${v} 行` }))}
            onChange={(v) => updateField('defaultPageSize', Number(v))}
          />
        </div>

        {/* Limit SELECT results toggle */}
        <div className="flex items-center justify-between">
          <div>
            <label className="text-xs font-medium text-fg-secondary">限制 SELECT 结果行数</label>
            <p className="text-[11px] text-fg-muted">自动为无 LIMIT 的 SELECT 添加行数限制</p>
          </div>
          <input
            type="checkbox"
            checked={draft.limitSelectResults}
            onChange={(e) => updateField('limitSelectResults', e.target.checked)}
            className="h-4 w-4 accent-accent"
          />
        </div>

        {/* Query result limit */}
        {draft.limitSelectResults && (
          <div>
            <label className="mb-1 block text-xs font-medium text-fg-secondary">最大返回行数</label>
            <Select
              value={draft.queryResultLimit}
              options={RESULT_LIMIT_OPTIONS.map((v) => ({ value: String(v), label: `${v.toLocaleString()} 行` }))}
              onChange={(v) => updateField('queryResultLimit', Number(v))}
            />
          </div>
        )}

        {/* Editor font size */}
        <div>
          <label className="mb-1 block text-xs font-medium text-fg-secondary">编辑器字号</label>
          <div className="flex items-center gap-2">
            <input
              type="range"
              min={10}
              max={24}
              step={1}
              value={draft.editorFontSize}
              onChange={(e) => updateField('editorFontSize', Number(e.target.value))}
              className="flex-1 accent-accent"
            />
            <span className="w-10 text-right text-xs text-fg-secondary">{draft.editorFontSize}px</span>
          </div>
        </div>

        {/* Editor font family */}
        <div>
          <label className="mb-1 block text-xs font-medium text-fg-secondary">编辑器字体</label>
          <input
            type="text"
            value={draft.editorFontFamily}
            onChange={(e) => updateField('editorFontFamily', e.target.value)}
            className="h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-500/25"
          />
        </div>

        {/* Confirm on delete */}
        <div className="flex items-center justify-between">
          <div>
            <label className="text-xs font-medium text-fg-secondary">删除确认</label>
            <p className="text-[11px] text-fg-muted">删除行时弹出确认对话框</p>
          </div>
          <input
            type="checkbox"
            checked={draft.confirmOnDelete}
            onChange={(e) => updateField('confirmOnDelete', e.target.checked)}
            className="h-4 w-4 accent-accent"
          />
        </div>

        {/* Auto commit */}
        <div className="flex items-center justify-between">
          <div>
            <label className="text-xs font-medium text-fg-secondary">自动提交</label>
            <p className="text-[11px] text-fg-muted">编辑数据后自动提交更改</p>
          </div>
          <input
            type="checkbox"
            checked={draft.autoCommit}
            onChange={(e) => updateField('autoCommit', e.target.checked)}
            className="h-4 w-4 accent-accent"
          />
        </div>
      </div>
    </Dialog>
  );
}
