/**
 * Host-side alias for the single @datazen/ui i18n hook.
 *
 * Kept as the canonical import path for the 300+ host/driver consumer files.
 * There is deliberately no host-local implementation — lookup, fallback and
 * interpolation live only in packages/ui/src/i18n.ts.
 */
import '../locales'; // side effect: register the eager host dictionaries (driver/extension packs self-register)
export { useI18n, type I18nParams } from '@datazen/ui';
