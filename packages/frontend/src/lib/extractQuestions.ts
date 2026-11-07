import type { AiQuestion, AiToolCall } from '../types';

const ASK_QUESTIONS_RE = /<ask_questions>\s*([\s\S]*?)\s*<\/ask_questions>/;

/**
 * Extracts structured questions from an AI response.
 * Questions are expected in `<ask_questions>[...]</ask_questions>` blocks.
 * Returns the cleaned content (tag removed) and parsed questions.
 */
export function extractQuestions(content: string): {
  cleanContent: string;
  questions: AiQuestion[];
} {
  const match = ASK_QUESTIONS_RE.exec(content);
  if (!match) {
    return { cleanContent: content, questions: [] };
  }

  const jsonStr = match[1].trim();
  const cleanContent = content.replace(ASK_QUESTIONS_RE, '').trim();

  try {
    const raw = JSON.parse(jsonStr) as unknown;
    if (!Array.isArray(raw)) {
      return { cleanContent: content, questions: [] };
    }

    const questions: AiQuestion[] = raw
      .filter(
        (q): q is Record<string, unknown> =>
          typeof q === 'object' && q !== null && typeof (q as Record<string, unknown>).id === 'string',
      )
      .map((q) => ({
        id: String(q.id),
        prompt: String(q.prompt ?? ''),
        options: Array.isArray(q.options)
          ? (q.options as Record<string, unknown>[])
              .filter((o) => typeof o.id === 'string' && typeof o.label === 'string')
              .map((o) => ({ id: String(o.id), label: String(o.label) }))
          : [],
        allowMultiple: q.allowMultiple === true,
      }));

    return { cleanContent, questions };
  } catch {
    return { cleanContent: content, questions: [] };
  }
}

/**
 * Parse questions from an `ask_questions` tool call's arguments JSON.
 * Returns parsed questions or empty array if parsing fails.
 */
export function parseToolCallQuestions(toolCalls: AiToolCall[] | undefined): AiQuestion[] {
  if (!toolCalls || toolCalls.length === 0) return [];

  const askCall = toolCalls.find((tc) => tc.name === 'ask_questions');
  if (!askCall) return [];

  try {
    const parsed = JSON.parse(askCall.arguments) as { questions?: unknown[] };
    if (!Array.isArray(parsed.questions)) return [];

    return parsed.questions
      .filter(
        (q): q is Record<string, unknown> =>
          typeof q === 'object' && q !== null && typeof (q as Record<string, unknown>).id === 'string',
      )
      .map((q) => ({
        id: String(q.id),
        prompt: String(q.prompt ?? ''),
        options: Array.isArray(q.options)
          ? (q.options as Record<string, unknown>[])
              .filter((o) => typeof o.id === 'string' && typeof o.label === 'string')
              .map((o) => ({ id: String(o.id), label: String(o.label) }))
          : [],
        allowMultiple: q.allowMultiple === true,
      }));
  } catch {
    return [];
  }
}
