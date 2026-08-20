import { lazy, Suspense, useEffect } from 'react';
import { getWindowKind } from './lib/windowKind';
import { mark } from './lib/startupTimer';
import { ErrorBoundary } from './components/ErrorBoundary';
import { WebContextMenuHost } from './components/ui/WebContextMenu';
import { WindowChromeFallback } from './components/WindowChromeFallback';
import {
  installDragSelectionGuard,
  installGlobalTextSelectionPolicy,
  installRightDragSelectionSuppressor,
} from './lib/globalTextSelection';

const MainPage = lazy(() =>
  import('./windows/main/MainPage').then((m) => {
    mark('MainPage chunk loaded');
    return { default: m.MainPage };
  }),
);
const NewConnectionWindow = lazy(() =>
  import('./windows/new-connection/NewConnectionWindow').then((m) => {
    mark('NewConnectionWindow chunk loaded');
    return { default: m.NewConnectionWindow };
  }),
);
const DataSyncWindow = lazy(() =>
  import('./windows/data-sync/DataSyncWindow').then((m) => {
    mark('DataSyncWindow chunk loaded');
    return { default: m.DataSyncWindow };
  }),
);
const SchemaDiffWindow = lazy(() =>
  import('./windows/schema-diff/SchemaDiffWindow').then((m) => {
    mark('SchemaDiffWindow chunk loaded');
    return { default: m.SchemaDiffWindow };
  }),
);
const BackupWindow = lazy(() =>
  import('./windows/backup/BackupWindow').then((m) => {
    mark('BackupWindow chunk loaded');
    return { default: m.BackupWindow };
  }),
);
const DocsWindow = lazy(() =>
  import('./windows/docs/DocsWindow').then((m) => {
    mark('DocsWindow chunk loaded');
    return { default: m.DocsWindow };
  }),
);

const windowKind = getWindowKind();
mark(`windowKind resolved: "${windowKind}"`);

function WindowContent() {
  useEffect(() => {
    mark('window component mounted');
  }, []);

  switch (windowKind) {
    case 'new-connection':
      return <NewConnectionWindow />;
    case 'data-sync':
      return <DataSyncWindow />;
    case 'schema-diff':
      return <SchemaDiffWindow />;
    case 'backup':
      return <BackupWindow />;
    case 'docs':
      return <DocsWindow />;
    case 'main':
    default:
      return <MainPage />;
  }
}

export default function App() {
  useEffect(() => {
    const stopSelectAll = installGlobalTextSelectionPolicy();
    const stopRightDrag = installRightDragSelectionSuppressor();
    const stopDragGuard = installDragSelectionGuard();
    return () => {
      stopSelectAll();
      stopRightDrag();
      stopDragGuard();
    };
  }, []);

  return (
    <ErrorBoundary>
      <Suspense fallback={<WindowChromeFallback />}>
        <WindowContent />
      </Suspense>
      <WebContextMenuHost />
    </ErrorBoundary>
  );
}
