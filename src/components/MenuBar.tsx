import { useState, useRef, useEffect, useCallback } from 'react';
import { createPortal } from 'react-dom';
import { cn } from '../lib/cn';
import { useI18n } from '../hooks/useI18n';
import { isProductFeatureEnabled } from '../lib/productFeatures';
import { useSettingsStore } from '../stores/settingsStore';
import { emitCrossWindow } from '../lib/crossWindowBus';
import { usePlatform } from '../hooks/usePlatform';
import { settingsCommands } from '../commands/settings';
import type { ThemeMode } from '../types/theme';

interface MenuItem {
  id: string;
  label: string;
  shortcut?: string;
  checked?: boolean;
  separator?: boolean;
  children?: MenuItem[];
}

interface Menu {
  id: string;
  label: string;
  items: MenuItem[];
}

function useMenus(): Menu[] {
  const { t } = useI18n();
  const theme = useSettingsStore((s) => s.settings.theme.mode);

  const toolsItems: MenuItem[] = [];
  if (isProductFeatureEnabled('schemaDiff')) {
    toolsItems.push({ id: 'schema-diff', label: t('common.schemaDiff') });
    toolsItems.push({ id: 'sep-3', label: '', separator: true });
  }
  toolsItems.push(
    { id: 'workflow', label: t('menu.workflow') },
    { id: 'dashboard', label: t('menu.dashboard') },
    { id: 'sep-3b', label: '', separator: true },
    { id: 'backup', label: t('common.backupDatabase') },
    { id: 'restore', label: t('common.restoreDatabase') },
    { id: 'sep-4', label: '', separator: true },
    { id: 'view-logs', label: t('common.viewLogs') },
  );

  return [
    {
      id: 'file',
      label: t('menu.file'),
      items: [
        { id: 'new-connection', label: t('common.newConnection'), shortcut: 'Ctrl+N' },
        { id: 'sep-f1', label: '', separator: true },
        { id: 'export-config', label: t('common.exportAppData') },
        { id: 'import-config', label: t('common.importAppData') },
        { id: 'export-connections', label: t('common.exportConnections') },
        {
          id: 'import-connections',
          label: t('common.importConnections'),
          children: [
            { id: 'import-connections-dbx', label: t('menu.importFromDbx') },
            { id: 'import-connections-navicat', label: t('menu.importFromNavicat') },
            { id: 'import-connections-datagrip', label: t('menu.importFromDataGrip') },
            { id: 'import-connections-dbeaver', label: t('menu.importFromDBeaver') },
            { id: 'import-connections-tableplus', label: t('menu.importFromTablePlus') },
            { id: 'sep-import', label: '', separator: true },
            { id: 'import-connections-file', label: t('menu.importFromFile') },
          ],
        },
        { id: 'sep-f2', label: '', separator: true },
        { id: 'open-settings', label: t('menu.settings'), shortcut: 'Ctrl+,' },
      ],
    },
    {
      id: 'view',
      label: t('menu.view'),
      items: [
        {
          id: 'theme-light',
          label: `${t('menu.theme')}: ${t('menu.themeLight')}`,
          checked: theme === 'light',
        },
        {
          id: 'theme-dark',
          label: `${t('menu.theme')}: ${t('menu.themeDark')}`,
          checked: theme === 'dark',
        },
        {
          id: 'theme-system',
          label: `${t('menu.theme')}: ${t('menu.themeSystem')}`,
          checked: theme === 'system',
        },
        { id: 'sep-2', label: '', separator: true },
        { id: 'fullscreen', label: t('menu.fullscreen') },
      ],
    },
    {
      id: 'tools',
      label: t('menu.tools'),
      items: toolsItems,
    },
    {
      id: 'help',
      label: t('menu.help'),
      items: [
        { id: 'help-docs', label: t('menu.documentation') },
        { id: 'sep-h1', label: '', separator: true },
        { id: 'help-report', label: t('menu.reportIssue') },
      ],
    },
  ];
}

async function handleMenuAction(id: string) {
  if (id.startsWith('theme-')) {
    const theme = id.replace('theme-', '') as ThemeMode;
    void emitCrossWindow('menu:theme-change', theme);
    return;
  }

  switch (id) {
    case 'fullscreen': {
      try {
        const { getCurrentWindow } = await import('@tauri-apps/api/window');
        const win = getCurrentWindow();
        const isFs = await win.isFullscreen();
        await win.setFullscreen(!isFs);
      } catch {
        // ignore
      }
      break;
    }
    case 'help-docs':
      void import('../lib/windowManager').then((m) => m.openDocsWindow());
      break;
    case 'help-report':
      void settingsCommands.openPath('https://github.com/flyxl/datazen/issues/new');
      break;
    default:
      void emitCrossWindow(`menu:${id}`);
  }
}

/**
 * Web-based menu bar for Windows/Linux where native menus are not available.
 * Renders in the title bar and mirrors macOS native menu structure
 * (Settings lives under File because there is no app menu on Windows).
 */
export function MenuBar() {
  const platform = usePlatform();
  const isMac = platform === 'macos';
  const [openMenu, setOpenMenu] = useState<string | null>(null);
  const barRef = useRef<HTMLDivElement>(null);
  const menus = useMenus();

  // Don't render on macOS (native menu bar is used)
  if (isMac) return null;

  return (
    <div ref={barRef} className="flex items-center gap-0.5">
      {menus.map((menu) => (
        <MenuButton
          key={menu.id}
          menu={menu}
          isOpen={openMenu === menu.id}
          onOpen={() => setOpenMenu(menu.id)}
          onClose={() => setOpenMenu(null)}
          onHover={() => {
            if (openMenu) setOpenMenu(menu.id);
          }}
          barRef={barRef}
        />
      ))}
    </div>
  );
}

interface MenuButtonProps {
  menu: Menu;
  isOpen: boolean;
  onOpen: () => void;
  onClose: () => void;
  onHover: () => void;
  barRef: React.RefObject<HTMLDivElement | null>;
}

function MenuButton({ menu, isOpen, onOpen, onClose, onHover }: MenuButtonProps) {
  const btnRef = useRef<HTMLButtonElement>(null);
  const dropRef = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);

  useEffect(() => {
    if (isOpen && btnRef.current) {
      const rect = btnRef.current.getBoundingClientRect();
      setPos({ x: rect.left, y: rect.bottom + 2 });
    } else {
      setPos(null);
    }
  }, [isOpen]);

  const handleClickOutside = useCallback(
    (e: MouseEvent) => {
      if (
        dropRef.current &&
        !dropRef.current.contains(e.target as Node) &&
        btnRef.current &&
        !btnRef.current.contains(e.target as Node)
      ) {
        onClose();
      }
    },
    [onClose],
  );

  useEffect(() => {
    if (!isOpen) return;
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [isOpen, handleClickOutside]);

  return (
    <>
      <button
        ref={btnRef}
        type="button"
        onClick={() => {
          if (isOpen) onClose();
          else onOpen();
        }}
        onMouseEnter={onHover}
        className={cn(
          'rounded px-2 py-0.5 text-xs transition-colors',
          isOpen
            ? 'bg-surface-raised text-fg'
            : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
        )}
      >
        {menu.label}
      </button>
      {isOpen &&
        pos &&
        createPortal(
          <div
            ref={dropRef}
            className="fixed z-[9999] min-w-[200px] rounded-lg border border-edge bg-surface-alt py-1 shadow-xl"
            style={{ left: pos.x, top: pos.y }}
          >
            {menu.items.map((item) => (
              <MenuEntry key={item.id} item={item} onClose={onClose} />
            ))}
          </div>,
          document.body,
        )}
    </>
  );
}

function MenuEntry({ item, onClose }: { item: MenuItem; onClose: () => void }) {
  const [open, setOpen] = useState(false);
  const rowRef = useRef<HTMLDivElement>(null);

  if (item.separator) {
    return <div className="my-1 h-px bg-edge" />;
  }

  if (item.children?.length) {
    return (
      <div
        ref={rowRef}
        className="relative"
        onMouseEnter={() => setOpen(true)}
        onMouseLeave={() => setOpen(false)}
      >
        <button
          type="button"
          className="flex w-full items-center justify-between px-3 py-1.5 text-left text-[13px] text-fg-secondary transition-colors hover:bg-surface-raised hover:text-fg"
        >
          <span>{item.label}</span>
          <span className="ml-4 text-[11px] text-fg-muted">▸</span>
        </button>
        {open && (
          <div className="absolute top-0 left-full z-[10000] min-w-[200px] rounded-lg border border-edge bg-surface-alt py-1 shadow-xl">
            {item.children.map((child) => (
              <MenuEntry key={child.id} item={child} onClose={onClose} />
            ))}
          </div>
        )}
      </div>
    );
  }

  return (
    <button
      type="button"
      className="flex w-full items-center justify-between px-3 py-1.5 text-left text-[13px] text-fg-secondary transition-colors hover:bg-surface-raised hover:text-fg"
      onClick={() => {
        void handleMenuAction(item.id);
        onClose();
      }}
    >
      <span className="flex items-center gap-2">
        {item.checked !== undefined && (
          <span className="w-4 text-center text-blue-500">{item.checked ? '✓' : ''}</span>
        )}
        {item.label}
      </span>
      {item.shortcut && <span className="ml-4 text-[11px] text-fg-muted">{item.shortcut}</span>}
    </button>
  );
}
