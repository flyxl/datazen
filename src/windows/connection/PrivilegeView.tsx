import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  ChevronDown,
  ChevronRight,
  Database,
  FolderOpen,
  Loader2,
  Play,
  Plus,
  RefreshCw,
  Table2,
  Trash2,
  User,
  X,
} from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { useI18n } from '../../hooks/useI18n';
import { CopyableError } from '../../components/ui/CopyableError';
import { databaseCommands } from '../../commands/database';
import { driverCommands } from '../../commands/driver';
import { queryCommands } from '../../commands/query';
import { PrivilegeSelector, usePrivilegeOptions } from '../../components/admin/PrivilegeSelector';
import { useConnectionCommand, useConnectionCommands } from '../../hooks/useConnectionCommand';
import { hasCommand } from '../../lib/commandSchema';
import type { PrivilegeGrant } from '../../types';

// ─── Types ────────────────────────────────────────────────────────────

interface PrivilegeViewProps {
  dbSessionId: string;
  databaseType?: string;
}

type ViewMode = 'by-user' | 'by-object';

interface SchemaGroup {
  [tableName: string]: string[];
}

interface DatabaseGroup {
  [schemaName: string]: SchemaGroup;
}

interface UserPrivTree {
  [database: string]: DatabaseGroup;
}

interface ObjectUserMap {
  [userName: string]: string[];
}

interface ObjectTableGroup {
  [tableName: string]: ObjectUserMap;
}

interface ObjectSchemaGroup {
  [schemaName: string]: ObjectTableGroup;
}

interface ObjectTree {
  [database: string]: ObjectSchemaGroup;
}

// ─── Tree builders ────────────────────────────────────────────────────

function buildUserTree(grants: PrivilegeGrant[]): Record<string, UserPrivTree> {
  const result: Record<string, UserPrivTree> = {};
  for (const g of grants) {
    const user = g.grantee;
    const schema = g.objectSchema ?? '*';
    const table = g.objectName || '*';

    if (!result[user]) result[user] = {};
    const dbKey = schema === '*' && table === '*' ? '*' : 'current';
    if (!result[user][dbKey]) result[user][dbKey] = {};
    if (!result[user][dbKey][schema]) result[user][dbKey][schema] = {};
    if (!result[user][dbKey][schema][table]) result[user][dbKey][schema][table] = [];
    result[user][dbKey][schema][table].push(g.privilege);
  }
  return result;
}

function buildObjectTree(grants: PrivilegeGrant[]): ObjectTree {
  const result: ObjectTree = {};
  for (const g of grants) {
    const schema = g.objectSchema ?? '*';
    const table = g.objectName || '*';
    const dbKey = schema === '*' && table === '*' ? '*' : 'current';

    if (!result[dbKey]) result[dbKey] = {};
    if (!result[dbKey][schema]) result[dbKey][schema] = {};
    if (!result[dbKey][schema][table]) result[dbKey][schema][table] = {};
    if (!result[dbKey][schema][table][g.grantee]) result[dbKey][schema][table][g.grantee] = [];
    result[dbKey][schema][table][g.grantee].push(g.privilege);
  }
  return result;
}

// ─── Collapsible Tree Node ────────────────────────────────────────────

function TreeNode({
  icon,
  label,
  count,
  trailing,
  defaultOpen = false,
  children,
  depth = 0,
}: {
  icon: React.ReactNode;
  label: string;
  count?: number;
  trailing?: React.ReactNode;
  defaultOpen?: boolean;
  children?: React.ReactNode;
  depth?: number;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div>
      <div
        role="button"
        tabIndex={0}
        className="group flex w-full cursor-pointer items-center gap-1 py-1 text-left text-xs hover:bg-surface-raised/50"
        style={{ paddingLeft: depth * 16 + 4 }}
        onClick={() => setOpen(!open)}
        onKeyDown={(e) => e.key === 'Enter' && setOpen(!open)}
      >
        {children ? (
          open ? (
            <ChevronDown className="h-3 w-3 shrink-0 text-fg-muted" />
          ) : (
            <ChevronRight className="h-3 w-3 shrink-0 text-fg-muted" />
          )
        ) : (
          <span className="h-3 w-3 shrink-0" />
        )}
        <span className="shrink-0 text-fg-muted">{icon}</span>
        <span className="min-w-0 truncate text-fg">{label}</span>
        {count != null && <span className="ml-1 text-[10px] text-fg-muted">({count})</span>}
        <span className="flex-1" />
        <span className="invisible group-hover:visible">{trailing}</span>
      </div>
      {open && children && <div>{children}</div>}
    </div>
  );
}

// ─── Privilege Pills ──────────────────────────────────────────────────

function PrivilegePills({
  privileges,
  onRevoke,
}: {
  privileges: string[];
  onRevoke?: (priv: string) => void;
}) {
  return (
    <div className="flex flex-wrap gap-1">
      {privileges.map((p) => (
        <span
          key={p}
          className="inline-flex items-center gap-0.5 rounded bg-surface-raised px-1.5 py-0 text-[10px] font-medium text-fg-secondary"
        >
          {p}
          {onRevoke && (
            <button
              type="button"
              className="ml-0.5 rounded-sm p-0 text-fg-muted opacity-60 hover:text-red-400 hover:opacity-100"
              onClick={(e) => {
                e.stopPropagation();
                onRevoke(p);
              }}
              title={`Revoke ${p}`}
            >
              <X className="h-2.5 w-2.5" />
            </button>
          )}
        </span>
      ))}
    </div>
  );
}

// ─── Leaf Row (privilege pills inline) ────────────────────────────────

function PrivilegeLeafRow({
  icon,
  label,
  privileges,
  onRevoke,
  trailing,
  depth = 0,
}: {
  icon: React.ReactNode;
  label: string;
  privileges: string[];
  onRevoke?: (priv: string) => void;
  trailing?: React.ReactNode;
  depth?: number;
}) {
  return (
    <div
      className="group flex items-center gap-1.5 py-1 text-xs"
      style={{ paddingLeft: depth * 16 + 4 + 12 }}
    >
      <span className="shrink-0 text-fg-muted">{icon}</span>
      <span className="shrink-0 text-fg">{label}</span>
      <span className="mx-1 flex-1">
        <PrivilegePills privileges={privileges} onRevoke={onRevoke} />
      </span>
      <span className="invisible shrink-0 group-hover:visible">{trailing}</span>
    </div>
  );
}

// ─── Grant Dialog ─────────────────────────────────────────────────────

function GrantDialog({
  dbSessionId,
  users,
  initialUser,
  onGranted,
  onClose,
}: {
  dbSessionId: string;
  users: string[];
  initialUser?: string;
  onGranted: () => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const { definition: grantDefinition } = useConnectionCommand(dbSessionId, 'grant_privileges');
  const { all: allPrivs } = usePrivilegeOptions(grantDefinition);

  const [username, setUsername] = useState(initialUser ?? '');
  const [database, setDatabase] = useState('');
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const toggle = (p: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(p)) next.delete(p);
      else next.add(p);
      return next;
    });
  };

  const handleGrant = async () => {
    if (!username.trim() || selected.size === 0) return;
    setSubmitting(true);
    setError(null);
    try {
      await driverCommands.execute({
        dbSessionId,
        command: 'grant_privileges',
        input: {
          username: username.trim(),
          database: database.trim() || undefined,
          privileges: [...selected],
        },
      });
      onGranted();
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="w-[420px] rounded-lg border border-edge bg-surface p-4 shadow-lg">
        <div className="mb-3 flex items-center justify-between">
          <h3 className="text-sm font-medium text-fg">{t('common.grantPrivileges')}</h3>
          <button
            type="button"
            className="rounded p-0.5 text-fg-muted hover:text-fg"
            onClick={onClose}
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="space-y-3">
          <div>
            <label className="mb-1 block text-[11px] font-medium text-fg-muted">
              {t('privileges.targetUser')}
            </label>
            <input
              className="h-8 w-full rounded border border-edge bg-surface-alt px-2.5 text-xs text-fg outline-none focus:border-accent"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              list="priv-users"
              placeholder={t('privileges.userPlaceholder')}
            />
            <datalist id="priv-users">
              {users.map((u) => (
                <option key={u} value={u} />
              ))}
            </datalist>
          </div>

          <div>
            <label className="mb-1 block text-[11px] font-medium text-fg-muted">
              {t('privileges.targetDatabase')}
            </label>
            <input
              className="h-8 w-full rounded border border-edge bg-surface-alt px-2.5 text-xs text-fg outline-none focus:border-accent"
              value={database}
              onChange={(e) => setDatabase(e.target.value)}
              placeholder={t('privileges.dbPlaceholder')}
            />
          </div>

          <div>
            <div className="mb-1.5 flex items-center justify-between">
              <span className="text-[11px] font-medium text-fg-muted">
                {t('privileges.selectPrivileges')}
              </span>
              <div className="flex gap-1">
                <button
                  type="button"
                  className="text-[10px] text-accent-fg hover:underline"
                  onClick={() => setSelected(new Set(allPrivs))}
                >
                  {t('common.selectAll')}
                </button>
                <span className="text-[10px] text-fg-muted">/</span>
                <button
                  type="button"
                  className="text-[10px] text-accent-fg hover:underline"
                  onClick={() => setSelected(new Set())}
                >
                  {t('common.deselectAll')}
                </button>
              </div>
            </div>

            <PrivilegeSelector
              definition={grantDefinition}
              selected={selected}
              onToggle={toggle}
              className="grid grid-cols-3 gap-1.5"
              itemClassName="flex cursor-pointer items-center gap-1.5 text-[11px] text-fg"
            />
          </div>

          {error && (
            <div className="flex items-start gap-2 rounded bg-red-500/10 px-2 py-1.5">
              <CopyableError message={error} className="min-w-0 flex-1 text-[11px] text-red-400" />
              <button
                type="button"
                className="shrink-0 text-red-400"
                onClick={() => setError(null)}
              >
                <X className="h-3 w-3" />
              </button>
            </div>
          )}

          <div className="flex justify-end gap-2 pt-1">
            <Button variant="ghost" className="h-7 px-3 text-xs" onClick={onClose}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              className="h-7 px-3 text-xs"
              disabled={submitting || !username.trim() || selected.size === 0}
              onClick={() => void handleGrant()}
            >
              {submitting ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                t('privileges.grant')
              )}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ─── By User View ─────────────────────────────────────────────────────

function ByUserView({
  grants,
  dbSessionId,
  supportsDropUser,
  onRefresh,
  actionError,
  setActionError,
}: {
  grants: PrivilegeGrant[];
  dbSessionId: string;
  supportsDropUser: boolean;
  onRefresh: () => void;
  actionError: string | null;
  setActionError: (e: string | null) => void;
}) {
  const { t } = useI18n();
  const [confirmAction, confirmDialog] = useConfirmDialog();
  const userTree = useMemo(() => buildUserTree(grants), [grants]);
  const users = useMemo(() => Object.keys(userTree).sort(), [userTree]);

  const handleDropUser = async (username: string) => {
    const ok = await confirmAction({
      title: t('privileges.dropUser'),
      message: t('privileges.confirmDropUser', { name: username }),
      confirmLabel: t('privileges.dropUser'),
      kind: 'warning',
    });
    if (!ok) return;
    setActionError(null);
    try {
      await driverCommands.execute({ dbSessionId, command: 'drop_user', input: { username } });
      onRefresh();
    } catch (e) {
      setActionError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleRevoke = async (grantee: string, privileges: string[], objectName: string) => {
    const label = privileges.length === 1 ? privileges[0] : t('privileges.allPrivileges');
    const ok = await confirmAction({
      title: t('privileges.revokePrivilege'),
      message: t('privileges.confirmRevoke', {
        privilege: label,
        user: grantee,
        object: objectName || t('privileges.currentDb'),
      }),
      confirmLabel: t('privileges.revokePrivilege'),
      kind: 'warning',
    });
    if (!ok) return;
    setActionError(null);
    try {
      await driverCommands.execute({
        dbSessionId,
        command: 'revoke_privileges',
        input: { username: grantee, database: objectName, privileges },
      });
      onRefresh();
    } catch (e) {
      setActionError(e instanceof Error ? e.message : String(e));
    }
  };

  const revokeAllBtn = (user: string, privs: string[], objectName: string) => (
    <button
      type="button"
      className="rounded p-0.5 text-fg-muted hover:bg-red-500/20 hover:text-red-400"
      title={t('privileges.revokeAll')}
      onClick={(e) => {
        e.stopPropagation();
        void handleRevoke(user, privs, objectName);
      }}
    >
      <Trash2 className="h-3 w-3" />
    </button>
  );

  return (
    <div className="min-h-0 flex-1 overflow-auto">
      {actionError && (
        <div className="flex items-start gap-2 border-b border-edge bg-red-500/10 px-3 py-2">
          <CopyableError message={actionError} className="min-w-0 flex-1 text-xs text-red-400" />
          <button
            type="button"
            className="shrink-0 rounded p-0.5 text-red-400 hover:bg-red-500/20"
            onClick={() => setActionError(null)}
          >
            <X className="h-3 w-3" />
          </button>
        </div>
      )}
      {users.length === 0 && (
        <div className="p-3 text-xs text-fg-muted">{t('privileges.empty')}</div>
      )}
      {users.map((user) => {
        const databases = userTree[user];
        const userPrivCount = grants.filter((g) => g.grantee === user).length;

        return (
          <TreeNode
            key={user}
            icon={<User className="h-3.5 w-3.5" />}
            label={user}
            count={userPrivCount}
            defaultOpen={users.length <= 5}
            trailing={
              supportsDropUser ? (
                <button
                  type="button"
                  className="rounded p-0.5 text-fg-muted hover:bg-red-500/20 hover:text-red-400"
                  title={t('privileges.dropUser')}
                  onClick={(e) => {
                    e.stopPropagation();
                    void handleDropUser(user);
                  }}
                >
                  <Trash2 className="h-3 w-3" />
                </button>
              ) : undefined
            }
          >
            {Object.entries(databases).map(([dbKey, schemas]) => {
              const dbLabel = dbKey === '*' ? t('privileges.roleLevel') : t('privileges.currentDb');

              return (
                <TreeNode
                  key={dbKey}
                  icon={<Database className="h-3.5 w-3.5" />}
                  label={dbLabel}
                  depth={1}
                  defaultOpen
                >
                  {Object.entries(schemas).map(([schemaName, tables]) => {
                    if (schemaName === '*') {
                      const allPrivs = Object.values(tables).flat();
                      return (
                        <PrivilegeLeafRow
                          key={schemaName}
                          icon={<Database className="h-3 w-3" />}
                          label={t('privileges.roleLevel')}
                          privileges={allPrivs}
                          depth={2}
                        />
                      );
                    }

                    const tableEntries = Object.entries(tables);
                    const allSchemaPrivs = tableEntries.flatMap(([, p]) => p);

                    if (tableEntries.length === 1 && tableEntries[0][0] === '*') {
                      return (
                        <PrivilegeLeafRow
                          key={schemaName}
                          icon={<FolderOpen className="h-3 w-3" />}
                          label={schemaName}
                          privileges={tableEntries[0][1]}
                          onRevoke={(priv) => void handleRevoke(user, [priv], schemaName)}
                          trailing={revokeAllBtn(user, tableEntries[0][1], schemaName)}
                          depth={2}
                        />
                      );
                    }

                    return (
                      <TreeNode
                        key={schemaName}
                        icon={<FolderOpen className="h-3.5 w-3.5" />}
                        label={schemaName}
                        count={tableEntries.length}
                        depth={2}
                        defaultOpen
                        trailing={revokeAllBtn(user, allSchemaPrivs, schemaName)}
                      >
                        {tableEntries.map(([tableName, privs]) => (
                          <PrivilegeLeafRow
                            key={tableName}
                            icon={<Table2 className="h-3 w-3" />}
                            label={tableName === '*' ? t('privileges.allTables') : tableName}
                            privileges={privs}
                            onRevoke={(priv) => void handleRevoke(user, [priv], tableName)}
                            trailing={
                              privs.length > 1 ? revokeAllBtn(user, privs, tableName) : undefined
                            }
                            depth={3}
                          />
                        ))}
                      </TreeNode>
                    );
                  })}
                </TreeNode>
              );
            })}
          </TreeNode>
        );
      })}
      {confirmDialog}
    </div>
  );
}

// ─── By Object View ───────────────────────────────────────────────────

function ByObjectView({
  grants,
  dbSessionId,
  onRefresh,
  actionError,
  setActionError,
}: {
  grants: PrivilegeGrant[];
  dbSessionId: string;
  onRefresh: () => void;
  actionError: string | null;
  setActionError: (e: string | null) => void;
}) {
  const { t } = useI18n();
  const [confirmAction, confirmDialog] = useConfirmDialog();
  const objectTree = useMemo(() => buildObjectTree(grants), [grants]);

  const handleRevoke = async (grantee: string, privileges: string[], objectName: string) => {
    const label = privileges.length === 1 ? privileges[0] : t('privileges.allPrivileges');
    const ok = await confirmAction({
      title: t('privileges.revokePrivilege'),
      message: t('privileges.confirmRevoke', {
        privilege: label,
        user: grantee,
        object: objectName || t('privileges.currentDb'),
      }),
      confirmLabel: t('privileges.revokePrivilege'),
      kind: 'warning',
    });
    if (!ok) return;
    setActionError(null);
    try {
      await driverCommands.execute({
        dbSessionId,
        command: 'revoke_privileges',
        input: { username: grantee, database: objectName, privileges },
      });
      onRefresh();
    } catch (e) {
      setActionError(e instanceof Error ? e.message : String(e));
    }
  };

  const revokeAllBtn = (user: string, privs: string[], objectName: string) => (
    <button
      type="button"
      className="rounded p-0.5 text-fg-muted hover:bg-red-500/20 hover:text-red-400"
      title={t('privileges.revokeAll')}
      onClick={(e) => {
        e.stopPropagation();
        void handleRevoke(user, privs, objectName);
      }}
    >
      <Trash2 className="h-3 w-3" />
    </button>
  );

  return (
    <div className="min-h-0 flex-1 overflow-auto">
      {actionError && (
        <div className="flex items-start gap-2 border-b border-edge bg-red-500/10 px-3 py-2">
          <CopyableError message={actionError} className="min-w-0 flex-1 text-xs text-red-400" />
          <button
            type="button"
            className="shrink-0 rounded p-0.5 text-red-400 hover:bg-red-500/20"
            onClick={() => setActionError(null)}
          >
            <X className="h-3 w-3" />
          </button>
        </div>
      )}
      {Object.keys(objectTree).length === 0 && (
        <div className="p-3 text-xs text-fg-muted">{t('privileges.empty')}</div>
      )}
      {Object.entries(objectTree).map(([dbKey, schemas]) => {
        const dbLabel = dbKey === '*' ? t('privileges.roleLevel') : t('privileges.currentDb');
        return (
          <TreeNode
            key={dbKey}
            icon={<Database className="h-3.5 w-3.5" />}
            label={dbLabel}
            defaultOpen
          >
            {Object.entries(schemas).map(([schemaName, tables]) => {
              if (schemaName === '*') {
                return Object.entries(tables).map(([, users]) =>
                  Object.entries(users).map(([user, privs]) => (
                    <PrivilegeLeafRow
                      key={`${schemaName}-${user}`}
                      icon={<User className="h-3 w-3" />}
                      label={user}
                      privileges={privs}
                      trailing={revokeAllBtn(user, privs, '')}
                      depth={1}
                    />
                  )),
                );
              }

              const tableEntries = Object.entries(tables);
              return (
                <TreeNode
                  key={schemaName}
                  icon={<FolderOpen className="h-3.5 w-3.5" />}
                  label={schemaName}
                  count={tableEntries.length}
                  depth={1}
                  defaultOpen
                >
                  {tableEntries.map(([tableName, users]) => {
                    const userEntries = Object.entries(users);
                    if (tableName === '*') {
                      return userEntries.map(([user, privs]) => (
                        <PrivilegeLeafRow
                          key={`${tableName}-${user}`}
                          icon={<User className="h-3 w-3" />}
                          label={user}
                          privileges={privs}
                          onRevoke={(priv) => void handleRevoke(user, [priv], schemaName)}
                          trailing={
                            privs.length > 1 ? revokeAllBtn(user, privs, schemaName) : undefined
                          }
                          depth={2}
                        />
                      ));
                    }
                    return (
                      <TreeNode
                        key={tableName}
                        icon={<Table2 className="h-3.5 w-3.5" />}
                        label={tableName}
                        count={userEntries.length}
                        depth={2}
                        defaultOpen={userEntries.length <= 3}
                      >
                        {userEntries.map(([user, privs]) => (
                          <PrivilegeLeafRow
                            key={user}
                            icon={<User className="h-3 w-3" />}
                            label={user}
                            privileges={privs}
                            onRevoke={(priv) => void handleRevoke(user, [priv], tableName)}
                            trailing={
                              privs.length > 1 ? revokeAllBtn(user, privs, tableName) : undefined
                            }
                            depth={3}
                          />
                        ))}
                      </TreeNode>
                    );
                  })}
                </TreeNode>
              );
            })}
          </TreeNode>
        );
      })}
      {confirmDialog}
    </div>
  );
}

// ─── Main Component ───────────────────────────────────────────────────

export function PrivilegeView({ dbSessionId }: PrivilegeViewProps) {
  const { t } = useI18n();
  const { definitions } = useConnectionCommands(dbSessionId);
  const supportsDropUser = hasCommand(definitions, 'drop_user');
  const [grants, setGrants] = useState<PrivilegeGrant[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<ViewMode>('by-user');
  const [sql, setSql] = useState('GRANT SELECT ON TABLE schema.table TO role;');
  const [running, setRunning] = useState(false);
  const [runMessage, setRunMessage] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [showGrant, setShowGrant] = useState(false);
  const [grantUser, setGrantUser] = useState<string | undefined>(undefined);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setGrants(await databaseCommands.getPrivileges(dbSessionId));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setGrants([]);
    } finally {
      setLoading(false);
    }
  }, [dbSessionId]);

  useEffect(() => {
    void load();
  }, [load]);

  const handleExecute = useCallback(async () => {
    if (!sql.trim()) return;
    setRunning(true);
    setRunMessage(null);
    try {
      await queryCommands.executeQuery(dbSessionId, sql);
      setRunMessage(t('privileges.executeOk'));
      void load();
    } catch (e) {
      setRunMessage(e instanceof Error ? e.message : String(e));
    } finally {
      setRunning(false);
    }
  }, [dbSessionId, load, sql, t]);

  const uniqueGrantees = useMemo(() => {
    return [...new Set(grants.map((g) => g.grantee))].sort();
  }, [grants]);

  const openGrantDialog = (forUser?: string) => {
    setGrantUser(forUser);
    setShowGrant(true);
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* Header */}
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-edge px-3">
        <span className="text-xs font-medium text-fg">{t('privileges.title')}</span>
        <div className="flex-1" />
        <div className="flex items-center rounded border border-edge bg-surface-alt">
          <button
            type="button"
            className={`px-2 py-0.5 text-[10px] font-medium transition-colors ${
              viewMode === 'by-user'
                ? 'bg-accent text-on-accent rounded-l'
                : 'text-fg-muted hover:text-fg'
            }`}
            onClick={() => setViewMode('by-user')}
          >
            {t('privileges.byUser')}
          </button>
          <button
            type="button"
            className={`px-2 py-0.5 text-[10px] font-medium transition-colors ${
              viewMode === 'by-object'
                ? 'bg-accent text-on-accent rounded-r'
                : 'text-fg-muted hover:text-fg'
            }`}
            onClick={() => setViewMode('by-object')}
          >
            {t('privileges.byObject')}
          </button>
        </div>
        <Button
          variant="ghost"
          className="h-7 w-7 !px-0"
          title={t('common.grantPrivileges')}
          onClick={() => openGrantDialog()}
        >
          <Plus className="h-3.5 w-3.5" />
        </Button>
        <Button variant="ghost" className="h-7 w-7 !px-0" onClick={() => void load()}>
          <RefreshCw className="h-3.5 w-3.5" />
        </Button>
      </div>

      {/* Content */}
      {loading && (
        <div className="flex items-center gap-2 p-3 text-xs text-fg-muted">
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          {t('common.loading')}
        </div>
      )}
      {error && <CopyableError message={error} className="p-3 text-xs text-red-400" />}

      {!loading && !error && viewMode === 'by-user' && (
        <ByUserView
          grants={grants}
          dbSessionId={dbSessionId}
          supportsDropUser={supportsDropUser}
          onRefresh={() => void load()}
          actionError={actionError}
          setActionError={setActionError}
        />
      )}
      {!loading && !error && viewMode === 'by-object' && (
        <ByObjectView
          grants={grants}
          dbSessionId={dbSessionId}
          onRefresh={() => void load()}
          actionError={actionError}
          setActionError={setActionError}
        />
      )}

      {/* SQL Input */}
      <div className="border-t border-edge p-3">
        <div className="mb-1.5 text-[11px] text-fg-muted">{t('privileges.sqlHint')}</div>
        <textarea
          className="h-16 w-full resize-y rounded border border-edge bg-surface-alt px-2.5 py-1.5 font-mono text-xs text-fg outline-none focus:border-accent"
          value={sql}
          onChange={(e) => setSql(e.target.value)}
        />
        <div className="mt-2 flex items-center gap-2">
          <Button
            variant="primary"
            className="h-7 gap-1 px-2 text-xs"
            disabled={running || !sql.trim()}
            onClick={() => void handleExecute()}
          >
            <Play className="h-3.5 w-3.5" />
            {t('query.execute')}
          </Button>
          {runMessage && <span className="text-[11px] text-fg-muted">{runMessage}</span>}
        </div>
      </div>

      {/* Grant Dialog */}
      {showGrant && (
        <GrantDialog
          dbSessionId={dbSessionId}
          users={uniqueGrantees}
          initialUser={grantUser}
          onGranted={() => void load()}
          onClose={() => setShowGrant(false)}
        />
      )}
    </div>
  );
}
