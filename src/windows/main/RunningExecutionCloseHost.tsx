import { useEffect } from 'react';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { setRunningExecutionGate } from '../../stores/runningExecutionClose';
import { t } from '../../locales/t';

/**
 * The prompt behind "close a tab whose query is still running".
 *
 * The store cannot open a dialog, so it asks a gate; this component installs
 * that gate for as long as the main window lives. Mounting it at the workspace
 * root (rather than inside one connection's content view) is deliberate: tabs
 * are also removed when a whole connection is closed, which can happen while a
 * different page is on screen, and a prompt nobody can render would leave the
 * close waiting forever.
 *
 * The dialog is one at a time by construction: `useConfirmDialog` holds a single
 * pending request, and `askRunningExecutionGate` queues callers so a "close all"
 * over several running tabs cannot overwrite an unanswered prompt.
 */
export function RunningExecutionCloseHost(): React.ReactNode {
  const [confirm, dialog] = useConfirmDialog();

  useEffect(() => {
    setRunningExecutionGate((request) =>
      confirm({
        kind: 'warning',
        title: t('query.session.closeTabActiveExecutionTitle'),
        message:
          request.runningPanes > 1
            ? t('query.session.closeTabActiveExecutionMany', { count: request.runningPanes })
            : t('query.session.closeTabActiveExecution'),
        confirmLabel: t('query.session.closeTabActiveExecutionConfirm'),
        badge: request.title,
        codePreview: request.sql,
      }),
    );
    return () => setRunningExecutionGate(null);
  }, [confirm]);

  return dialog;
}
