import { describe, it, expect } from 'vitest';
import { extractQuestions, parseToolCallQuestions } from '../extractQuestions';

describe('extractQuestions', () => {
  it('returns original content and empty questions when no tag present', () => {
    const content = 'Hello, how can I help?';
    const result = extractQuestions(content);
    expect(result.cleanContent).toBe(content);
    expect(result.questions).toEqual([]);
  });

  it('extracts a single question with options', () => {
    const content = `Let me ask you something.

<ask_questions>
[{"id":"q1","prompt":"Which database?","options":[{"id":"pg","label":"PostgreSQL"},{"id":"mysql","label":"MySQL"}],"allowMultiple":false}]
</ask_questions>`;

    const result = extractQuestions(content);
    expect(result.cleanContent).toBe('Let me ask you something.');
    expect(result.questions).toHaveLength(1);
    expect(result.questions[0].id).toBe('q1');
    expect(result.questions[0].prompt).toBe('Which database?');
    expect(result.questions[0].options).toHaveLength(2);
    expect(result.questions[0].options[0].label).toBe('PostgreSQL');
    expect(result.questions[0].allowMultiple).toBe(false);
  });

  it('extracts multiple questions', () => {
    const content = `I need some information.

<ask_questions>
[
  {"id":"q1","prompt":"Question 1","options":[{"id":"a","label":"A"},{"id":"b","label":"B"}],"allowMultiple":false},
  {"id":"q2","prompt":"Question 2","options":[],"allowMultiple":true}
]
</ask_questions>`;

    const result = extractQuestions(content);
    expect(result.questions).toHaveLength(2);
    expect(result.questions[0].id).toBe('q1');
    expect(result.questions[1].id).toBe('q2');
    expect(result.questions[1].options).toEqual([]);
    expect(result.questions[1].allowMultiple).toBe(true);
  });

  it('removes the tag from content cleanly', () => {
    const content = `Before text.

<ask_questions>
[{"id":"q1","prompt":"Pick one","options":[{"id":"a","label":"A"}]}]
</ask_questions>

After text.`;

    const result = extractQuestions(content);
    expect(result.cleanContent).toContain('Before text.');
    expect(result.cleanContent).toContain('After text.');
    expect(result.cleanContent).not.toContain('ask_questions');
  });

  it('handles invalid JSON gracefully', () => {
    const content = `Some text.

<ask_questions>
not valid json
</ask_questions>`;

    const result = extractQuestions(content);
    expect(result.cleanContent).toBe(content);
    expect(result.questions).toEqual([]);
  });

  it('handles non-array JSON', () => {
    const content = `Text.

<ask_questions>
{"id":"q1","prompt":"test"}
</ask_questions>`;

    const result = extractQuestions(content);
    expect(result.cleanContent).toBe(content);
    expect(result.questions).toEqual([]);
  });

  it('filters out malformed question objects', () => {
    const content = `Text.

<ask_questions>
[
  {"id":"q1","prompt":"Valid","options":[]},
  {"noId":true,"prompt":"Missing id"},
  {"id":"q3","prompt":"Also valid","options":[{"id":"a","label":"A"}]}
]
</ask_questions>`;

    const result = extractQuestions(content);
    expect(result.questions).toHaveLength(2);
    expect(result.questions[0].id).toBe('q1');
    expect(result.questions[1].id).toBe('q3');
  });

  it('handles empty options array', () => {
    const content = `Text.

<ask_questions>
[{"id":"q1","prompt":"Free text question","options":[]}]
</ask_questions>`;

    const result = extractQuestions(content);
    expect(result.questions).toHaveLength(1);
    expect(result.questions[0].options).toEqual([]);
  });

  it('handles content with only the tag (no surrounding text)', () => {
    const content = `<ask_questions>
[{"id":"q1","prompt":"Just a question","options":[{"id":"a","label":"Answer A"}]}]
</ask_questions>`;

    const result = extractQuestions(content);
    expect(result.cleanContent).toBe('');
    expect(result.questions).toHaveLength(1);
  });

  it('defaults allowMultiple to false when not specified', () => {
    const content = `Text.

<ask_questions>
[{"id":"q1","prompt":"Test","options":[]}]
</ask_questions>`;

    const result = extractQuestions(content);
    expect(result.questions[0].allowMultiple).toBe(false);
  });

  it('filters out options with missing id or label', () => {
    const content = `Text.

<ask_questions>
[{"id":"q1","prompt":"Test","options":[{"id":"a","label":"Good"},{"id":"b"},{"label":"No id"},{"id":"c","label":"Also good"}]}]
</ask_questions>`;

    const result = extractQuestions(content);
    expect(result.questions[0].options).toHaveLength(2);
    expect(result.questions[0].options[0].id).toBe('a');
    expect(result.questions[0].options[1].id).toBe('c');
  });
});

describe('parseToolCallQuestions', () => {
  it('returns empty array when toolCalls is undefined', () => {
    expect(parseToolCallQuestions(undefined)).toEqual([]);
  });

  it('returns empty array when toolCalls is empty', () => {
    expect(parseToolCallQuestions([])).toEqual([]);
  });

  it('returns empty array when no ask_questions tool call', () => {
    expect(parseToolCallQuestions([
      { id: 'tc1', name: 'other_tool', arguments: '{}' },
    ])).toEqual([]);
  });

  it('parses valid ask_questions tool call', () => {
    const toolCalls = [{
      id: 'tc1',
      name: 'ask_questions',
      arguments: JSON.stringify({
        questions: [
          {
            id: 'q1',
            prompt: 'Which DB?',
            options: [
              { id: 'pg', label: 'PostgreSQL' },
              { id: 'mysql', label: 'MySQL' },
            ],
            allowMultiple: false,
          },
        ],
      }),
    }];
    const result = parseToolCallQuestions(toolCalls);
    expect(result).toHaveLength(1);
    expect(result[0].id).toBe('q1');
    expect(result[0].prompt).toBe('Which DB?');
    expect(result[0].options).toHaveLength(2);
    expect(result[0].options[0].label).toBe('PostgreSQL');
    expect(result[0].allowMultiple).toBe(false);
  });

  it('parses multiple questions from tool call', () => {
    const toolCalls = [{
      id: 'tc1',
      name: 'ask_questions',
      arguments: JSON.stringify({
        questions: [
          { id: 'q1', prompt: 'Q1', options: [] },
          { id: 'q2', prompt: 'Q2', options: [{ id: 'a', label: 'A' }], allowMultiple: true },
        ],
      }),
    }];
    const result = parseToolCallQuestions(toolCalls);
    expect(result).toHaveLength(2);
    expect(result[1].allowMultiple).toBe(true);
  });

  it('returns empty array for invalid JSON arguments', () => {
    const toolCalls = [{
      id: 'tc1',
      name: 'ask_questions',
      arguments: 'not json',
    }];
    expect(parseToolCallQuestions(toolCalls)).toEqual([]);
  });

  it('returns empty array when questions field is not an array', () => {
    const toolCalls = [{
      id: 'tc1',
      name: 'ask_questions',
      arguments: JSON.stringify({ questions: 'not array' }),
    }];
    expect(parseToolCallQuestions(toolCalls)).toEqual([]);
  });

  it('filters out malformed question objects', () => {
    const toolCalls = [{
      id: 'tc1',
      name: 'ask_questions',
      arguments: JSON.stringify({
        questions: [
          { id: 'q1', prompt: 'Valid', options: [] },
          { noId: true, prompt: 'Missing id' },
          null,
          42,
          { id: 'q3', prompt: 'Also valid', options: [] },
        ],
      }),
    }];
    const result = parseToolCallQuestions(toolCalls);
    expect(result).toHaveLength(2);
    expect(result[0].id).toBe('q1');
    expect(result[1].id).toBe('q3');
  });

  it('filters out invalid options', () => {
    const toolCalls = [{
      id: 'tc1',
      name: 'ask_questions',
      arguments: JSON.stringify({
        questions: [{
          id: 'q1',
          prompt: 'Test',
          options: [
            { id: 'a', label: 'Good' },
            { id: 'b' },
            { label: 'No id' },
            { id: 'c', label: 'Also good' },
          ],
        }],
      }),
    }];
    const result = parseToolCallQuestions(toolCalls);
    expect(result[0].options).toHaveLength(2);
    expect(result[0].options[0].id).toBe('a');
    expect(result[0].options[1].id).toBe('c');
  });

  it('picks ask_questions from multiple tool calls', () => {
    const toolCalls = [
      { id: 'tc1', name: 'other_tool', arguments: '{}' },
      { id: 'tc2', name: 'ask_questions', arguments: JSON.stringify({
        questions: [{ id: 'q1', prompt: 'Found', options: [] }],
      }) },
    ];
    const result = parseToolCallQuestions(toolCalls);
    expect(result).toHaveLength(1);
    expect(result[0].prompt).toBe('Found');
  });

  it('defaults allowMultiple to false when not specified', () => {
    const toolCalls = [{
      id: 'tc1',
      name: 'ask_questions',
      arguments: JSON.stringify({
        questions: [{ id: 'q1', prompt: 'Test', options: [] }],
      }),
    }];
    const result = parseToolCallQuestions(toolCalls);
    expect(result[0].allowMultiple).toBe(false);
  });
});
