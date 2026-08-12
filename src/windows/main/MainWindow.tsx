import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ChevronDown, ChevronRight, Plus, Search } from 'lucide-react';
import { Menu, MenuItem, Submenu, PredefinedMenuItem } from '@tauri-apps/api/menu';
import { StatusBar } from '../../components/StatusBar';
import { Dialog } from '../../components/ui/Dialog';
import { Input } from '../../components/ui/Input';
import { Button } from '../../components/ui/Button';
import { TitleBar } from '../../components/TitleBar';
import { MenuBar } from '../../components/MenuBar';
import { useKeyboardShortcuts } from '../../hooks/useKeyboardShortcuts';
import { useResizable } from '../../hooks/useResizable';
import { useTauriEvent } from '../../hooks/useTauriEvent';
import { useThemeListener } from '../../hooks/useThemeListener';
import { groupConnections, useConnectionStore } from '../../stores/connectionStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useUiStore } from '../../stores/uiStore';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import { cn } from '../../lib/cn';
import { formatGroupLabel } from '../../lib/connectionGroups';
import { listenCrossWindow } from '../../lib/crossWindowBus';
import {
  openBackupWindow,
  openConnectionWindow,
  openDashboardWindow,
  openDataSyncWindow,
  openNewConnectionWindow,
  openSchemaDiffWindow,
  openSettingsWindow,
  openWorkflowWindow,
} from '../../lib/windowManager';
import { ThemeToggle } from '../../components/ThemeToggle';
import { useI18n } from '../../hooks/useI18n';
import { ActionPanel } from './ActionPanel';
import { ConnectionItem } from './ConnectionItem';
import { backupCommands } from '../../commands/backup';
import { settingsCommands } from '../../commands/settings';
import {
  ConnectionShareDialog,
  type ConnectionImportSource,
  type ConnectionShareMode,
} from '../../components/connection/ConnectionShareDialog';
import { useDashboardStore } from '../../stores/dashboardStore';
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

  const markConnecting = useActiveConnectionStore((s) => s.markConnecting);
  const disconnectAction = useActiveConnectionStore((s) => s.disconnect);
  const activeConnections = useActiveConnectionStore((s) => s.connections);

  const [messageDialogOpen, setMessageDialogOpen] = useState(false);
  const [messageDialogText, setMessageDialogText] = useState('');
  const [messageDialogKind, setMessageDialogKind] = useState<'error' | 'success'>('error');
  const showMessageDialog = useCallback((text: string, kind: 'error' | 'success') => {
    setMessageDialogText(text);
    setMessageDialogKind(kind);
    setMessageDialogOpen(true);
  }, []);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());

  // Inline rename state
  const [renamingGroup, setRenamingGroup] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState('');

  // New group dialog state
  const [newGroupDialogOpen, setNewGroupDialogOpen] = useState(false);
  const [newGroupName, setNewGroupName] = useState('');

  const [connShareOpen, setConnShareOpen] = useState(false);
  const [connShareMode, setConnShareMode] = useState<ConnectionShareMode>('export');
  const [connShareImportSource, setConnShareImportSource] =
    useState<ConnectionImportSource>('file');

  const fetchDashboards = useDashboardStore((s) => s.fetchDashboards);

  // ── Pointer-based drag state ──
  const [draggingConnId, setDraggingConnId] = useState<string | null>(null);
  const [dragOverGroup, setDragOverGroup] = useState<string | null>(null);
  const [dragGhostPos, setDragGhostPos] = useState<{ x: number; y: number } | null>(null);
  const dragStartPos = useRef<{ x: number; y: number } | null>(null);
  const dragConnRef = useRef<ConnectionConfig | null>(null);
  const dragActiveRef = useRef(false);
  const groupRectsRef = useRef<Map<string, DOMRect>>(new Map());

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
    let cancelled = false;
    const fns: (() => void)[] = [];
    listenCrossWindow('datazen:connection-closed', (payload) => {
      const data = payload as { connectionId?: string } | undefined;
      if (data?.connectionId) {
        useActiveConnectionStore.getState().removeByConnectionId(data.connectionId);
      }
    }).then((fn) => {
      if (cancelled) fn();
      else fns.push(fn);
    });
    listenCrossWindow('datazen:connection-ready', (payload) => {
      const data = payload as { configId?: string; connectionId?: string } | undefined;
      if (data?.configId && data?.connectionId) {
        useActiveConnectionStore.getState().markConnected(data.configId, data.connectionId);
      }
    }).then((fn) => {
      if (cancelled) fn();
      else fns.push(fn);
    });
    listenCrossWindow('datazen:connection-failed', (payload) => {
      const data = payload as { configId?: string; error?: string } | undefined;
      if (data?.configId) {
        useActiveConnectionStore
          .getState()
          .markError(data.configId, data?.error ?? t('backend.unknownError'));
      }
    }).then((fn) => {
      if (cancelled) fn();
      else fns.push(fn);
    });
    return () => {
      cancelled = true;
      fns.forEach((fn) => fn());
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    const cleanups: (() => void)[] = [];
    void listenCrossWindow('menu:open-settings', () => {
      if (!cancelled) openSettingsWindow();
    }).then((u) => {
      if (cancelled) u();
      else cleanups.push(u);
    });
    void listenCrossWindow('menu:new-connection', () => {
      if (!cancelled) openNewConnectionWindow();
    }).then((u) => {
      if (cancelled) u();
      else cleanups.push(u);
    });
    void listenCrossWindow('menu:data-sync', () => {
      if (!cancelled) openDataSyncWindow();
    }).then((u) => {
      if (cancelled) u();
      else cleanups.push(u);
    });
    void listenCrossWindow('menu:schema-diff', () => {
      if (!cancelled) openSchemaDiffWindow();
    }).then((u) => {
      if (cancelled) u();
      else cleanups.push(u);
    });
    void listenCrossWindow('menu:backup', () => {
      if (!cancelled) openBackupWindow();
    }).then((u) => {
      if (cancelled) u();
      else cleanups.push(u);
    });
    void listenCrossWindow('menu:view-logs', () => {
      if (!cancelled) void settingsCommands.openLogDir();
    }).then((u) => {
      if (cancelled) u();
      else cleanups.push(u);
    });
    return () => {
      cancelled = true;
      cleanups.forEach((fn) => fn());
    };
  }, []);

  // (native context menus handle their own dismiss)

  const handleConnect = useCallback(
    (cfg: ConnectionConfig) => {
      const existing = useActiveConnectionStore.getState().connections[cfg.id];
      if (existing?.status === 'connected' && existing.connectionId) {
        openConnectionWindow(
          { connectionId: existing.connectionId },
          cfg.name,
          cfg.database,
          cfg.databaseType,
        );
        return;
      }
      if (existing?.status !== 'connecting') {
        markConnecting(cfg.id, cfg.database ?? null);
      }
      openConnectionWindow({ configId: cfg.id }, cfg.name, cfg.database, cfg.databaseType);
    },
    [markConnecting],
  );

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
    {
      key: 'mod+n',
      scope: 'global',
      description: 'New Connection',
      action: () => openNewConnectionWindow(),
    },
  ]);

  // ── Status ──
  const activeCount = useMemo(
    () => Object.values(activeConnections).filter((e) => e.status === 'connected').length,
    [activeConnections],
  );

  const statusLeft = (() => {
    if (loading) return t('common.loading');
    if (error) return <span className="text-red-400">{error}</span>;
    if (activeCount > 0)
      return (
        <span className="text-green-400">
          {t('main.activeConnections', { count: activeCount })}
        </span>
      );
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

  const handleGroupContextMenu = useCallback(
    async (e: React.MouseEvent, groupName: string) => {
      e.preventDefault();
      e.stopPropagation();
      const isUngrouped = groupName === '';

      const items: Array<MenuItem | PredefinedMenuItem> = [
        await MenuItem.new({
          text: t('main.ctx.newGroup'),
          action: () => {
            setNewGroupName('');
            setNewGroupDialogOpen(true);
          },
        }),
      ];
      if (!isUngrouped) {
        items.push(
          await PredefinedMenuItem.new({ item: 'Separator' }),
          await MenuItem.new({
            text: t('main.ctx.renameGroup'),
            action: () => {
              setRenamingGroup(groupName);
              setRenameValue(formatGroupLabel(groupName, t));
            },
          }),
          await MenuItem.new({
            text: t('main.ctx.deleteGroup'),
            action: () => {
              void deleteGroup(groupName);
            },
          }),
        );
      }
      const menu = await Menu.new({ items });
      await menu.popup();
    },
    [deleteGroup, t],
  );

  const handleConnectionContextMenu = useCallback(
    async (e: React.MouseEvent, conn: ConnectionConfig) => {
      e.preventDefault();
      e.stopPropagation();
      setSelectedId(conn.id);

      const isConnected = activeConnections[conn.id]?.status === 'connected';
      const items: Array<MenuItem | Submenu | PredefinedMenuItem> = [
        await MenuItem.new({
          text: isConnected ? t('main.ctx.disconnect') : t('main.ctx.openConnection'),
          action: () => {
            if (isConnected) void disconnectAction(conn.id);
            else void handleConnect(conn);
          },
        }),
        await PredefinedMenuItem.new({ item: 'Separator' }),
        await MenuItem.new({
          text: t('main.ctx.editConnection'),
          action: () => openNewConnectionWindow(conn.id),
        }),
        await MenuItem.new({
          text: t('main.ctx.duplicateConnection'),
          action: () => {
            void duplicateConnection(conn.id);
          },
        }),
        await PredefinedMenuItem.new({ item: 'Separator' }),
      ];

      const moveTargets = groups.filter((g) => g !== conn.group);
      if (moveTargets.length > 0 || conn.group) {
        const subItems: MenuItem[] = [];
        for (const g of moveTargets) {
          subItems.push(
            await MenuItem.new({
              text: formatGroupLabel(g, t),
              action: () => {
                void moveConnectionToGroup(conn.id, g);
              },
            }),
          );
        }
        if (conn.group) {
          subItems.push(
            await MenuItem.new({
              text: t('main.ctx.removeFromGroup'),
              action: () => {
                void moveConnectionToGroup(conn.id, undefined);
              },
            }),
          );
        }
        items.push(await Submenu.new({ text: t('main.ctx.moveToGroup'), items: subItems }));
        items.push(await PredefinedMenuItem.new({ item: 'Separator' }));
      }

      items.push(
        await MenuItem.new({
          text: t('main.ctx.deleteConnection'),
          action: async () => {
            const { ask } = await import('@tauri-apps/plugin-dialog');
            const confirmed = await ask(
              t('main.ctx.confirmDeleteConnection', { name: conn.name }),
              { title: t('main.ctx.deleteConnection'), kind: 'warning' },
            );
            if (confirmed) {
              void deleteConnection(conn.id);
            }
          },
        }),
      );
      const menu = await Menu.new({ items });
      await menu.popup();
    },
    [
      activeConnections,
      groups,
      disconnectAction,
      handleConnect,
      duplicateConnection,
      deleteConnection,
      moveConnectionToGroup,
      t,
    ],
  );

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

  // ── Export / Import app data handlers ──

  const handleExportConfig = useCallback(async () => {
    let saved: boolean;
    try {
      const date = new Date().toISOString().slice(0, 10).replace(/-/g, '');
      saved = await backupCommands.exportAppDataWithDialog(`datazen-backup-${date}.zip`);
    } catch (e) {
      showMessageDialog(e instanceof Error ? e.message : t('appData.exportFailed'), 'error');
      return;
    }
    if (!saved) return;
    showMessageDialog(t('appData.exportSuccess'), 'success');

    try {
      const { ask } = await import('@tauri-apps/plugin-dialog');
      const wantKey = await ask(t('appData.backupKeyMessage'), {
        title: t('appData.backupKeyTitle'),
        kind: 'info',
      });
      if (wantKey) {
        const keySaved = await backupCommands.saveEncryptionKeyWithDialog('datazen.key');
        if (keySaved) {
          showMessageDialog(t('appData.backupKeySaved'), 'success');
        }
      }
    } catch (e) {
      showMessageDialog(e instanceof Error ? e.message : t('appData.backupKeyFailed'), 'error');
    }
  }, [t, showMessageDialog]);

  const handleImportConfig = useCallback(async () => {
    try {
      const imported = await backupCommands.importAppDataWithDialog(
        t('appData.importConfirmTitle'),
        t('appData.importConfirmMessage'),
      );
      if (!imported) return;
      await backupCommands.restartApp();
    } catch (e) {
      showMessageDialog(e instanceof Error ? e.message : t('appData.importFailed'), 'error');
    }
  }, [t, showMessageDialog]);

  const openConnShare = useCallback(
    (mode: ConnectionShareMode, source: ConnectionImportSource = 'file') => {
      setConnShareMode(mode);
      setConnShareImportSource(source);
      setConnShareOpen(true);
    },
    [],
  );

  const handleConnShareExportSuccess = useCallback(
    (count: number) => {
      showMessageDialog(t('connShare.exportSuccess', { count }), 'success');
    },
    [showMessageDialog, t],
  );

  const handleOpenDashboard = useCallback(async () => {
    await fetchDashboards();
    const list = useDashboardStore.getState().list;
    if (list.length > 0) {
      openDashboardWindow(list[0]!.id, list[0]!.name);
      return;
    }
    // No boards yet — open the dashboard shell; window shows empty-state create CTA.
    openDashboardWindow();
  }, [fetchDashboards]);

  const handleConnShareImportSuccess = useCallback(
    async (result: {
      imported: number;
      overwritten: number;
      groupsAdded: number;
      skipped?: string[];
      sourceFormat?: string;
    }) => {
      await fetchConnections();
      await fetchGroups();
      const skippedCount = result.skipped?.length ?? 0;
      const base =
        skippedCount > 0
          ? t('connShare.importSuccessWithSkipped', {
              imported: result.imported,
              overwritten: result.overwritten,
              groupsAdded: result.groupsAdded,
              skipped: skippedCount,
            })
          : t('connShare.importSuccess', {
              imported: result.imported,
              overwritten: result.overwritten,
              groupsAdded: result.groupsAdded,
            });
      const message = result.sourceFormat ? `${base} (${result.sourceFormat})` : base;
      showMessageDialog(message, 'success');
    },
    [fetchConnections, fetchGroups, showMessageDialog, t],
  );

  // ── Menu bar events for export/import ──
  useEffect(() => {
    let cancelled = false;
    const cleanups: (() => void)[] = [];
    void listenCrossWindow('menu:export-config', () => {
      if (!cancelled) void handleExportConfig();
    }).then((u) => {
      if (cancelled) u();
      else cleanups.push(u);
    });
    void listenCrossWindow('menu:import-config', () => {
      if (!cancelled) void handleImportConfig();
    }).then((u) => {
      if (cancelled) u();
      else cleanups.push(u);
    });
    void listenCrossWindow('menu:export-connections', () => {
      if (!cancelled) openConnShare('export');
    }).then((u) => {
      if (cancelled) u();
      else cleanups.push(u);
    });
    void listenCrossWindow('menu:import-connections', () => {
      if (!cancelled) openConnShare('import', 'file');
    }).then((u) => {
      if (cancelled) u();
      else cleanups.push(u);
    });
    void listenCrossWindow('menu:import-connections-file', () => {
      if (!cancelled) openConnShare('import', 'file');
    }).then((u) => {
      if (cancelled) u();
      else cleanups.push(u);
    });
    void listenCrossWindow('menu:import-connections-dbx', () => {
      if (!cancelled) openConnShare('import', 'dbx');
    }).then((u) => {
      if (cancelled) u();
      else cleanups.push(u);
    });
    void listenCrossWindow('menu:import-connections-navicat', () => {
      if (!cancelled) openConnShare('import', 'navicat');
    }).then((u) => {
      if (cancelled) u();
      else cleanups.push(u);
    });
    void listenCrossWindow('menu:import-connections-datagrip', () => {
      if (!cancelled) openConnShare('import', 'datagrip');
    }).then((u) => {
      if (cancelled) u();
      else cleanups.push(u);
    });
    void listenCrossWindow('menu:import-connections-dbeaver', () => {
      if (!cancelled) openConnShare('import', 'dbeaver');
    }).then((u) => {
      if (cancelled) u();
      else cleanups.push(u);
    });
    void listenCrossWindow('menu:import-connections-tableplus', () => {
      if (!cancelled) openConnShare('import', 'tableplus');
    }).then((u) => {
      if (cancelled) u();
      else cleanups.push(u);
    });
    return () => {
      cancelled = true;
      cleanups.forEach((fn) => fn());
    };
  }, [handleExportConfig, handleImportConfig, openConnShare]);

  // ── Backup / Restore handlers ──

  const handleRestore = useCallback(async () => {
    try {
      if (!selectedId) {
        showMessageDialog(t('main.restoreFailed'), 'error');
        return;
      }
      const conn = connections.find((c) => c.id === selectedId);
      if (!conn) return;

      const entry = activeConnections[conn.id];
      if (entry?.status !== 'connected' || !entry.connectionId) {
        showMessageDialog(t('main.restoreFailed'), 'error');
        return;
      }

      const { invoke } = await import('@tauri-apps/api/core');
      const restored = await invoke<boolean>('restore_database_with_dialog', {
        connectionId: entry.connectionId,
      });
      if (!restored) return;
      showMessageDialog(t('main.restoreSuccess'), 'success');
    } catch (e) {
      showMessageDialog(e instanceof Error ? e.message : String(e), 'error');
    }
  }, [selectedId, connections, activeConnections, t, showMessageDialog]);

  useEffect(() => {
    let cancelled = false;
    const cleanups: (() => void)[] = [];
    void listenCrossWindow('menu:restore', () => {
      if (!cancelled) void handleRestore();
    }).then((u) => {
      if (cancelled) u();
      else cleanups.push(u);
    });
    return () => {
      cancelled = true;
      cleanups.forEach((fn) => fn());
    };
  }, [handleRestore]);

  // ── Blank area context menu ──

  const handleBlankContextMenu = useCallback(async (e: React.MouseEvent) => {
    const el = e.target as HTMLElement;
    if (el.closest('[data-group-header]') || el.closest('[data-conn-item]')) return;
    e.preventDefault();
    const menu = await Menu.new({
      items: [
        await MenuItem.new({
          text: t('main.ctx.newGroup'),
          action: () => {
            setNewGroupName('');
            setNewGroupDialogOpen(true);
          },
        }),
        await MenuItem.new({
          text: t('main.newConnection'),
          action: () => openNewConnectionWindow(),
        }),
      ],
    });
    await menu.popup();
  }, []);

  return (
    <div className="flex h-screen min-h-0 min-w-[520px] flex-col bg-surface text-fg">
      {/* ── Title bar ── */}
      <TitleBar title="DataZen" leftContent={<MenuBar />} rightContent={<ThemeToggle />} />

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
            onWorkflow={() => openWorkflowWindow()}
            onDashboard={() => void handleOpenDashboard()}
          />
        </aside>
        <div
          ref={handleRef}
          className="w-1 shrink-0 cursor-col-resize bg-transparent hover:bg-accent/30"
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
          <div className="flex-1 overflow-auto" onContextMenu={handleBlankContextMenu}>
            {grouped.length === 0 && (
              <div className="flex flex-col items-center justify-center py-20 text-sm text-fg-muted">
                <p>{t('main.noConnections')}</p>
                <Button variant="ghost" className="mt-3" onClick={() => openNewConnectionWindow()}>
                  <Plus className="h-4 w-4" />
                  {t('main.createFirst')}
                </Button>
              </div>
            )}
            {grouped.map(({ group: groupName, connections: groupConns }) => {
              const expanded = expandedGroups.has(groupName);
              const displayName = groupName ? formatGroupLabel(groupName, t) : t('main.ungrouped');
              const isDragOver = dragOverGroup === groupName;

              return (
                <div
                  key={groupName}
                  data-group-name={groupName}
                  className={cn(
                    'transition-colors',
                    isDragOver && draggingConnId && 'bg-accent/10 ring-1 ring-inset ring-accent/30',
                  )}
                >
                  {/* ── Group header ── */}
                  <div
                    data-group-header
                    className="flex cursor-pointer select-none items-center gap-1.5 px-3 py-2 hover:bg-surface-raised/50"
                    onClick={() => toggleGroup(groupName)}
                    onContextMenu={(e) => {
                      void handleGroupContextMenu(e, groupName);
                    }}
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
          {draggingConnId &&
            dragGhostPos &&
            (() => {
              const conn = connections.find((c) => c.id === draggingConnId);
              if (!conn) return null;
              return (
                <div
                  className="pointer-events-none fixed z-[9999] rounded-lg border border-accent/40 bg-surface-alt px-3 py-2 text-[13px] font-medium text-fg shadow-xl"
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

      <ConnectionShareDialog
        open={connShareOpen}
        mode={connShareMode}
        importSource={connShareImportSource}
        onClose={() => setConnShareOpen(false)}
        onExportSuccess={handleConnShareExportSuccess}
        onImportSuccess={(result) => void handleConnShareImportSuccess(result)}
        onError={(message) => showMessageDialog(message, 'error')}
      />

      {/* ── Message dialog (success / error) ── */}
      <Dialog
        open={messageDialogOpen}
        title={messageDialogKind === 'success' ? t('common.success') : t('common.error')}
        onClose={() => setMessageDialogOpen(false)}
        className="max-w-xs"
        footer={
          <Button variant="primary" onClick={() => setMessageDialogOpen(false)}>
            {t('common.ok')}
          </Button>
        }
      >
        <p className="whitespace-pre-wrap break-all text-sm text-fg-secondary">
          {messageDialogText}
        </p>
      </Dialog>

      {/* ── Status bar ── */}
      <StatusBar
        left={
          <span className="truncate">
            {statusLeft}
            <span className="mx-2 text-edge">|</span>
            <span>{t('main.connectionCount', { count: connections.length })}</span>
          </span>
        }
        right={<span className="tabular-nums">DataZen v0.0.9</span>}
      />
    </div>
  );
}
