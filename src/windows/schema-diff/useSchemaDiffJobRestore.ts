import { useEffect, type Dispatch, type SetStateAction } from 'react';
import {
  dialectSupportsTransactionalDdl,
  type SchemaDiffDeployResult,
  type SchemaDiffPlan,
  type SchemaDiffPrepareEnvelope,
} from '../../commands/schemaDiff';
import type { SchemaDiffWizardStep } from './SchemaDiffWizardProgress';
import type { SchemaDiffTrackedJob } from './useSchemaDiffJobLifecycle';
import type { SchemaDiffJobDetails } from '../../commands/schemaDiff';

export function useSchemaDiffJobRestore({
  currentJob,
  latestApply,
  sourceConnectionId,
  targetConnectionId,
  setPlan,
  setPlanMeta,
  setUseTransaction,
  setStep,
  setDeployResult,
}: {
  currentJob: SchemaDiffTrackedJob | null;
  latestApply: SchemaDiffJobDetails | null;
  sourceConnectionId: string | null;
  targetConnectionId: string | null;
  setPlan: Dispatch<SetStateAction<SchemaDiffPlan | null>>;
  setPlanMeta: Dispatch<SetStateAction<SchemaDiffPrepareEnvelope | null>>;
  setUseTransaction: Dispatch<SetStateAction<boolean>>;
  setStep: Dispatch<SetStateAction<SchemaDiffWizardStep>>;
  setDeployResult: Dispatch<SetStateAction<SchemaDiffDeployResult | null>>;
}) {
  useEffect(() => {
    const prepared = currentJob?.details?.prepared;
    if (
      !prepared ||
      prepared.sourceConnectionId !== sourceConnectionId ||
      prepared.targetConnectionId !== targetConnectionId
    )
      return;
    setPlan(prepared.plan);
    setPlanMeta(prepared);
    setUseTransaction(dialectSupportsTransactionalDdl(prepared.plan.targetDialect));
    setStep((current) => (current === 'endpoints' ? 'plan' : current));
  }, [
    currentJob,
    sourceConnectionId,
    targetConnectionId,
    setPlan,
    setPlanMeta,
    setUseTransaction,
    setStep,
  ]);

  useEffect(() => {
    if (currentJob?.details?.deployResult) setDeployResult(currentJob.details.deployResult);
  }, [currentJob, setDeployResult]);

  useEffect(() => {
    if (latestApply?.deployResult) setDeployResult(latestApply.deployResult);
  }, [latestApply, setDeployResult]);
}
