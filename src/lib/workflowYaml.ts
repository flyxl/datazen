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
