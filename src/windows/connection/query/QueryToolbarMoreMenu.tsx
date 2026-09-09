import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from 'react';
import { createPortal } from 'react-dom';
import { MoreHorizontal, FileSearch, Wand2, RefreshCw, CirclePlay } from 'lucide-react';
import { ToolbarButton } from '../../../components/ui/ToolbarButton';
import { useI18n } from '../../../hooks/useI18n';
import { usePlatform } from '../../../hooks/usePlatform';
import { cn } from '../../../lib/cn';

export interface QueryToolbarMoreMenuProps {
  compact?: boolean;
  disabled?: boolean;
  supportsExplain?: boolean;
  explainDisabled?: boolean;
  formatDisabled?: boolean;
  refreshCompletionDisabled?: boolean;
  inTransaction?: boolean;
  txBusy?: boolean;
  onFormat: () => void;
  onExplain: () => void;
  onBeginTx: () => void;
  onCommitTx: () => void;
  onRollbackTx: () => void;
  onRefreshCompletion?: () => void;
  renderSnippetButton?: () => ReactNode;
}

interface MenuItemProps {
  testId: string;
  label: string;
  shortcut?: string;
  icon: ReactNode;
  disabled?: boolean;
  onClick: () => void;
}

function MenuItem({ testId, label, shortcut, icon, disabled, onClick }: MenuItemProps) {
  return (
    <button
      type="button"
      data-testid={testId}
      disabled={disabled}
      className={cn(
        'flex w-full items-center gap-2 px-2.5 py-1.5 text-left transition-colors',
        disabled ? 'cursor-not-allowed opacity-40' : 'cursor-pointer hover:bg-surface-overlay',
      )}
      onClick={onClick}
    >
      <span className="shrink-0 text-fg-muted">{icon}</span>
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {shortcut && <span className="shrink-0 text-fg-muted">{shortcut}</span>}
    </button>
  );
}

export function QueryToolbarMoreMenu({
  compact = false,
  disabled = false,
  supportsExplain = false,
  explainDisabled = false,
  formatDisabled = false,
  refreshCompletionDisabled = false,
  inTransaction = false,
  txBusy = false,
  onFormat,
  onExplain,
  onBeginTx,
  onCommitTx: _onCommitTx,
  onRollbackTx: _onRollbackTx,
  onRefreshCompletion,
  renderSnippetButton,
}: QueryToolbarMoreMenuProps) {
  const { t } = useI18n();
  const platform = usePlatform();
  const isMac = platform === 'macos' || (platform as string) === 'ios';

  const [open, setOpen] = useState(false);
  const [menuPos, setMenuPos] = useState<{ x: number; y: number }>({ x: 0, y: 0 });
  const containerRef = useRef<HTMLDivElement>(null);

  const close = useCallback(() => setOpen(false), []);

  const runAction = useCallback(
    (action: () => void) => {
      action();
      close();
    },
    [close],
  );

  useEffect(() => {
    if (!open) return;

    const handleMouseDown = (event: MouseEvent) => {
      const t = event.target as Node | null;
      if (containerRef.current?.contains(t)) return;
      if (
        (event.target as HTMLElement)?.closest('[data-testid="query-toolbar-more-menu-dropdown"]')
      )
        return;
      setOpen(false);
    };

    document.addEventListener('mousedown', handleMouseDown);
    return () => document.removeEventListener('mousedown', handleMouseDown);
  }, [open]);

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'Escape') {
      event.preventDefault();
      setOpen(false);
      containerRef.current?.querySelector<HTMLButtonElement>('button')?.focus();
    }
  };

  const handleToggle = useCallback(() => {
    setOpen((prev) => {
      if (!prev && containerRef.current) {
        const rect = containerRef.current.getBoundingClientRect();
        setMenuPos({ x: rect.left, y: rect.bottom });
      }
      return !prev;
    });
  }, []);

  const formatShortcut = isMac ? '⇧⌥F' : 'Shift+Alt+F';

  return (
    <div
      ref={containerRef}
      className="relative inline-flex shrink-0 items-center"
      onKeyDown={handleKeyDown}
    >
      <ToolbarButton
        compact={compact}
        variant="ghost"
        label={t('dataTable.moreActions')}
        title={t('dataTable.moreActions')}
        icon={<MoreHorizontal className="h-3.5 w-3.5" />}
        disabled={disabled}
        aria-expanded={open}
        aria-haspopup="menu"
        data-testid="query-toolbar-more-menu-trigger"
        onClick={handleToggle}
      />

      {open &&
        createPortal(
          <div
            role="menu"
            data-testid="query-toolbar-more-menu-dropdown"
            className="fixed z-[10000] min-w-[200px] overflow-hidden rounded-lg border border-edge bg-surface-alt py-1 text-xs text-fg shadow-xl"
            style={{ left: menuPos.x, top: menuPos.y }}
          >
            <MenuItem
              testId="more-menu-format"
              label={t('query.format')}
              shortcut={formatShortcut}
              icon={<Wand2 className="h-3.5 w-3.5" />}
              disabled={formatDisabled}
              onClick={() => runAction(onFormat)}
            />

            {supportsExplain && (
              <MenuItem
                testId="more-menu-explain"
                label={t('explain.title')}
                icon={<FileSearch className="h-3.5 w-3.5" />}
                disabled={explainDisabled}
                onClick={() => runAction(onExplain)}
              />
            )}

            {renderSnippetButton && (
              <div className="border-t border-edge/50 px-1 py-0.5">{renderSnippetButton()}</div>
            )}

            {onRefreshCompletion && (
              <MenuItem
                testId="more-menu-refresh-completion"
                label={t('query.refreshCompletion')}
                icon={<RefreshCw className="h-3.5 w-3.5" />}
                disabled={refreshCompletionDisabled}
                onClick={() => runAction(onRefreshCompletion)}
              />
            )}

            <div className="my-0.5 border-t border-edge/50" />

            {!inTransaction && (
              <MenuItem
                testId="more-menu-begin-tx"
                label={t('query.beginTx')}
                icon={<CirclePlay className="h-3.5 w-3.5" />}
                disabled={txBusy}
                onClick={() => runAction(onBeginTx)}
              />
            )}
          </div>,
          document.body,
        )}
    </div>
  );
}
