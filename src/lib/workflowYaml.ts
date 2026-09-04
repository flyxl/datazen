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
  return /\bsteps\s*:/.test(yaml) && (/\bid\s*:/.test(yaml) || /\bname\s*:/.test(yaml));
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
 */
export function validateWorkflowFields(obj: Record<string, unknown>): string | null {
  if (!obj.id || typeof obj.id !== 'string') return 'id';
  if (!obj.name || typeof obj.name !== 'string') return 'name';
  if (!Array.isArray(obj.steps) || obj.steps.length === 0) return 'steps';
  return null;
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
 * Runtime guard: narrow a validated YAML object to WorkflowDefinition.
 * Throws when shape does not match the IPC contract.
 */
export function parseValidatedWorkflowDefinition(obj: Record<string, unknown>): import('../types').WorkflowDefinition {
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

  const steps = obj.steps;
  if (!Array.isArray(steps) || !steps.every(isWorkflowStep)) {
    throw new Error('Invalid workflow field: steps must be a non-empty array of step objects');
  }

  return {
    id: obj.id as string,
    name: obj.name as string,
    description: typeof description === 'string' ? description : '',
    version: typeof obj.version === 'string' ? obj.version : undefined,
    author: typeof obj.author === 'string' ? obj.author : undefined,
    variables: (variables as import('../types').WorkflowVariable[] | undefined) ?? [],
    connection: typeof obj.connection === 'string' ? obj.connection : undefined,
    steps: steps as import('../types').WorkflowStep[],
    output: isRecord(obj.output) ? (obj.output as unknown as import('../types').WorkflowOutput) : undefined,
    timeoutSecs: typeof obj.timeoutSecs === 'number' ? obj.timeoutSecs : undefined,
    errorHandling: isRecord(obj.errorHandling)
      ? (obj.errorHandling as unknown as import('../types').ErrorHandlingConfig)
      : undefined,
    schedule: isRecord(obj.schedule)
      ? (obj.schedule as unknown as import('../types').WorkflowSchedule)
      : undefined,
    visibility:
      obj.visibility === 'user' || obj.visibility === 'dashboardHidden' ? obj.visibility : undefined,
  };
}
