import { lazy, Suspense, useEffect } from 'react';
import { getWindowKind } from './lib/windowKind';
import { mark } from './lib/startupTimer';

const MainWindow = lazy(() =>
  import('./windows/main/MainWindow').then((m) => {
    mark('MainWindow chunk loaded');
    return { default: m.MainWindow };
  }),
);
const NewConnectionWindow = lazy(() =>
  import('./windows/new-connection/NewConnectionWindow').then((m) => {
    mark('NewConnectionWindow chunk loaded');
    return { default: m.NewConnectionWindow };
  }),
);
const ConnectionWindow = lazy(() =>
  import('./windows/connection/ConnectionWindow').then((m) => {
    mark('ConnectionWindow chunk loaded');
    return { default: m.ConnectionWindow };
  }),
);
const SettingsWindow = lazy(() =>
  import('./windows/settings/SettingsWindow').then((m) => {
    mark('SettingsWindow chunk loaded');
    return { default: m.SettingsWindow };
  }),
);
const DataSyncWindow = lazy(() =>
  import('./windows/data-sync/DataSyncWindow').then((m) => {
    mark('DataSyncWindow chunk loaded');
    return { default: m.DataSyncWindow };
  }),
);
const BackupWindow = lazy(() =>
  import('./windows/backup/BackupWindow').then((m) => {
    mark('BackupWindow chunk loaded');
    return { default: m.BackupWindow };
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
    case 'connection':
      return <ConnectionWindow />;
    case 'settings':
      return <SettingsWindow />;
    case 'data-sync':
      return <DataSyncWindow />;
    case 'backup':
      return <BackupWindow />;
    case 'main':
    default:
      return <MainWindow />;
  }
}

export default function App() {
  return (
    <Suspense fallback={null}>
      <WindowContent />
    </Suspense>
  );
}
