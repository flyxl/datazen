/**
 * SafeCompartmentWrapper — circuit-breaker guard for privileged decorator calls.
 *
 * When a Pro extension decorator throws, the wrapper logs the error, unregisters
 * the extension point (falling back to host defaults), and returns a safe fallback
 * value so the editor keeps working.
 *
 */

import type { ExtensionPoint } from './extensionPoints';
import { extensionRegistry } from './extensionPoints';

export interface SafeCompartmentOptions {
  point: ExtensionPoint<unknown>;
  featureName: string;
  onCircuitBreak?: (error: unknown) => void;
}

/**
 * Execute a privileged extension factory inside a try/catch circuit breaker.
 * On failure: log, unregister the EP, return fallback.
 */
export function SafeCompartmentWrapper<T>(
  options: SafeCompartmentOptions,
  fn: () => T,
  fallback: T,
): T {
  try {
    return fn();
  } catch (err) {
    console.error(
      `[SafeCompartmentWrapper] ${options.featureName} crashed, auto-falling back to core:`,
      err,
    );
    extensionRegistry.unregister(options.point);
    options.onCircuitBreak?.(err);
    return fallback;
  }
}
