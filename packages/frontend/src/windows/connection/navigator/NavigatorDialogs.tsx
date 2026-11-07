import { Button } from '../../../components/ui/Button';
import { Dialog } from '../../../components/ui/Dialog';
import { Input } from '../../../components/ui/Input';
import { ObjectFilterDialog } from '../../../components/connection/ObjectFilterDialog';
import type { I18nKey } from '../../../locales';
import type { ConnectionConfig } from '../../../types';

export interface NavigatorDialogsProps {
  t: (key: I18nKey) => string;
  newGroupDialogOpen: boolean;
  setNewGroupDialogOpen: (open: boolean) => void;
  newGroupName: string;
  setNewGroupName: (name: string) => void;
  addGroup: (name: string) => Promise<void>;
  renamingGroup: string | null;
  setRenamingGroup: (name: string | null) => void;
  renameValue: string;
  setRenameValue: (value: string) => void;
  renameGroup: (oldName: string, newName: string) => Promise<void>;
  objectFilterConn: ConnectionConfig | null;
  setObjectFilterConn: (conn: ConnectionConfig | null) => void;
  saveConnection: (config: ConnectionConfig) => Promise<void>;
  refreshConnection: (connectionId: string) => Promise<void>;
  confirmDeleteGroupDialog: React.ReactNode;
  confirmDropDatabaseDialog: React.ReactNode;
  confirmDropSchemaDialog: React.ReactNode;
  confirmDropRelationDialog: React.ReactNode;
  confirmTruncateTableDialog: React.ReactNode;
}

export function NavigatorDialogs({
  t,
  newGroupDialogOpen,
  setNewGroupDialogOpen,
  newGroupName,
  setNewGroupName,
  addGroup,
  renamingGroup,
  setRenamingGroup,
  renameValue,
  setRenameValue,
  renameGroup,
  objectFilterConn,
  setObjectFilterConn,
  saveConnection,
  refreshConnection,
  confirmDeleteGroupDialog,
  confirmDropDatabaseDialog,
  confirmDropSchemaDialog,
  confirmDropRelationDialog,
  confirmTruncateTableDialog,
}: NavigatorDialogsProps) {
  return (
    <>
      <Dialog
        open={newGroupDialogOpen}
        title={t('common.newGroup')}
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
              if (newGroupName.trim()) void addGroup(newGroupName.trim());
              setNewGroupDialogOpen(false);
            }
          }}
          placeholder={t('main.groupNamePlaceholder')}
          className="text-sm"
          autoCapitalize="off"
        />
      </Dialog>

      <Dialog
        open={renamingGroup !== null}
        title={t('main.ctx.renameGroup')}
        onClose={() => setRenamingGroup(null)}
        className="max-w-sm"
        footer={
          <>
            <Button variant="ghost" onClick={() => setRenamingGroup(null)}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              onClick={() => {
                if (renamingGroup !== null && renameValue.trim()) {
                  void renameGroup(renamingGroup, renameValue.trim());
                }
                setRenamingGroup(null);
              }}
            >
              {t('common.ok')}
            </Button>
          </>
        }
      >
        <Input
          autoFocus
          value={renameValue}
          onChange={(e) => setRenameValue(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              if (renamingGroup !== null && renameValue.trim()) {
                void renameGroup(renamingGroup, renameValue.trim());
              }
              setRenamingGroup(null);
            }
          }}
          placeholder={t('main.groupNamePlaceholder')}
          className="text-sm"
          autoCapitalize="off"
        />
      </Dialog>

      <ObjectFilterDialog
        open={objectFilterConn != null}
        connection={objectFilterConn}
        onClose={() => setObjectFilterConn(null)}
        onSave={async (config) => {
          await saveConnection(config);
          await refreshConnection(config.id);
        }}
      />

      {confirmDeleteGroupDialog}
      {confirmDropDatabaseDialog}
      {confirmDropSchemaDialog}
      {confirmDropRelationDialog}
      {confirmTruncateTableDialog}
    </>
  );
}
