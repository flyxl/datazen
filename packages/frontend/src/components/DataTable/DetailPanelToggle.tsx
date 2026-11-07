import { PanelRightClose, PanelRightOpen } from 'lucide-react';
import { cn } from '../../lib/cn';
import { useI18n } from '../../hooks/useI18n';

export interface DetailPanelToggleProps {
  open: boolean;
  onToggle: () => void;
}

export function DetailPanelToggle({ open, onToggle }: DetailPanelToggleProps) {
  const { t } = useI18n();
  return (
    <button
      type="button"
      aria-pressed={open}
      onClick={onToggle}
      title={open ? t('detail.hide') : t('detail.show')}
      className={cn(
        'inline-flex h-7 w-7 items-center justify-center rounded-md transition-colors',
        open
          ? 'bg-accent/15 text-accent hover:bg-accent/25'
          : 'text-fg-muted hover:bg-surface-raised hover:text-fg',
      )}
    >
      {open ? <PanelRightClose className="h-4 w-4" /> : <PanelRightOpen className="h-4 w-4" />}
    </button>
  );
}
