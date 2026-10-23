import { beforeEach, describe, expect, it } from 'vitest';
import { assertResultWritable, resultEditKey, usePanelResultEdit } from '../panelResultEdit';
import type { ServerEditability } from '../../lib/resultEditability';

const refused: ServerEditability = {
  writable: false,
  identityColumns: [],
  editableColumns: [],
  table: null,
  reason: 'multiTable',
};

const granted: ServerEditability = {
  writable: true,
  identityColumns: [{ column: 'id', sourceColumn: 'id' }],
  editableColumns: [{ column: 'id', sourceColumn: 'id' }],
  table: { catalog: null, schema: null, name: 'orders', aliased: false },
  reason: null,
};

beforeEach(() => {
  usePanelResultEdit.setState({ verdicts: {}, resolving: {}, errors: {} });
});

describe('assertResultWritable', () => {
  it('refuses a write to a result the driver refused', () => {
    // The origin channel's guard: an origin refusal closes the origin write
    // path (and nothing else — the narrow channel has its own verdict and
    // its own re-verification against the live catalog).
    usePanelResultEdit.setState({
      verdicts: { [resultEditKey('sess-1', 0)]: refused },
      resolving: {},
      errors: {},
    });

    expect(() => assertResultWritable('sess-1', 0)).toThrow();
  });

  it('refuses the write even when the refusal is for another reason', () => {
    usePanelResultEdit.setState({
      verdicts: { [resultEditKey('sess-1', 0)]: { ...refused, reason: 'view' } },
      resolving: {},
      errors: {},
    });

    expect(() => assertResultWritable('sess-1', 0)).toThrow();
  });

  it('lets a write through when the driver granted it', () => {
    usePanelResultEdit.setState({
      verdicts: { [resultEditKey('sess-1', 0)]: granted },
      resolving: {},
      errors: {},
    });

    expect(() => assertResultWritable('sess-1', 0)).not.toThrow();
  });

  it('lets a write through when nobody judged the result yet', () => {
    // "Not judged yet" is not "judged read-only": the judgement arrives on
    // its own, and the interface stays read-only until it does, so the store
    // does not refuse here.
    expect(() => assertResultWritable('sess-1', 0)).not.toThrow();
  });

  it('judges the result that is being written, not another one', () => {
    usePanelResultEdit.setState({
      verdicts: { [resultEditKey('sess-1', 0)]: refused },
      resolving: {},
      errors: {},
    });

    expect(() => assertResultWritable('sess-1', 1)).not.toThrow();
    expect(() => assertResultWritable('sess-2', 0)).not.toThrow();
  });
});
