import { useEffect, type Dispatch, type SetStateAction } from 'react';
import type { SchemaDiffDeployResult } from '../../commands/schemaDiff';
import type { SchemaDiffTrackedJob } from './useSchemaDiffJobLifecycle';

/** Historical jobs stay in the status panel. They never overwrite a new
 * wizard's endpoint scope, reviewed plan or execution result. */
export function useSchemaDiffJobRestore({
  currentJob,
  setDeployResult,
}: {
  currentJob: SchemaDiffTrackedJob | null;
  setDeployResult: Dispatch<SetStateAction<SchemaDiffDeployResult | null>>;
}) {
  useEffect(() => {
    if (currentJob?.submittedLocally && currentJob.details?.deployResult) {
      setDeployResult(currentJob.details.deployResult);
    }
  }, [currentJob, setDeployResult]);
}
