import { useCallback, useState } from 'react';

type FeedbackKind = 'summary' | 'sql' | 'config' | null;

export function useSchemaDiffClipboardFeedback() {
  const [feedback, setFeedback] = useState<FeedbackKind>(null);
  const showFeedback = useCallback((kind: FeedbackKind) => {
    setFeedback(kind);
    window.setTimeout(() => setFeedback(null), 2000);
  }, []);

  return [feedback, showFeedback] as const;
}
