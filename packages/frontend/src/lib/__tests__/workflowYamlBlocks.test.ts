import { describe, expect, it } from 'vitest';
import {
  extractWorkflowYaml,
  parseValidatedWorkflowDefinition,
  validateWorkflowFields,
} from '../workflowYaml';

// A v2 definition keeps its steps inside `blocks`; a block is a resource
// scope, not step sugar. These tests pin what the form must preserve and what
// it must refuse before the backend ever sees the definition.
const FENCE = String.fromCharCode(96).repeat(3);

describe('validateWorkflowFields — v2 blocks', () => {
  const PG_UNPINNED = [{ id: 'pg', requiresExplicitDatabase: true }];

  const blockOnly = (block: Record<string, unknown>) => ({
    id: 'wf-1',
    name: 'Demo',
    version: '2',
    blocks: [block],
  });

  const sessionBlock = (extra: Record<string, unknown> = {}) => ({
    id: 'blk',
    type: 'sessionBlock',
    steps: [{ type: 'query', id: 's1', sql: 'SELECT 1' }],
    ...extra,
  });

  it('accepts a definition whose steps live only inside blocks', () => {
    expect(validateWorkflowFields(blockOnly(sessionBlock()))).toBeNull();
    expect(validateWorkflowFields(blockOnly(sessionBlock()), PG_UNPINNED)).toBeNull();
  });

  it('still rejects a definition carrying neither steps nor blocks', () => {
    expect(validateWorkflowFields({ id: 'wf-1', name: 'Demo' })).toBe('steps');
    expect(validateWorkflowFields({ id: 'wf-1', name: 'Demo', steps: [], blocks: [] })).toBe(
      'steps',
    );
  });

  it('inherits the block-level database before the workflow-level one', () => {
    expect(
      validateWorkflowFields(
        blockOnly(sessionBlock({ connection: 'pg', database: 'block_db' })),
        PG_UNPINNED,
      ),
    ).toBeNull();
    // No block database: the workflow default sits one level below it and does
    // not cover the block, so the block step is still unresolved.
    expect(validateWorkflowFields(blockOnly(sessionBlock({ connection: 'pg' })), PG_UNPINNED)).toBe(
      'blocks[0].steps[0].database',
    );
    expect(
      validateWorkflowFields(
        { ...blockOnly(sessionBlock({ connection: 'pg' })), database: 'wf_db' },
        PG_UNPINNED,
      ),
    ).toBeNull();
  });

  it('treats a blank block database as undeclared', () => {
    expect(
      validateWorkflowFields(
        { ...blockOnly(sessionBlock({ connection: 'pg', database: '   ' })), database: 'wf_db' },
        PG_UNPINNED,
      ),
    ).toBeNull();
  });

  it('reports the offending block step by path', () => {
    const block = {
      id: 'blk',
      type: 'sessionBlock',
      connection: 'pg',
      steps: [
        { type: 'query', id: 'ok', database: 'd' },
        { type: 'query', id: 'bad' },
      ],
    };
    expect(validateWorkflowFields(blockOnly(block), PG_UNPINNED)).toBe(
      'blocks[0].steps[1].database',
    );
  });

  it('lets a block step override the block target with its own connection', () => {
    const block = sessionBlock({
      connection: 'pg',
      steps: [{ type: 'query', id: 's1', connection: 'pinned' }],
    });
    expect(
      validateWorkflowFields(blockOnly(block), [
        ...PG_UNPINNED,
        { id: 'pinned', requiresExplicitDatabase: false },
      ]),
    ).toBeNull();
  });

  it('keeps checking top-level steps when blocks are also declared', () => {
    const obj = {
      id: 'wf-1',
      name: 'Demo',
      connection: 'pg',
      steps: [{ type: 'query', id: 's1', sql: 'SELECT 1' }],
      blocks: [sessionBlock({ connection: 'pg', database: 'block_db' })],
    };
    expect(validateWorkflowFields(obj, PG_UNPINNED)).toBe('steps[0].database');
    expect(validateWorkflowFields({ ...obj, database: 'wf_db' }, PG_UNPINNED)).toBeNull();
  });
});

describe('parseValidatedWorkflowDefinition — v2 blocks', () => {
  const blockDefinition = (block: Record<string, unknown>) => ({
    id: 'wf-1',
    name: 'Demo',
    version: '2',
    blocks: [block],
  });

  it('keeps the workflow-level database that the backend defaults inherit from', () => {
    const workflow = parseValidatedWorkflowDefinition({
      id: 'wf-1',
      name: 'Demo',
      database: 'datazen_demo',
      steps: [{ id: 's1', type: 'query' }],
    });
    expect(workflow.database).toBe('datazen_demo');
  });

  it('keeps a block-only definition with its block target and steps', () => {
    const workflow = parseValidatedWorkflowDefinition(
      blockDefinition({
        id: 'blk',
        type: 'transactionBlock',
        connection: 'pg',
        database: 'datazen_demo',
        parallel: true,
        retry: { block: 2 },
        steps: [{ id: 's1', type: 'command', command: 'c1' }],
      }),
    );
    expect(workflow.steps).toEqual([]);
    expect(workflow.blocks).toHaveLength(1);
    expect(workflow.blocks?.[0]).toEqual({
      id: 'blk',
      type: 'transactionBlock',
      connection: 'pg',
      database: 'datazen_demo',
      parallel: true,
      retry: { block: 2 },
      steps: [{ id: 's1', type: 'command', command: 'c1' }],
    });
  });

  it('defaults parallel to false and retry to absent', () => {
    const workflow = parseValidatedWorkflowDefinition(
      blockDefinition({
        id: 'blk',
        type: 'sessionBlock',
        steps: [{ id: 's1', type: 'query', sql: 'SELECT 1' }],
      }),
    );
    expect(workflow.blocks?.[0]?.parallel).toBe(false);
    expect(workflow.blocks?.[0]?.retry).toBeUndefined();
  });

  it('rejects a block type the backend would not execute', () => {
    expect(() =>
      parseValidatedWorkflowDefinition(
        blockDefinition({
          id: 'blk',
          type: 'parallelBlock',
          steps: [{ id: 's1', type: 'query' }],
        }),
      ),
    ).toThrow(/type must be sessionBlock or transactionBlock/);
  });

  it('rejects duplicate block ids that would make result paths ambiguous', () => {
    expect(() =>
      parseValidatedWorkflowDefinition({
        id: 'wf-1',
        name: 'Demo',
        blocks: [
          { id: 'blk', type: 'sessionBlock', steps: [{ id: 's1', type: 'query' }] },
          { id: 'blk', type: 'sessionBlock', steps: [{ id: 's2', type: 'query' }] },
        ],
      }),
    ).toThrow(/duplicates block 'blk'/);
  });

  it('rejects a block with no steps and a step-shaped hole', () => {
    expect(() =>
      parseValidatedWorkflowDefinition(
        blockDefinition({ id: 'blk', type: 'sessionBlock', steps: [] }),
      ),
    ).toThrow(/blocks\[0\]\.steps must be a non-empty array/);
    expect(() =>
      parseValidatedWorkflowDefinition(
        blockDefinition({ id: 'blk', type: 'sessionBlock', steps: [{ id: 's1' }] }),
      ),
    ).toThrow(/blocks\[0\]\.steps must be a non-empty array/);
  });

  it('rejects a non-boolean parallel flag', () => {
    expect(() =>
      parseValidatedWorkflowDefinition(
        blockDefinition({
          id: 'blk',
          type: 'sessionBlock',
          parallel: 'yes',
          steps: [{ id: 's1', type: 'query' }],
        }),
      ),
    ).toThrow(/parallel must be a boolean/);
  });

  it('rejects a retry that is not a whole-block count', () => {
    // A per-step retry would silently mean nothing here; the block retry unit
    // is always the whole block, so the form refuses anything else.
    for (const retry of [{ step: 1 }, { block: -1 }, { block: 1.5 }, 'block']) {
      expect(() =>
        parseValidatedWorkflowDefinition(
          blockDefinition({
            id: 'blk',
            type: 'transactionBlock',
            retry,
            steps: [{ id: 's1', type: 'query' }],
          }),
        ),
      ).toThrow(/retry must be \{ block: <non-negative integer> \}/);
    }
  });
});

describe('extractWorkflowYaml — v2 blocks', () => {
  it('recognizes a block-only workflow in an AI answer', () => {
    const text = [
      FENCE + 'yaml',
      'id: wf-2',
      'name: Blocked',
      'version: "2"',
      'blocks:',
      '  - id: blk',
      '    type: transactionBlock',
      FENCE,
    ].join('\n');
    const blocks = extractWorkflowYaml(text);
    expect(blocks).toHaveLength(1);
    expect(blocks[0]).toContain('type: transactionBlock');
  });
});
