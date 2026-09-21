/**
 * Redis driver locale pack — self-registration entry point.
 *
 * Driver translations are owned by this package: importing this module feeds
 * every shipped dictionary into the ONE shared i18n registry in
 * `@datazen/ui` (`registerTranslations`). The host knows nothing about which
 * languages a driver ships, and drivers never call `setLocale` (host-only).
 *
 * Side-effect module: exports no runtime value. Imported from the driver UI
 * entry module (`ui/shared/meta.ts`), which `src/extensions/generated.ts`
 * imports on every build where this driver is selected, so the pack is
 * registered as soon as the driver UI is loaded. Idempotent by design —
 * `registerTranslations` merges keys, repeated imports are harmless.
 *
 * Locale codes must match the host's `src/locales/builtinLocales.ts`
 * literals exactly (`pt-BR` / `zh-CN` / `zh-TW` are hyphenated).
 */
import { registerTranslations } from '@datazen/ui';
import de from './de';
import en from './en';
import es from './es';
import fr from './fr';
import ja from './ja';
import ko from './ko';
import ptBR from './pt-BR';
import ru from './ru';
import zhCN from './zh-CN';
import zhTW from './zh-TW';

registerTranslations({
  en,
  'zh-CN': zhCN,
  'zh-TW': zhTW,
  de,
  es,
  fr,
  ja,
  ko,
  'pt-BR': ptBR,
  ru,
});

export {};
