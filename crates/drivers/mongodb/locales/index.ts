/**
 * MongoDB driver locale pack — self-registration entry point.
 *
 * Driver translations are owned by this package: importing this module feeds
 * the app's English and Simplified Chinese dictionaries into the ONE shared i18n registry in
 * `@datazen/ui` (`registerTranslations`). The host knows nothing about which
 * languages a driver ships, and drivers never call `setLocale` (host-only).
 *
 * Side-effect module: exports no runtime value. Imported from the driver UI
 * entry module (`ui/meta.ts`), which `src/extensions/generated.ts` imports on
 * every build where this driver is selected, so the pack is registered as soon
 * as the driver UI is loaded. Idempotent by design — `registerTranslations`
 * merges keys, repeated imports are harmless.
 *
 * Only `en` and `zh-CN` are bundled. Other locale dictionaries remain
 * source-only and must not be imported here.
 */
import { registerTranslations } from '@datazen/ui';
import en from './en';
import zhCN from './zh-CN';

registerTranslations({
  en,
  'zh-CN': zhCN,
});

export {};
