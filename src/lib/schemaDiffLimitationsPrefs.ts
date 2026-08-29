import { createDismissPrefs } from './createDismissPrefs';

export const {
  isDismissed: isSchemaDiffLimitationsDismissed,
  setDismissed: setSchemaDiffLimitationsDismissed,
  clearDismissed: clearSchemaDiffLimitationsDismissed,
} = createDismissPrefs('datazen:schema-diff-limitations-dismissed');
