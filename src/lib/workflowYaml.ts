import YAML from 'yaml';

/**
 * Extract YAML code blocks from an AI response that look like workflow definitions.
 * Returns an array of raw YAML strings (without the fence markers).
 */
export function extractWorkflowYaml(text: string): string[] {
  const regex = /```ya?ml\n([\s\S]*?)```/g;
  const blocks: string[] = [];
  let match: RegExpExecArray | null;
  while ((match = regex.exec(text)) !== null) {
    const content = match[1].trim();
    if (looksLikeWorkflow(content)) {
      blocks.push(content);
    }
  }
  return blocks;
}

function looksLikeWorkflow(yaml: string): boolean {
  // A v2 definition may carry only `blocks:` — its steps live inside the blocks.
  const hasBody = /\bsteps\s*:/.test(yaml) || /\bblocks\s*:/.test(yaml);
  return hasBody && (/\bid\s*:/.test(yaml) || /\bname\s*:/.test(yaml));
}

/**
 * Parse a YAML string into a plain object.
 * Throws on invalid YAML.
 */
export function parseWorkflowYaml(yaml: string): Record<string, unknown> {
  const parsed = YAML.parse(yaml);
  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
    throw new Error('YAML must be an object');
  }
  return parsed as Record<string, unknown>;
}

/**
 * Validate that a parsed workflow object has the required fields.
 * Returns the name of the first missing field, or null if valid.
 *
 * When `connections` is supplied, also enforces the database rule that the
 * workflow form promises: for a multi-database connection that has no default
 * database of its own, every data-operation step must resolve a database —
 * from its own `database`, its `input.database`, or the workflow-level
 * `database`. Without this the form showed a dropdown labelled "required"
 * that nothing actually enforced, and the workflow failed at run time with a
 * misleading `relation "..." does not exist`.
 *
 * `connections` is optional so the pure shape check stays usable by callers
 * that have no connection context (and keeps existing callers working).
 */
export function validateWorkflowFields(
  obj: Record<string, unknown>,
  connections?: WorkflowValidationConnection[],
): string | null {
  if (!obj.id || typeof obj.id !== 'string') return 'id';
  if (!obj.name || typeof obj.name !== 'string') return 'name';
  // A v2 definition may hold only `blocks`; its steps live inside them. What
  // makes a definition empty is having neither, not lacking top-level steps.
  if (!hasEntries(obj.steps) && !hasEntries(obj.blocks)) return 'steps';
  const missing = findStepMissingDatabase(obj, connections);
  if (missing) return missing;
  return null;
}

function hasEntries(value: unknown): boolean {
  return Array.isArray(value) && value.length > 0;
}

/** Minimal connection shape the database rule needs. */
export interface WorkflowValidationConnection {
  id: string;
  /** True when this driver is multi-db AND the connection pins no database of
   *  its own, so a step must name one. Computed by the caller from
   *  `DB_REGISTRY` (see `connectionAllowsMultiDb` in
   *  `windows/workflow/WorkflowForm.tsx`) so this module stays free of
   *  driver-metadata imports and stays cheap to unit test. */
  requiresExplicitDatabase: boolean;
}

function findStepMissingDatabase(
  obj: Record<string, unknown>,
  connections: WorkflowValidationConnection[] | undefined,
): string | null {
  if (!connections || connections.length === 0) return null;
  const byId = new Map(connections.map((c) => [c.id, c]));
  const workflowDatabase = typeof obj.database === 'string' ? obj.database.trim() : '';
  const workflowConnection = typeof obj.connection === 'string' ? obj.connection.trim() : '';
  const blocks = Array.isArray(obj.blocks) ? obj.blocks : [];

  // Top-level steps and block steps read the same four-level chain: a block's
  // own target sits directly above its steps, and the workflow default stays
  // the fallback below that. A blank value counts as undeclared at either
  // level, exactly like the backend normalizer.
  const scopes: { label: string; steps: unknown[]; connection: string; database: string }[] = [];
  if (Array.isArray(obj.steps)) {
    scopes.push({
      label: 'steps',
      steps: obj.steps,
      connection: workflowConnection,
      database: workflowDatabase,
    });
  }
  blocks.forEach((block: unknown, index: number) => {
    if (!isRecord(block)) return;
    scopes.push({
      label: `blocks[${index}].steps`,
      steps: Array.isArray(block.steps) ? block.steps : [],
      connection: textOf(block.connection) || workflowConnection,
      database: textOf(block.database) || workflowDatabase,
    });
  });

  for (const scope of scopes) {
    for (let i = 0; i < scope.steps.length; i++) {
      const step = scope.steps[i];
      if (!isRecord(step)) continue;
      const type = step.type;
      if (type !== 'query' && type !== 'command') continue;

      const connId = textOf(step.connection) || scope.connection;
      const conn = byId.get(connId);
      if (!conn?.requiresExplicitDatabase) continue;

      // A query step names its target in `database`; a command step carries it
      // in `input.database`, since `WorkflowStep::Command` has no such field.
      const stepDatabase =
        type === 'query'
          ? textOf(step.database)
          : textOf(isRecord(step.input) ? step.input.database : undefined);

      if (stepDatabase || scope.database) continue;
      return `${scope.label}[${i}].database`;
    }
  }
  return null;
}

function textOf(value: unknown): string {
  return typeof value === 'string' ? value.trim() : '';
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isWorkflowVariable(value: unknown): boolean {
  if (!isRecord(value)) return false;
  return typeof value.name === 'string' && typeof value.varType === 'string';
}

function isWorkflowStep(value: unknown): boolean {
  if (!isRecord(value)) return false;
  return typeof value.id === 'string' && typeof value.type === 'string';
}

/**
 * Narrow the v2 `blocks` array. Every rejection here mirrors what the backend
 * refuses to execute, so a bad block fails in the form instead of mid-run:
 * unknown block type, empty id, duplicate id, empty steps, non-boolean
 * `parallel`, or a `retry` that is not `{ block: <non-negative integer> }`.
 * Deliberately not duplicated here: cross-step target conflicts and plan
 * safety — those need the resolved connection and a dialect parser.
 */
function parseWorkflowBlocks(value: unknown): import('../types').WorkflowBlock[] {
  if (value === undefined) return [];
  if (!Array.isArray(value)) {
    throw new Error('Invalid workflow field: blocks must be an array of block objects');
  }
  const seen = new Set<string>();
  return value.map((raw, index) => {
    if (!isRecord(raw)) {
      throw new Error(`Invalid workflow field: blocks[${index}] must be an object`);
    }
    if (typeof raw.id !== 'string' || raw.id.trim() === '') {
      throw new Error(`Invalid workflow field: blocks[${index}].id must be a non-empty string`);
    }
    if (seen.has(raw.id)) {
      throw new Error(`Invalid workflow field: blocks[${index}].id duplicates block '${raw.id}'`);
    }
    seen.add(raw.id);
    if (raw.type !== 'sessionBlock' && raw.type !== 'transactionBlock') {
      throw new Error(
        `Invalid workflow field: blocks[${index}].type must be sessionBlock or transactionBlock`,
      );
    }
    if (!Array.isArray(raw.steps) || raw.steps.length === 0 || !raw.steps.every(isWorkflowStep)) {
      throw new Error(
        `Invalid workflow field: blocks[${index}].steps must be a non-empty array of step objects`,
      );
    }
    if (raw.parallel !== undefined && typeof raw.parallel !== 'boolean') {
      throw new Error(`Invalid workflow field: blocks[${index}].parallel must be a boolean`);
    }
    const retry = raw.retry;
    if (retry !== undefined) {
      if (
        !isRecord(retry) ||
        typeof retry.block !== 'number' ||
        !Number.isInteger(retry.block) ||
        retry.block < 0
      ) {
        throw new Error(
          `Invalid workflow field: blocks[${index}].retry must be { block: <non-negative integer> }`,
        );
      }
    }
    return {
      id: raw.id,
      type: raw.type,
      connection: typeof raw.connection === 'string' ? raw.connection : undefined,
      database: typeof raw.database === 'string' ? raw.database : undefined,
      parallel: raw.parallel === true,
      steps: raw.steps as import('../types').WorkflowStep[],
      retry: isRecord(retry) ? { block: retry.block as number } : undefined,
    };
  });
}

/**
 * Runtime guard: narrow a validated YAML object to WorkflowDefinition.
 * Throws when shape does not match the IPC contract.
 */
export function parseValidatedWorkflowDefinition(
  obj: Record<string, unknown>,
): import('../types').WorkflowDefinition {
  const missing = validateWorkflowFields(obj);
  if (missing) {
    throw new Error(`Missing required workflow field: ${missing}`);
  }

  const description = obj.description;
  if (description !== undefined && typeof description !== 'string') {
    throw new Error('Invalid workflow field: description must be a string');
  }

  const variables = obj.variables;
  if (variables !== undefined) {
    if (!Array.isArray(variables) || !variables.every(isWorkflowVariable)) {
      throw new Error('Invalid workflow field: variables must be an array of variable objects');
    }
  }

  // A v2 definition may declare only `blocks`; v1 requires top-level steps.
  // `steps: []` alongside `blocks` is a legal v2 no-op, and the
  // neither-steps-nor-blocks case already failed in `validateWorkflowFields`.
  const steps = obj.steps;
  if (steps !== undefined && (!Array.isArray(steps) || !steps.every(isWorkflowStep))) {
    throw new Error('Invalid workflow field: steps must be a non-empty array of step objects');
  }
  const blocks = parseWorkflowBlocks(obj.blocks);

  return {
    id: obj.id as string,
    name: obj.name as string,
    description: typeof description === 'string' ? description : '',
    version: typeof obj.version === 'string' ? obj.version : undefined,
    author: typeof obj.author === 'string' ? obj.author : undefined,
    variables: (variables as import('../types').WorkflowVariable[] | undefined) ?? [],
    connection: typeof obj.connection === 'string' ? obj.connection : undefined,
    database: typeof obj.database === 'string' ? obj.database : undefined,
    steps: (steps ?? []) as import('../types').WorkflowStep[],
    blocks,
    output: isRecord(obj.output)
      ? (obj.output as unknown as import('../types').WorkflowOutput)
      : undefined,
    timeoutSecs: typeof obj.timeoutSecs === 'number' ? obj.timeoutSecs : undefined,
    errorHandling: isRecord(obj.errorHandling)
      ? (obj.errorHandling as unknown as import('../types').ErrorHandlingConfig)
      : undefined,
    schedule: isRecord(obj.schedule)
      ? (obj.schedule as unknown as import('../types').WorkflowSchedule)
      : undefined,
    visibility:
      obj.visibility === 'user' || obj.visibility === 'dashboardHidden'
        ? obj.visibility
        : undefined,
  };
}
