/**
 * Window chrome extension slot (form-split F2).
 *
 * Shared window pages render the SLOT, never a host chrome component: the
 * desktop app registers `TitleBar` here at assembly time; a future web form
 * simply never registers one (the browser ships its own chrome). Same
 * registration pattern as `setTableSchemaProvider`, expressed through the
 * extension registry so React consumers stay tear-free via `useExtension`.
 */

import { createElement, type ReactNode } from 'react';
import { createExtensionPoint, extensionRegistry } from './extensionPoints';
import { useExtension } from './useExtension';

export interface WindowChromeProps {
  title?: ReactNode;
  leftContent?: ReactNode;
  rightContent?: ReactNode;
}

/** Chrome components are plain function components (no hooks of their own beyond React's). */
export type WindowChromeComponent = (props: WindowChromeProps) => ReactNode;

export const windowChromeEP = createExtensionPoint<WindowChromeComponent | null>({
  id: 'window.chrome',
  name: 'Window Chrome',
  description:
    'The window title bar / controls slot. The desktop app registers its TitleBar; a web form leaves it null.',
  getDefault: () => null,
});

/** Register (or clear, with `null`) the window chrome component. Returns an unregister fn. */
export function registerWindowChrome(chrome: WindowChromeComponent | null): () => void {
  if (chrome === null) {
    extensionRegistry.unregister(windowChromeEP);
    return () => {};
  }
  return extensionRegistry.register(windowChromeEP, chrome);
}

/** Current chrome component or `null` when no app registered one. */
export function useWindowChrome(): WindowChromeComponent | null {
  return useExtension(windowChromeEP);
}

/** Render the registered chrome (nothing when unregistered — web form). */
export function WindowChromeSlot(props: WindowChromeProps): ReactNode {
  const Chrome = useWindowChrome();
  return Chrome ? createElement(Chrome, props) : null;
}
