import { useState, type Dispatch, type SetStateAction } from 'react';

/**
 * User-visible feedback of the Data Sync window: the error dialog, the
 * status bar line and the progress text of a running execution.
 */
export interface DataSyncFeedback {
  errorMsg: string;
  setErrorMsg: Dispatch<SetStateAction<string>>;
  errorOpen: boolean;
  setErrorOpen: Dispatch<SetStateAction<boolean>>;
  statusMsg: string;
  setStatusMsg: Dispatch<SetStateAction<string>>;
  executeProgress: string;
  setExecuteProgress: Dispatch<SetStateAction<string>>;
}

export function useDataSyncFeedback(): DataSyncFeedback {
  const [errorMsg, setErrorMsg] = useState('');
  const [errorOpen, setErrorOpen] = useState(false);
  const [statusMsg, setStatusMsg] = useState('');
  const [executeProgress, setExecuteProgress] = useState('');

  return {
    errorMsg,
    setErrorMsg,
    errorOpen,
    setErrorOpen,
    statusMsg,
    setStatusMsg,
    executeProgress,
    setExecuteProgress,
  };
}
