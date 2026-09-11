import {
  classifyRisk,
  type SqlRiskAssessment,
  type SqlRiskClassification,
  type SqlRiskFinding,
  type SqlStatementRisk,
} from '../../../lib/dangerousSql';

export type {
  SqlRiskAssessment,
  SqlRiskClassification,
  SqlRiskFinding,
  SqlRiskFindingType,
  SqlStatementRisk,
} from '../../../lib/dangerousSql';

/** Execution-context flags the gate cares about. */
export type ExecutionRiskMode = {
  readOnly: boolean;
  safeMode: boolean;
  isProduction: boolean;
  /** When true, confirmation is offered for high-risk/production SQL when Safe Mode is off. Default true. */
  confirmDangerous: boolean;
};

/** The subset of the mode relevant to the Host hard guard. */
export type HostGuardMode = {
  readOnly: boolean;
  safeMode: boolean;
};

export type ExecutionRiskDecision = {
  classification: SqlRiskClassification;
  findings: SqlRiskFinding[];
  statements: SqlStatementRisk[];
  /** True when the Host guard (readOnly/safeMode) would hard-block the SQL. */
  hardBlocked: boolean;
  hardBlockReason?: 'readOnly' | 'safeMode';
  /**
   * True when the gate must show a confirmation (production or non-safeMode
   * high-risk). Always false when the Host would hard-block.
   */
  needsConfirm: boolean;
  highRisk: boolean;
  confirmReasons: string[];
};

/**
 * Mirrors the Rust Host guard's `check_sql` decision so the frontend assessment
 * and the Host enforcement agree on the same SQL (shared fixture contract).
 */
export function hostGuardWouldBlock(sql: string, mode: HostGuardMode): boolean {
  const assessment = classifyRisk(sql);
  for (const stmt of assessment.statements) {
    if (mode.readOnly && stmt.strictWrite) return true;
    if (mode.safeMode && (stmt.verb === 'DROP' || stmt.verb === 'TRUNCATE')) return true;
    if (
      mode.safeMode &&
      (stmt.verb === 'UPDATE' || stmt.verb === 'DELETE') &&
      !stmt.hasTopLevelWhere
    ) {
      return true;
    }
    if (
      (mode.readOnly || mode.safeMode) &&
      stmt.classification === 'unknown' &&
      stmt.commentHidesWriteVerb
    ) {
      return true;
    }
  }
  return false;
}

/**
 * Full gate-facing risk assessment: classification + findings + whether the Host
 * hard-blocks and whether a confirmation is required.
 */
export function assessExecutionRisk(sql: string, mode: ExecutionRiskMode): ExecutionRiskDecision {
  const assessment: SqlRiskAssessment = classifyRisk(sql);

  let hardBlocked = false;
  let hardBlockReason: 'readOnly' | 'safeMode' | undefined;
  for (const stmt of assessment.statements) {
    if (hardBlocked) break;
    if (mode.readOnly && stmt.strictWrite) {
      hardBlocked = true;
      hardBlockReason = 'readOnly';
    } else if (mode.safeMode && (stmt.verb === 'DROP' || stmt.verb === 'TRUNCATE')) {
      hardBlocked = true;
      hardBlockReason = 'safeMode';
    } else if (
      mode.safeMode &&
      (stmt.verb === 'UPDATE' || stmt.verb === 'DELETE') &&
      !stmt.hasTopLevelWhere
    ) {
      hardBlocked = true;
      hardBlockReason = 'safeMode';
    } else if (
      (mode.readOnly || mode.safeMode) &&
      stmt.classification === 'unknown' &&
      stmt.commentHidesWriteVerb
    ) {
      hardBlocked = true;
      hardBlockReason = 'safeMode';
    }
  }

  const confirmReasons: string[] = [];
  if (mode.confirmDangerous) {
    if (mode.isProduction && assessment.classification !== 'read') {
      confirmReasons.push('production');
    }
    if (!mode.safeMode && assessment.hasHighRisk) {
      confirmReasons.push('high-risk');
    }
  }
  const needsConfirm = !hardBlocked && confirmReasons.length > 0;

  return {
    classification: assessment.classification,
    findings: assessment.findings,
    statements: assessment.statements,
    hardBlocked,
    hardBlockReason,
    needsConfirm,
    highRisk: assessment.hasHighRisk,
    confirmReasons,
  };
}
