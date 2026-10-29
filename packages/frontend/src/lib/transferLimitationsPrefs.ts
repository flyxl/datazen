import { createDismissPrefs } from './createDismissPrefs';

export const {
  isDismissed: isTransferLimitationsDismissed,
  setDismissed: setTransferLimitationsDismissed,
  clearDismissed: clearTransferLimitationsDismissed,
} = createDismissPrefs('datazen:transfer-limitations-dismissed');
