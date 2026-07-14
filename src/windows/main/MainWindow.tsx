import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  ChevronDown,
  ChevronRight,
  Plus,
  Search,
} from 'lucide-react';
import { Menu, MenuItem, Submenu, PredefinedMenuItem } from '@tauri-apps/api/menu';
import { StatusBar } from '../../components/StatusBar';
import { Dialog } from '../../components/ui/Dialog';
import { Input } from '../../components/ui/Input';
import { Button } from '../../components/ui/Button';
import { TitleBar } from '../../components/TitleBar';
import { useKeyboardShortcuts } from '../../hooks/useKeyboardShortcuts';
import { useResizable } from '../../hooks/useResizable';
import { useTauriEvent } from '../../hooks/useTauriEvent';
import { useThemeListener } from '../../hooks/useThemeListener';
import { groupConnections, useConnectionStore } from '../../stores/connectionStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useUiStore } from '../../stores/uiStore';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import { cn } from '../../lib/cn';
import { listenCrossWindow } from '../../lib/crossWindowBus';
import { openBackupWindow, openConnectionWindow, openDataSyncWindow, openNewConnectionWindow, openSettingsWindow } from '../../lib/windowManager';
import { ThemeToggle } from '../../components/ThemeToggle';
import { useI18n } from '../../hooks/useI18n';
import { ActionPanel } from './ActionPanel';
import { ConnectionItem } from './ConnectionItem';
import { ImportConfigDialog } from './ImportConfigDialog';
import { connectionCommands } from '../../commands/connection';
import type { ConnectionConfig } from '../../types';

// ─── Main Window ────────────────────────────────────────────────────

export function MainWindow() {
  useTauriEvent();
  useThemeListener();
  const { t } = useI18n();

  const fetchConnections = useConnectionStore((s) => s.fetchConnections);
  const fetchGroups = useConnectionStore((s) => s.fetchGroups);
  const connections = useConnectionStore((s) => s.connections);
  const groups = useConnectionStore((s) => s.groups);
  const searchQuery = useConnectionStore((s) => s.searchQuery);
  const setSearchQuery = useConnectionStore((s) => s.setSearchQuery);
  const addGroup = useConnectionStore((s) => s.addGroup);
  const renameGroup = useConnectionStore((s) => s.renameGroup);
  const deleteGroup = useConnectionStore((s) => s.deleteGroup);
  const duplicateConnection = useConnectionStore((s) => s.duplicateConnection);
  const deleteConnection = useConnectionStore((s) => s.deleteConnection);
  const moveConnectionToGroup = useConnectionStore((s) => s.moveConnectionToGroup);
  const loading = useConnectionStore((s) => s.loading);
  const error = useConnectionStore((s) => s.error);

  const loadSettings = useSettingsStore((s) => s.loadSettings);

  const mainSidebarWidth = useUiStore((s) => s.mainSidebarWidth);
  const setMainSidebarWidth = useUiStore((s) => s.setMainSidebarWidth);

  const connectAction = useActiveConnectionStore((s) => s.connect);
  const disconnectAction = useActiveConnectionStore((s) => s.disconnect);
  const activeConnections = useActiveConnectionStore((s) => s.connections);

  const [errorDialogOpen, setErrorDialogOpen] = useState(false);
  const [errorMessage, setErrorMessage] = useState('');
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());

  // Inline rename state
  const [renamingGroup, setRenamingGroup] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState('');

  // New group dialog state
  const [newGroupDialogOpen, setNewGroupDialogOpen] = useState(false);
  const [newGroupName, setNewGroupName] = useState('');

  // Import config dialog state
  const [importDialogOpen, setImportDialogOpen] = useState(false);
  const [importData, setImportData] = useState<{ connections: ConnectionConfig[]; groups: string[] } | null>(null);

  // Export password dialog state
  const [exportPasswordDialogOpen, setExportPasswordDialogOpen] = useState(false);
  const [exportPassword, setExportPassword] = useState('');
  const [exportPasswordConfirm, setExportPasswordConfirm] = useState('');
  const [exportPasswordError, setExportPasswordError] = useState('');
  const [pendingExportPath, setPendingExportPath] = useState<string | null>(null);

  // Import password dialog state
  const [importPasswordDialogOpen, setImportPasswordDialogOpen] = useState(false);
  const [importPassword, setImportPassword] = useState('');
  const [importPasswordError, setImportPasswordError] = useState('');
  const [pendingImportPath, setPendingImportPath] = useState<string | null>(null);

  // ── Pointer-based drag state ──
  const [draggingConnId, setDraggingConnId] = useState<string | null>(null);
  const [dragOverGroup, setDragOverGroup] = useState<string | null>(null);
  const [dragGhostPos, setDragGhostPos] = useState<{ x: number; y: number } | null>(null);
  const dragStartPos = useRef<{ x: number; y: number } | null>(null);
  const dragConnRef = useRef<ConnectionConfig | null>(null);
  const dragActiveRef = useRef(false);
  const groupRectsRef = useRef<Map<string, DOMRect>>(new Map());

  const lastError = useMemo(() => {
    for (const entry of Object.values(activeConnections)) {
      if (entry.status === 'error' && entry.error) return entry.error;
    }
    return null;
  }, [activeConnections]);

  useEffect(() => {
    if (lastError) {
      setErrorMessage(lastError);
      setErrorDialogOpen(true);
    }
  }, [lastError]);

  // ── Init ──
  useEffect(() => {
    void fetchConnections();
    void fetchGroups();
    void loadSettings();
  }, [fetchConnections, fetchGroups, loadSettings]);

  const prevGroupsRef = useRef<string[] | null>(null);
  useEffect(() => {
    const prev = prevGroupsRef.current;
    prevGroupsRef.current = groups;
    if (!prev) {
      // First load: expand all groups + ungrouped
      setExpandedGroups(new Set([...groups, '']));
      return;
    }
    const newGroups = groups.filter((g) => !prev.includes(g));
    if (newGroups.length === 0) return;
    setExpandedGroups((s) => {
      const next = new Set(s);
      for (const g of newGroups) next.add(g);
      next.add('');
      return next;
    });
  }, [groups]);

  // ── Cross-window events ──
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listenCrossWindow('datazen:connection-closed', (payload) => {
      const data = payload as { connectionId?: string } | undefined;
      if (data?.connectionId) {
        useActiveConnectionStore.getState().removeByConnectionId(data.connectionId);
      }
    }).then((fn) => { unlisten = fn; });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    let cancelled = false;
    const cleanups: (() => void)[] = [];
    void listenCrossWindow('menu:open-settings', () => {
      if (!cancelled) openSettingsWindow();
    }).then((u) => { if (cancelled) u(); else cleanups.push(u); });
    void listenCrossWindow('menu:new-connection', () => {
      if (!cancelled) openNewConnectionWindow();
    }).then((u) => { if (cancelled) u(); else cleanups.push(u); });
    void listenCrossWindow('menu:data-sync', () => {
      if (!cancelled) openDataSyncWindow();
    }).then((u) => { if (cancelled) u(); else cleanups.push(u); });
    return () => { cancelled = true; cleanups.forEach((fn) => fn()); };
  }, []);

  // (native context menus handle their own dismiss)

  const handleConnect = useCallback(async (cfg: ConnectionConfig) => {
    const existing = useActiveConnectionStore.getState().connections[cfg.id];
    if (existing?.status === 'connected' && existing.connectionId) {
      openConnectionWindow(existing.connectionId, cfg.name, cfg.database, cfg.databaseType);
      return;
    }
    await connectAction(cfg);
    const entry = useActiveConnectionStore.getState().connections[cfg.id];
    if (entry?.status === 'connected' && entry.connectionId) {
      openConnectionWindow(entry.connectionId, cfg.name, cfg.database, cfg.databaseType);
    }
  }, [connectAction]);

  const { size: sidebarWidth, handleRef } = useResizable({
    direction: 'horizontal',
    initialSize: mainSidebarWidth,
    minSize: 160,
    maxSize: 320,
    storageKey: 'main.sidebar',
  });

  useEffect(() => {
    setMainSidebarWidth(sidebarWidth);
  }, [setMainSidebarWidth, sidebarWidth]);

  // ── Grouped connections ──
  const grouped = useMemo(
    () => groupConnections(connections, groups, searchQuery),
    [connections, groups, searchQuery],
  );

  // ── Keyboard shortcuts ──
  useKeyboardShortcuts([
    { key: 'mod+n', scope: 'global', description: 'New Connection', action: () => openNewConnectionWindow() },
  ]);

  // ── Status ──
  const activeCount = useMemo(
    () => Object.values(activeConnections).filter((e) => e.status === 'connected').length,
    [activeConnections],
  );

  const statusLeft = (() => {
    if (loading) return t('common.loading');
    if (error) return <span className="text-red-400">{error}</span>;
    if (activeCount > 0) return <span className="text-green-400">{t('main.activeConnections', { count: activeCount })}</span>;
    return t('main.ready');
  })();

  // ── Helpers ──

  const toggleGroup = useCallback((group: string) => {
    setExpandedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(group)) next.delete(group);
      else next.add(group);
      return next;
    });
  }, []);

  // ── Native context menus (Tauri Menu API) ──

  const handleGroupContextMenu = useCallback(async (e: React.MouseEvent, groupName: string) => {
    e.preventDefault();
    e.stopPropagation();
    const isUngrouped = groupName === '';

    const items: Array<MenuItem | PredefinedMenuItem> = [
      await MenuItem.new({ text: t('main.ctx.newGroup'), action: () => { setNewGroupName(''); setNewGroupDialogOpen(true); } }),
    ];
    if (!isUngrouped) {
      items.push(
        await PredefinedMenuItem.new({ item: 'Separator' }),
        await MenuItem.new({ text: t('main.ctx.renameGroup'), action: () => { setRenamingGroup(groupName); setRenameValue(groupName); } }),
        await MenuItem.new({ text: t('main.ctx.deleteGroup'), action: () => { void deleteGroup(groupName); } }),
      );
    }
    const menu = await Menu.new({ items });
    await menu.popup();
  }, [deleteGroup]);

  const handleConnectionContextMenu = useCallback(async (e: React.MouseEvent, conn: ConnectionConfig) => {
    e.preventDefault();
    e.stopPropagation();
    setSelectedId(conn.id);

    const isConnected = activeConnections[conn.id]?.status === 'connected';
    const items: Array<MenuItem | Submenu | PredefinedMenuItem> = [
      await MenuItem.new({
        text: isConnected ? t('main.ctx.disconnect') : t('main.ctx.openConnection'),
        action: () => { if (isConnected) void disconnectAction(conn.id); else void handleConnect(conn); },
      }),
      await PredefinedMenuItem.new({ item: 'Separator' }),
      await MenuItem.new({ text: t('main.ctx.editConnection'), action: () => openNewConnectionWindow(conn.id) }),
      await MenuItem.new({ text: t('main.ctx.duplicateConnection'), action: () => { void duplicateConnection(conn.id); } }),
      await PredefinedMenuItem.new({ item: 'Separator' }),
    ];

    const moveTargets = groups.filter((g) => g !== conn.group);
    if (moveTargets.length > 0 || conn.group) {
      const subItems: MenuItem[] = [];
      for (const g of moveTargets) {
        subItems.push(await MenuItem.new({ text: g, action: () => { void moveConnectionToGroup(conn.id, g); } }));
      }
      if (conn.group) {
        subItems.push(await MenuItem.new({ text: t('main.ctx.removeFromGroup'), action: () => { void moveConnectionToGroup(conn.id, undefined); } }));
      }
      items.push(await Submenu.new({ text: t('main.ctx.moveToGroup'), items: subItems }));
      items.push(await PredefinedMenuItem.new({ item: 'Separator' }));
    }

    items.push(await MenuItem.new({ text: t('main.ctx.deleteConnection'), action: () => { void deleteConnection(conn.id); } }));
    const menu = await Menu.new({ items });
    await menu.popup();
  }, [activeConnections, groups, disconnectAction, handleConnect, duplicateConnection, deleteConnection, moveConnectionToGroup]);

  // ── Pointer-based drag & drop ──

  const snapshotGroupRects = useCallback(() => {
    const map = new Map<string, DOMRect>();
    document.querySelectorAll<HTMLElement>('[data-group-name]').forEach((el) => {
      const name = el.dataset.groupName ?? '';
      map.set(name, el.getBoundingClientRect());
    });
    groupRectsRef.current = map;
  }, []);

  const hitTestGroup = useCallback((x: number, y: number): string | null => {
    for (const [name, rect] of groupRectsRef.current) {
      if (x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom) {
        return name;
      }
    }
    return null;
  }, []);

  const handlePointerDown = useCallback((e: React.PointerEvent, conn: ConnectionConfig) => {
    if (e.button !== 0) return;
    dragStartPos.current = { x: e.clientX, y: e.clientY };
    dragConnRef.current = conn;
    dragActiveRef.current = false;
  }, []);

  useEffect(() => {
    const onMove = (e: PointerEvent) => {
      if (!dragStartPos.current || !dragConnRef.current) return;

      if (!dragActiveRef.current) {
        const dx = e.clientX - dragStartPos.current.x;
        const dy = e.clientY - dragStartPos.current.y;
        if (Math.abs(dx) < 5 && Math.abs(dy) < 5) return;
        dragActiveRef.current = true;
        snapshotGroupRects();
        setDraggingConnId(dragConnRef.current.id);
      }

      setDragGhostPos({ x: e.clientX, y: e.clientY });

      const overGroup = hitTestGroup(e.clientX, e.clientY);
      setDragOverGroup(overGroup);
    };

    const onUp = (e: PointerEvent) => {
      if (!dragConnRef.current) return;

      if (dragActiveRef.current) {
        const targetGroup = hitTestGroup(e.clientX, e.clientY);
        const conn = dragConnRef.current;
        if (targetGroup !== null && targetGroup !== (conn.group || '')) {
          void moveConnectionToGroup(conn.id, targetGroup || undefined);
        }
      }

      dragStartPos.current = null;
      dragConnRef.current = null;
      dragActiveRef.current = false;
      setDraggingConnId(null);
      setDragOverGroup(null);
      setDragGhostPos(null);
    };

    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    return () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
    };
  }, [snapshotGroupRects, hitTestGroup, moveConnectionToGroup]);

  // ── Rename submit ──

  const submitRename = useCallback(() => {
    if (renamingGroup !== null && renameValue.trim()) {
      void renameGroup(renamingGroup, renameValue.trim());
    }
    setRenamingGroup(null);
  }, [renamingGroup, renameValue, renameGroup]);

  // ── Export / Import config handlers ──

  const handleExportConfig = useCallback(async () => {
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const path = await save({
        title: t('action.exportConfig'),
        defaultPath: `datazen-config-${new Date().toISOString().slice(0, 10)}.json`,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!path) return;
      setPendingExportPath(path);
      setExportPassword('');
      setExportPasswordConfirm('');
      setExportPasswordError('');
      setExportPasswordDialogOpen(true);
    } catch (e) {
      setErrorMessage(e instanceof Error ? e.message : String(e));
      setErrorDialogOpen(true);
    }
  }, [t]);

  const submitExportPassword = useCallback(async () => {
    if (!exportPassword) {
      setExportPasswordError(t('configExport.passwordRequired'));
      return;
    }
    if (exportPassword !== exportPasswordConfirm) {
      setExportPasswordError(t('configExport.passwordMismatch'));
      return;
    }
    if (!pendingExportPath) return;
    setExportPasswordDialogOpen(false);
    try {
      const count = await connectionCommands.exportConnections(pendingExportPath, exportPassword);
      setErrorMessage(t('configExport.success', { count }));
      setErrorDialogOpen(true);
    } catch (e) {
      setErrorMessage(e instanceof Error ? e.message : String(e));
      setErrorDialogOpen(true);
    } finally {
      setPendingExportPath(null);
      setExportPassword('');
      setExportPasswordConfirm('');
    }
  }, [exportPassword, exportPasswordConfirm, pendingExportPath, t]);

  const handleImportConfig = useCallback(async () => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const path = await open({
        title: t('action.importConfig'),
        filters: [{ name: 'JSON', extensions: ['json'] }],
        multiple: false,
      });
      if (!path) return;
      const filePath = typeof path === 'string' ? path : (path as unknown as string);
      setPendingImportPath(filePath);
      setImportPassword('');
      setImportPasswordError('');
      setImportPasswordDialogOpen(true);
    } catch (e) {
      setErrorMessage(e instanceof Error ? e.message : String(e));
      setErrorDialogOpen(true);
    }
  }, [t]);

  const submitImportPassword = useCallback(async () => {
    if (!pendingImportPath) return;
    try {
      const data = await connectionCommands.importConnectionsPreview(pendingImportPath, importPassword);
      if (!data.connections || !Array.isArray(data.connections)) {
        setImportPasswordDialogOpen(false);
        setErrorMessage(t('configImport.invalidFile'));
        setErrorDialogOpen(true);
        return;
      }
      setImportPasswordDialogOpen(false);

      const existingById = new Map(connections.map((c) => [c.id, c]));
      const existingByName = new Map(connections.map((c) => [c.name, c]));
      const hasConflict = data.connections.some((inc) =>
        existingById.has(inc.id) || existingByName.has(inc.name),
      );

      if (hasConflict) {
        setImportData(data as { connections: ConnectionConfig[]; groups: string[] });
        setImportDialogOpen(true);
      } else {
        let imported = 0;
        for (const conn of data.connections) {
          await connectionCommands.saveConnection(conn);
          imported++;
        }
        if (data.groups?.length) {
          const existingGroups = await connectionCommands.getGroups();
          const allGroups = [...new Set([...existingGroups, ...data.groups])];
          await connectionCommands.saveGroups(allGroups);
        }
        await fetchConnections();
        await fetchGroups();
        setErrorMessage(t('configImport.success', { count: imported }));
        setErrorDialogOpen(true);
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      if (msg.includes('wrong password') || msg.includes('Decryption failed')) {
        setImportPasswordError(t('configImport.wrongPassword'));
      } else {
        setImportPasswordDialogOpen(false);
        setErrorMessage(msg);
        setErrorDialogOpen(true);
      }
    }
  }, [pendingImportPath, importPassword, connections, fetchConnections, fetchGroups, t]);

  const handleImportResult = useCallback((message: string) => {
    setErrorMessage(message);
    setErrorDialogOpen(true);
  }, []);

  // ── Menu bar events for export/import ──
  useEffect(() => {
    let cancelled = false;
    const cleanups: (() => void)[] = [];
    void listenCrossWindow('menu:export-config', () => {
      if (!cancelled) void handleExportConfig();
    }).then((u) => { if (cancelled) u(); else cleanups.push(u); });
    void listenCrossWindow('menu:import-config', () => {
      if (!cancelled) void handleImportConfig();
    }).then((u) => { if (cancelled) u(); else cleanups.push(u); });
    return () => { cancelled = true; cleanups.forEach((fn) => fn()); };
  }, [handleExportConfig, handleImportConfig]);

  // ── Backup / Restore handlers ──

  const handleRestore = useCallback(async () => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const path = await open({
        title: t('action.restore'),
        filters: [{ name: 'SQL Files', extensions: ['sql'] }],
        multiple: false,
      });
      if (!path) return;

      if (!selectedId) {
        setErrorMessage(t('main.restoreFailed'));
        setErrorDialogOpen(true);
        return;
      }
      const conn = connections.find((c) => c.id === selectedId);
      if (!conn) return;

      const entry = activeConnections[conn.id];
      if (entry?.status !== 'connected' || !entry.connectionId) {
        setErrorMessage(t('main.restoreFailed'));
        setErrorDialogOpen(true);
        return;
      }

      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('restore_database', {
        connectionId: entry.connectionId,
        inputPath: typeof path === 'string' ? path : (path as unknown as string),
      });
      setErrorMessage(t('main.restoreSuccess'));
      setErrorDialogOpen(true);
    } catch (e) {
      setErrorMessage(e instanceof Error ? e.message : String(e));
      setErrorDialogOpen(true);
    }
  }, [selectedId, connections, activeConnections]);

  // ── Blank area context menu ──

  const handleBlankContextMenu = useCallback(async (e: React.MouseEvent) => {
    const el = e.target as HTMLElement;
    if (el.closest('[data-group-header]') || el.closest('[data-conn-item]')) return;
    e.preventDefault();
    const menu = await Menu.new({
      items: [
        await MenuItem.new({ text: t('main.ctx.newGroup'), action: () => { setNewGroupName(''); setNewGroupDialogOpen(true); } }),
        await MenuItem.new({ text: t('main.newConnection'), action: () => openNewConnectionWindow() }),
      ],
    });
    await menu.popup();
  }, []);

  return (
    <div className="flex h-screen min-h-0 min-w-[520px] flex-col bg-surface text-fg">
      {/* ── Title bar ── */}
      <TitleBar title="DataZen" rightContent={<ThemeToggle />} />

      {/* ── Body ── */}
      <div className="flex min-h-0 flex-1">
        {/* ── Left action panel ── */}
        <aside
          style={{ width: sidebarWidth }}
          className="shrink-0 border-r border-edge bg-surface-alt"
        >
          <ActionPanel
            onNewConnection={() => openNewConnectionWindow()}
            onBackup={() => openBackupWindow()}
            onRestore={() => void handleRestore()}
            onDataSync={() => openDataSyncWindow()}
          />
        </aside>
        <div
          ref={handleRef}
          className="w-1 shrink-0 cursor-col-resize bg-transparent hover:bg-blue-500/30"
          title={t('main.sidebar.resize')}
        />

        {/* ── Main content ── */}
        <main className="flex min-w-0 flex-1 flex-col">
          {/* ── Search bar ── */}
          <div className="flex shrink-0 items-center gap-2 border-b border-edge px-4 py-2">
            <button
              type="button"
              onClick={() => openNewConnectionWindow()}
              className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg"
              title={t('main.newConnection')}
            >
              <Plus className="h-4 w-4" />
            </button>
            <div className="relative min-w-0 flex-1">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-fg-muted" />
              <Input
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder={t('main.searchPlaceholder')}
                className="h-8 pl-8 text-[13px]"
              />
            </div>
          </div>

          {/* ── Grouped connection list ── */}
          <div
            className="flex-1 overflow-auto"
            onContextMenu={handleBlankContextMenu}
          >
            {grouped.length === 0 && (
              <div className="flex flex-col items-center justify-center py-20 text-sm text-fg-muted">
                <p>{t('main.noConnections')}</p>
                <Button
                  variant="ghost"
                  className="mt-3"
                  onClick={() => openNewConnectionWindow()}
                >
                  <Plus className="h-4 w-4" />
                  {t('main.createFirst')}
                </Button>
              </div>
            )}
            {grouped.map(({ group: groupName, connections: groupConns }) => {
              const expanded = expandedGroups.has(groupName);
              const displayName = groupName || t('main.ungrouped');
              const isDragOver = dragOverGroup === groupName;

              return (
                <div
                  key={groupName}
                  data-group-name={groupName}
                  className={cn(
                    'transition-colors',
                    isDragOver && draggingConnId && 'bg-blue-500/10 ring-1 ring-inset ring-blue-500/30',
                  )}
                >
                  {/* ── Group header ── */}
                  <div
                    data-group-header
                    className="flex cursor-pointer select-none items-center gap-1.5 px-3 py-2 hover:bg-surface-raised/50"
                    onClick={() => toggleGroup(groupName)}
                    onContextMenu={(e) => { void handleGroupContextMenu(e, groupName); }}
                  >
                    {expanded ? (
                      <ChevronDown className="h-3 w-3 shrink-0 text-fg-muted" />
                    ) : (
                      <ChevronRight className="h-3 w-3 shrink-0 text-fg-muted" />
                    )}
                    {renamingGroup === groupName ? (
                      <Input
                        autoFocus
                        value={renameValue}
                        onChange={(e) => setRenameValue(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') submitRename();
                          if (e.key === 'Escape') setRenamingGroup(null);
                        }}
                        onBlur={submitRename}
                        className="h-6 flex-1 text-xs"
                        onClick={(e) => e.stopPropagation()}
                      />
                    ) : (
                      <>
                        <span className="text-[13px] font-semibold text-fg">{displayName}</span>
                        <span className="text-[11px] text-fg-muted">({groupConns.length})</span>
                      </>
                    )}
                  </div>

                  {/* ── Connections ── */}
                  {expanded && (
                    <div className="px-2 pb-2">
                      {groupConns.map((conn) => (
                        <ConnectionItem
                          key={conn.id}
                          connection={conn}
                          status={activeConnections[conn.id]?.status ?? 'idle'}
                          selected={selectedId === conn.id}
                          isDragging={draggingConnId === conn.id}
                          onSelect={setSelectedId}
                          onConnect={handleConnect}
                          onContextMenu={handleConnectionContextMenu}
                          onPointerDown={handlePointerDown}
                        />
                      ))}
                    </div>
                  )}
                </div>
              );
            })}
          </div>

          {/* ── Drag ghost overlay ── */}
          {draggingConnId && dragGhostPos && (() => {
            const conn = connections.find((c) => c.id === draggingConnId);
            if (!conn) return null;
            return (
              <div
                className="pointer-events-none fixed z-[9999] rounded-lg border border-blue-500/40 bg-surface-alt px-3 py-2 text-[13px] font-medium text-fg shadow-xl"
                style={{ left: dragGhostPos.x + 12, top: dragGhostPos.y + 12 }}
              >
                {conn.name}
              </div>
            );
          })()}
        </main>
      </div>

      {/* ── New group dialog ── */}
      <Dialog
        open={newGroupDialogOpen}
        title={t('main.newGroupTitle')}
        onClose={() => setNewGroupDialogOpen(false)}
        className="max-w-sm"
        footer={
          <>
            <Button variant="ghost" onClick={() => setNewGroupDialogOpen(false)}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              onClick={() => {
                if (newGroupName.trim()) {
                  void addGroup(newGroupName.trim());
                }
                setNewGroupDialogOpen(false);
              }}
            >
              {t('common.ok')}
            </Button>
          </>
        }
      >
        <Input
          autoFocus
          value={newGroupName}
          onChange={(e) => setNewGroupName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              if (newGroupName.trim()) {
                void addGroup(newGroupName.trim());
              }
              setNewGroupDialogOpen(false);
            }
          }}
          placeholder={t('main.groupNamePlaceholder')}
          className="text-sm"
          autoCapitalize="off"
          autoCorrect="off"
        />
      </Dialog>

      {/* ── Error / info dialog ── */}
      <Dialog
        open={errorDialogOpen}
        title={t('common.hint')}
        onClose={() => setErrorDialogOpen(false)}
        className="max-w-xs"
        footer={
          <Button variant="primary" onClick={() => setErrorDialogOpen(false)}>
            {t('common.ok')}
          </Button>
        }
      >
        <p className="whitespace-pre-wrap break-all text-sm text-fg-secondary">{errorMessage}</p>
      </Dialog>

      {/* ── Export password dialog ── */}
      <Dialog
        open={exportPasswordDialogOpen}
        title={t('configExport.passwordTitle')}
        onClose={() => { setExportPasswordDialogOpen(false); setPendingExportPath(null); }}
        className="max-w-sm"
        footer={
          <>
            <Button variant="ghost" onClick={() => { setExportPasswordDialogOpen(false); setPendingExportPath(null); }}>
              {t('common.cancel')}
            </Button>
            <Button variant="primary" onClick={() => void submitExportPassword()}>
              {t('common.ok')}
            </Button>
          </>
        }
      >
        <p className="mb-3 text-sm text-fg-secondary">{t('configExport.passwordDesc')}</p>
        <Input
          type="password"
          autoFocus
          value={exportPassword}
          onChange={(e) => { setExportPassword(e.target.value); setExportPasswordError(''); }}
          placeholder={t('configExport.passwordPlaceholder')}
          className="mb-2 text-sm"
          onKeyDown={(e) => { if (e.key === 'Enter') document.getElementById('export-pw-confirm')?.focus(); }}
        />
        <Input
          id="export-pw-confirm"
          type="password"
          value={exportPasswordConfirm}
          onChange={(e) => { setExportPasswordConfirm(e.target.value); setExportPasswordError(''); }}
          placeholder={t('configExport.passwordConfirmPlaceholder')}
          className="text-sm"
          onKeyDown={(e) => { if (e.key === 'Enter') void submitExportPassword(); }}
        />
        {exportPasswordError && (
          <p className="mt-2 text-xs text-red-400">{exportPasswordError}</p>
        )}
      </Dialog>

      {/* ── Import password dialog ── */}
      <Dialog
        open={importPasswordDialogOpen}
        title={t('configImport.passwordTitle')}
        onClose={() => { setImportPasswordDialogOpen(false); setPendingImportPath(null); }}
        className="max-w-sm"
        footer={
          <>
            <Button variant="ghost" onClick={() => { setImportPasswordDialogOpen(false); setPendingImportPath(null); }}>
              {t('common.cancel')}
            </Button>
            <Button variant="primary" onClick={() => void submitImportPassword()}>
              {t('common.ok')}
            </Button>
          </>
        }
      >
        <p className="mb-3 text-sm text-fg-secondary">{t('configImport.passwordDesc')}</p>
        <Input
          type="password"
          autoFocus
          value={importPassword}
          onChange={(e) => { setImportPassword(e.target.value); setImportPasswordError(''); }}
          placeholder={t('configImport.passwordPlaceholder')}
          className="text-sm"
          onKeyDown={(e) => { if (e.key === 'Enter') void submitImportPassword(); }}
        />
        {importPasswordError && (
          <p className="mt-2 text-xs text-red-400">{importPasswordError}</p>
        )}
      </Dialog>

      {/* ── Import config dialog ── */}
      <ImportConfigDialog
        open={importDialogOpen}
        onClose={() => setImportDialogOpen(false)}
        importData={importData}
        onResult={handleImportResult}
      />

      {/* ── Status bar ── */}
      <StatusBar
        left={
          <span className="truncate">
            {statusLeft}
            <span className="mx-2 text-edge">|</span>
            <span>{t('main.connectionCount', { count: connections.length })}</span>
          </span>
        }
        right={<span className="tabular-nums">DataZen v0.0.6</span>}
      />
    </div>
  );
}
