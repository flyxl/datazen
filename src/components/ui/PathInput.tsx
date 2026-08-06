import { useCallback, type KeyboardEventHandler } from 'react';
import { FolderOpen } from 'lucide-react';
import { open, type OpenDialogOptions } from '@tauri-apps/plugin-dialog';
import { Input } from './Input';
import { Button } from './Button';
import { cn } from '../../lib/cn';
import { useI18n } from '../../hooks/useI18n';

export interface PathInputProps {
  value: string;
  onChange: (path: string) => void;
  placeholder?: string;
  dialogOptions?: Partial<OpenDialogOptions>;
  disabled?: boolean;
  className?: string;
  error?: boolean;
  onKeyDown?: KeyboardEventHandler<HTMLInputElement>;
}

export function PathInput({
  value,
  onChange,
  placeholder,
  dialogOptions,
  disabled,
  className,
  error,
  onKeyDown,
}: PathInputProps) {
  const { t } = useI18n();

  const handleBrowse = useCallback(async () => {
    if (disabled) return;
    try {
      const selected = await open({
        multiple: false,
        ...dialogOptions,
      });
      if (typeof selected === 'string') {
        onChange(selected);
      }
    } catch {
      // Dialog cancelled or unavailable (e.g. browser dev mode)
    }
  }, [disabled, dialogOptions, onChange]);

  return (
    <div className={cn('flex w-full gap-1', className)}>
      <Input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        disabled={disabled}
        onKeyDown={onKeyDown}
        className={cn('min-w-0 flex-1', error && 'border-red-500')}
      />
      <Button
        variant="ghost"
        type="button"
        disabled={disabled}
        onClick={() => void handleBrowse()}
        className="h-9 w-9 shrink-0 px-0"
        title={t('common.browse')}
        aria-label={t('common.browse')}
      >
        <FolderOpen className="h-4 w-4" />
      </Button>
    </div>
  );
}
