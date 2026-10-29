import { useCallback, useState } from 'react';
import { ResultMessageDialog } from '../ui/ResultMessageDialog';
import { useI18n } from '../../hooks/useI18n';
import { useConnectionStore } from '../../stores/connectionStore';
import { formatConnectionImportSuccess } from '../../lib/connectionShareError';
import { closeConnectionShareDialog, useConnectionShareStore } from '../../lib/connectionShare';
import { ConnectionShareDialog } from './ConnectionShareDialog';

/** Global dialog wired to {@link useConnectionShareStore}. Mount once in MainPage. */
export function ConnectionShareDialogHost() {
  const { t } = useI18n();
  const open = useConnectionShareStore((s) => s.open);
  const mode = useConnectionShareStore((s) => s.mode);
  const importSource = useConnectionShareStore((s) => s.importSource);
  const fetchConnections = useConnectionStore((s) => s.fetchConnections);
  const fetchGroups = useConnectionStore((s) => s.fetchGroups);

  const [messageDialogOpen, setMessageDialogOpen] = useState(false);
  const [messageDialogText, setMessageDialogText] = useState('');
  const [messageDialogKind, setMessageDialogKind] = useState<'error' | 'success'>('error');

  const showMessageDialog = useCallback((text: string, kind: 'error' | 'success') => {
    setMessageDialogText(text);
    setMessageDialogKind(kind);
    setMessageDialogOpen(true);
  }, []);

  const handleRefresh = useCallback(() => {
    void fetchConnections();
    void fetchGroups();
  }, [fetchConnections, fetchGroups]);

  return (
    <>
      <ConnectionShareDialog
        open={open}
        mode={mode}
        importSource={importSource}
        onClose={closeConnectionShareDialog}
        onExportSuccess={(count) => {
          closeConnectionShareDialog();
          showMessageDialog(t('connShare.exportSuccess', { count }), 'success');
        }}
        onImportSuccess={(result) => {
          closeConnectionShareDialog();
          showMessageDialog(formatConnectionImportSuccess(result, t), 'success');
          handleRefresh();
        }}
        onError={(message) => {
          closeConnectionShareDialog();
          showMessageDialog(message, 'error');
        }}
      />
      <ResultMessageDialog
        open={messageDialogOpen}
        kind={messageDialogKind}
        message={messageDialogText}
        onClose={() => setMessageDialogOpen(false)}
      />
    </>
  );
}
