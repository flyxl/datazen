import { TitleBar } from './TitleBar';

/**
 * Overlay title-bar windows have no native drag area until React mounts.
 * Use as Suspense/error chrome so the window can be moved and closed.
 */
export function WindowChromeFallback() {
  return (
    <div className="flex h-screen min-h-0 flex-col bg-surface" data-testid="window-chrome-fallback">
      <TitleBar />
    </div>
  );
}
