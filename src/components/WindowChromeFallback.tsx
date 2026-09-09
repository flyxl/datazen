import { TitleBar } from './TitleBar';

/**
 * Overlay title-bar windows have no native drag area until React mounts.
 * Use as Suspense/error chrome so the window can be moved and closed.
 */
export function WindowChromeFallback() {
  return (
    <div className="flex h-screen min-h-0 flex-col bg-surface" data-testid="window-chrome-fallback">
      <TitleBar />
      <div className="flex flex-1 items-center justify-center">
        <div className="h-6 w-6 animate-spin rounded-full border-2 border-accent border-t-transparent" />
      </div>
    </div>
  );
}
