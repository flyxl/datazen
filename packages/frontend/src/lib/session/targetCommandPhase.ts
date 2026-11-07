/** Fixed boundaries only; callbacks never receive request or error content. */
export type TargetCommandPhase = 'profiles' | 'submission-token' | 'target-admission' | 'execution-view' | 'artifact-read' | 'dispose';
export type TargetCommandPhaseHook = (phase: TargetCommandPhase) => void;
