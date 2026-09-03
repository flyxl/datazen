import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { useState } from 'react';
import { render, cleanup, fireEvent, screen } from '@testing-library/react';
import { WorkflowForm, emptyDraft } from '../WorkflowForm';
import type { WorkflowDraft } from '../WorkflowForm';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../components/SqlEditor', async () => {
  const { forwardRef } = await import('react');
  return {
    SqlEditor: forwardRef(
      (
        {
          value,
          onChange,
          placeholder,
        }: { value: string; onChange: (v: string) => void; placeholder?: string },
        _ref: unknown,
      ) => (
        <textarea
          data-testid="sql-editor"
          placeholder={placeholder}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      ),
    ),
  };
});

vi.mock('../../../components/ui/Select', () => ({
  Select: ({
    value,
    options,
    onChange,
    className,
  }: {
    value: string;
    options: { value: string; label: string }[];
    onChange: (v: string) => void;
    className?: string;
  }) => (
    <select
      data-testid="mock-select"
      className={className}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  ),
}));

const connections = [{ id: 'c1', name: 'PG', databaseType: 'postgresql' }];

function StatefulForm({
  initialDraft,
  editingId = null,
  conns = connections,
  onSave = vi.fn(),
  onCancel = vi.fn(),
}: {
  initialDraft: WorkflowDraft;
  editingId?: string | null;
  conns?: typeof connections;
  onSave?: ReturnType<typeof vi.fn>;
  onCancel?: ReturnType<typeof vi.fn>;
}) {
  const [draft, setDraft] = useState(initialDraft);
  return (
    <WorkflowForm
      draft={draft}
      editingId={editingId}
      connections={conns}
      onDraftChange={setDraft}
      onSave={onSave}
      onCancel={onCancel}
    />
  );
}

function renderForm(
  draft: WorkflowDraft,
  opts: {
    editingId?: string | null;
    onDraftChange?: ReturnType<typeof vi.fn>;
    onSave?: ReturnType<typeof vi.fn>;
    onCancel?: ReturnType<typeof vi.fn>;
    conns?: typeof connections;
    stateful?: boolean;
  } = {},
) {
  const onDraftChange = opts.onDraftChange ?? vi.fn();
  const onSave = opts.onSave ?? vi.fn();
  const onCancel = opts.onCancel ?? vi.fn();
  if (opts.stateful) {
    render(
      <StatefulForm
        initialDraft={draft}
        editingId={opts.editingId ?? null}
        conns={opts.conns ?? connections}
        onSave={onSave}
        onCancel={onCancel}
      />,
    );
  } else {
    render(
      <WorkflowForm
        draft={draft}
        editingId={opts.editingId ?? null}
        connections={opts.conns ?? connections}
        onDraftChange={onDraftChange}
        onSave={onSave}
        onCancel={onCancel}
      />,
    );
  }
  return { onDraftChange, onSave, onCancel };
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(cleanup);

describe('WorkflowForm', () => {
  it('emptyDraft seeds one query step', () => {
    const d = emptyDraft();
    expect(d.steps).toHaveLength(1);
    expect(d.steps[0].type).toBe('query');
    expect(d.id).toBe('');
    expect(d.variables).toHaveLength(0);
  });

  it('edits id, name and description via onDraftChange', () => {
    const { onDraftChange } = renderForm(emptyDraft());
    const inputs = document.querySelectorAll('input');

    fireEvent.change(inputs[0], { target: { value: 'my-wf' } });
    expect(onDraftChange.mock.calls.at(-1)?.[0].id).toBe('my-wf');

    fireEvent.change(inputs[1], { target: { value: 'My WF' } });
    expect(onDraftChange.mock.calls.at(-1)?.[0].name).toBe('My WF');

    fireEvent.change(inputs[2], { target: { value: 'desc' } });
    expect(onDraftChange.mock.calls.at(-1)?.[0].description).toBe('desc');
  });

  it('disables id field while editing', () => {
    renderForm({ ...emptyDraft(), id: 'wf-1' }, { editingId: 'wf-1' });
    expect(screen.getByDisplayValue('wf-1')).toBeDisabled();
  });

  it('adds, edits, and removes a variable row', () => {
    renderForm(emptyDraft(), { stateful: true });

    fireEvent.click(screen.getByText('+ workflows.form.addVariable'));
    expect(screen.getByPlaceholderText('workflows.form.varName')).toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText('workflows.form.varName'), {
      target: { value: 'connVar' },
    });
    expect(screen.getByDisplayValue('connVar')).toBeInTheDocument();

    // The workflow-level connection select is now rendered before variable selects.
    const typeSelect = screen.getAllByRole('combobox')[1];
    fireEvent.change(typeSelect, { target: { value: 'connection' } });
    expect(typeSelect).toHaveValue('connection');

    fireEvent.change(screen.getByPlaceholderText('workflows.form.varDesc'), {
      target: { value: 'pick conn' },
    });
    expect(screen.getByDisplayValue('pick conn')).toBeInTheDocument();

    const required = screen.getByLabelText('workflows.form.varRequired');
    fireEvent.click(required);
    expect(required).toBeChecked();

    const trashBtn = Array.from(document.querySelectorAll('button')).find((b) =>
      b.querySelector('.lucide-trash2'),
    );
    expect(trashBtn).toBeTruthy();
    fireEvent.click(trashBtn!);
    expect(screen.queryByPlaceholderText('workflows.form.varName')).not.toBeInTheDocument();
  });

  it('adds a step and edits step fields', () => {
    renderForm(emptyDraft(), { stateful: true });

    fireEvent.click(screen.getByText('+ workflows.addStep'));
    const stepIdInputs = screen.getAllByPlaceholderText('step_id');
    expect(stepIdInputs).toHaveLength(2);

    fireEvent.change(stepIdInputs[1], { target: { value: 'step2' } });
    expect(screen.getByDisplayValue('step2')).toBeInTheDocument();

    // Select order is: workflow connection, step 1 type, step 1 connection,
    // step 2 type, step 2 connection. Changing step 1 to AI removes its
    // connection select, so locate the second step's connection again after
    // the rerender rather than relying on the old index.
    const typeSelects = screen.getAllByTestId('mock-select');
    const firstStepType = typeSelects[1];
    fireEvent.change(firstStepType, { target: { value: 'ai' } });
    expect(firstStepType).toHaveValue('ai');
    expect(screen.getByPlaceholderText('AI prompt...')).toBeInTheDocument();

    const selectsAfterTypeChange = screen.getAllByTestId('mock-select');
    const secondStepConnection = selectsAfterTypeChange[3];
    fireEvent.change(secondStepConnection, { target: { value: 'c1' } });
    expect(secondStepConnection).toHaveValue('c1');
  });

  it('edits query SQL and ai prompt textareas', () => {
    const draft: WorkflowDraft = {
      ...emptyDraft(),
      steps: [
        { type: 'query', id: 'q1', sql: '' },
        { type: 'ai', id: 'a1', prompt: '' },
      ],
    };
    const { onDraftChange } = renderForm(draft);

    fireEvent.change(screen.getByTestId('sql-editor'), { target: { value: 'SELECT 1' } });
    expect(onDraftChange.mock.calls.at(-1)?.[0].steps[0].sql).toBe('SELECT 1');

    fireEvent.change(screen.getByPlaceholderText('AI prompt...'), {
      target: { value: 'Summarize data' },
    });
    expect(onDraftChange.mock.calls.at(-1)?.[0].steps[1].prompt).toBe('Summarize data');
  });

  it('removes a step when more than one exists', () => {
    const draft: WorkflowDraft = {
      ...emptyDraft(),
      steps: [
        { type: 'query', id: 'step1', sql: '' },
        { type: 'query', id: 'step2', sql: '' },
      ],
    };
    renderForm(draft, { stateful: true });

    const stepDeleteButtons = Array.from(document.querySelectorAll('button')).filter(
      (b) => b.className.includes('text-red-400') && b.querySelector('svg'),
    );
    expect(stepDeleteButtons.length).toBeGreaterThan(0);
    fireEvent.click(stepDeleteButtons[0]);
    expect(screen.getAllByPlaceholderText('step_id')).toHaveLength(1);
  });

  it('hides step connection select when no connections are provided', () => {
    renderForm(emptyDraft(), { conns: [] });
    // Workflow-level connection remains available even without connections;
    // the per-step connection selector is the one that should be hidden.
    expect(screen.queryAllByTestId('mock-select')).toHaveLength(2);
  });

  it('save and cancel buttons fire handlers', () => {
    const onSave = vi.fn();
    const onCancel = vi.fn();
    renderForm(emptyDraft(), { onSave, onCancel });
    fireEvent.click(screen.getByText('common.save'));
    fireEvent.click(screen.getByText('common.cancel'));
    expect(onSave).toHaveBeenCalled();
    expect(onCancel).toHaveBeenCalled();
  });

  it('toggles schedule and edits interval', () => {
    const { onDraftChange } = renderForm(emptyDraft());
    fireEvent.click(screen.getByText('workflows.schedule.enabled'));
    expect(onDraftChange.mock.calls.at(-1)?.[0].scheduleEnabled).toBe(true);

    const { onDraftChange: onChange2 } = renderForm({
      ...emptyDraft(),
      scheduleEnabled: true,
      scheduleIntervalSecs: 3600,
    });
    fireEvent.change(screen.getByDisplayValue('3600'), { target: { value: '90' } });
    expect(onChange2.mock.calls.at(-1)?.[0].scheduleIntervalSecs).toBe(90);
  });
});
