/**
 * Extension module lifecycle contracts — activate/deactivate with safe disposal.
 *
 * @see docs/todo/hot-plug-prd.md §3.1–3.2
 */

import type { ExtensionPoint } from './extensionPoints';
import { extensionRegistry } from './extensionPoints';

/** Resource that must be released when an extension module is unloaded. */
export interface Disposable {
  dispose(): void;
}

/** A subscription handle returned by register/subscribe APIs. */
export type ExtensionSubscription = (() => void) | Disposable;

/** Context passed to ExtensionModule.activate — holds lifecycle subscriptions. */
export interface ExtensionContext {
  /** Side-effect handles disposed in LIFO order on unload. */
  subscriptions: ExtensionSubscription[];
  /** Stable extension identifier (e.g. 'sql-editor-pro'). */
  extensionId: string;
}

/** Standard ESM extension module entry contract. */
export interface ExtensionModule {
  /** Activate: register extension points and wire listeners. */
  activate(context: ExtensionContext): void | Promise<void>;
  /** Optional module-level cleanup (cache pools, timers, etc.). */
  deactivate?(): void | Promise<void>;
}

function disposeSubscription(sub: ExtensionSubscription): void {
  if (typeof sub === 'function') {
    sub();
  } else {
    sub.dispose();
  }
}

/**
 * Host-controlled dynamic extension loader.
 * Tracks active modules and disposes subscriptions in strict LIFO order on unload.
 */
export class HostExtensionLoader {
  private activeExtensions = new Map<
    string,
    {
      context: ExtensionContext;
      deactivate?: () => void | Promise<void>;
    }
  >();

  /** Load and activate an extension module (unloads any prior instance with same id). */
  async load(extensionId: string, module: ExtensionModule): Promise<void> {
    if (this.activeExtensions.has(extensionId)) {
      await this.unload(extensionId);
    }

    const context: ExtensionContext = {
      extensionId,
      subscriptions: [],
    };

    await module.activate(context);

    this.activeExtensions.set(extensionId, {
      context,
      deactivate: module.deactivate,
    });
  }

  /** Dynamically import and activate an ESM bundle entry. */
  async loadFromUrl(extensionId: string, entryUrl: string): Promise<void> {
    const mod: ExtensionModule = await import(/* @vite-ignore */ entryUrl);
    await this.load(extensionId, mod);
  }

  /** Unload: call deactivate, then dispose all subscriptions in LIFO order. */
  async unload(extensionId: string): Promise<void> {
    const active = this.activeExtensions.get(extensionId);
    if (!active) return;

    try {
      if (active.deactivate) {
        await active.deactivate();
      }
    } finally {
      const subs = active.context.subscriptions;
      for (let i = subs.length - 1; i >= 0; i--) {
        try {
          disposeSubscription(subs[i]);
        } catch (err) {
          console.error('[ExtensionLoader] Error disposing resource:', err);
        }
      }
      this.activeExtensions.delete(extensionId);
    }
  }

  isLoaded(extensionId: string): boolean {
    return this.activeExtensions.has(extensionId);
  }

  getContext(extensionId: string): ExtensionContext | undefined {
    return this.activeExtensions.get(extensionId)?.context;
  }

  /** Unload all active extensions (primarily for tests). */
  async reset(): Promise<void> {
    for (const id of [...this.activeExtensions.keys()]) {
      await this.unload(id);
    }
  }
}

/** Global singleton extension loader. */
export const hostExtensionLoader = new HostExtensionLoader();

/**
 * Register an extension point implementation and track the unsubscribe handle
 * on the module context for automatic cleanup on unload.
 */
export function registerExtensionPoint<T>(
  context: ExtensionContext,
  point: ExtensionPoint<T>,
  impl: T,
): void {
  const unsub = extensionRegistry.register(point, impl);
  context.subscriptions.push(unsub);
}
