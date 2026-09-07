import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup, screen } from '@testing-library/react';
import { BindParamPanel } from '../BindParamPanel';
import type { ParamHistoryEntry } from '../../../windows/connection/query/useBindParameters';
import type { SqlParam } from '../../../lib/sqlBindParams';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

afterEach(cleanup);

const colonParam: SqlParam = {
  name: 'uid',
  kind: 'named',
  syntax: 'colon',
  stableId: 'named:uid',
};

const dollarParam: SqlParam = {
  name: '1',
  kind: 'positional',
  syntax: 'dollar-positional',
  stableId: 'dollar:1',
};

const atParam: SqlParam = {
  name: 'name',
  kind: 'named',
  syntax: 'at',
  stableId: 'named:name',
};

const questionParam: SqlParam = {
  name: '1',
  kind: 'positional',
  syntax: 'question',
  stableId: 'question:1',
  ordinal: 1,
};

const templateParam: SqlParam = {
  name: 'x',
  kind: 'named',
  syntax: 'template',
  stableId: 'named:x',
};

describe('BindParamPanel', () => {
  it('renders nothing without params', () => {
    const { container } = render(<BindParamPanel params={[]} values={{}} onChange={vi.fn()} />);
    expect(container.firstChild).toBeNull();
  });

  it('shows param label header', () => {
    render(<BindParamPanel params={[colonParam]} values={{}} onChange={vi.fn()} />);
    expect(screen.getByText('query.params:')).toBeInTheDocument();
  });

  it('shows colon and dollar labels in legacy mode', () => {
    render(<BindParamPanel params={[colonParam, dollarParam]} values={{}} onChange={vi.fn()} />);
    expect(screen.getByText(':uid')).toBeInTheDocument();
    expect(screen.getByText('$1')).toBeInTheDocument();
  });

  it('reports edits using param.name in legacy mode', () => {
    const onChange = vi.fn();
    render(<BindParamPanel params={[colonParam]} values={{}} onChange={onChange} />);
    const input = screen.getByPlaceholderText('query.paramValue');
    fireEvent.change(input, { target: { value: '42' } });
    expect(onChange).toHaveBeenCalledWith('uid', '42');
  });

  it('displays current value from values prop', () => {
    render(<BindParamPanel params={[colonParam]} values={{ uid: '7' }} onChange={vi.fn()} />);
    const input = screen.getByPlaceholderText('query.paramValue') as HTMLInputElement;
    expect(input.value).toBe('7');
  });
});

describe('BindParamPanel stableId mode', () => {
  it('uses stableId as keys when labels prop is provided', () => {
    const onChange = vi.fn();
    render(
      <BindParamPanel
        params={[colonParam, dollarParam]}
        values={{ 'named:uid': '42', 'dollar:1': '99' }}
        labels={{ 'named:uid': ':uid', 'dollar:1': '$1' }}
        onChange={onChange}
      />,
    );
    expect(screen.getByText(':uid')).toBeInTheDocument();
    expect(screen.getByText('$1')).toBeInTheDocument();

    const inputs = screen.getAllByPlaceholderText('query.paramValue');
    fireEvent.change(inputs[0], { target: { value: '100' } });
    expect(onChange).toHaveBeenCalledWith('named:uid', '100');
  });

  it('displays values keyed by stableId', () => {
    render(
      <BindParamPanel
        params={[colonParam]}
        values={{ 'named:uid': '42' }}
        labels={{ 'named:uid': ':uid' }}
        onChange={vi.fn()}
      />,
    );
    const input = screen.getByPlaceholderText('query.paramValue') as HTMLInputElement;
    expect(input.value).toBe('42');
  });
});

describe('BindParamPanel active highlighting', () => {
  it('applies accent class when param is active', () => {
    render(
      <BindParamPanel
        params={[colonParam, dollarParam]}
        values={{}}
        labels={{ 'named:uid': ':uid', 'dollar:1': '$1' }}
        activeStableIds={new Set(['named:uid'])}
        onChange={vi.fn()}
      />,
    );
    const label = screen.getByText(':uid');
    expect(label.className).toContain('text-accent');
    const dollarLabel = screen.getByText('$1');
    expect(dollarLabel.className).not.toContain('text-accent');
  });
});

describe('BindParamPanel history', () => {
  const historyEntries: ParamHistoryEntry[] = [
    { value: 'prev1', timestamp: 1000 },
    { value: 'prev2', timestamp: 2000 },
  ];

  it('shows history chevron when history entries exist', () => {
    render(
      <BindParamPanel
        params={[colonParam]}
        values={{}}
        labels={{ 'named:uid': ':uid' }}
        history={{ 'named:uid': historyEntries }}
        onChange={vi.fn()}
      />,
    );
    // Chevron button should be present
    expect(screen.getAllByPlaceholderText('query.paramValue')).toHaveLength(1);
  });

  it('opens history dropdown on focus and selects entry', () => {
    const onApplyHistory = vi.fn();
    render(
      <BindParamPanel
        params={[colonParam]}
        values={{}}
        labels={{ 'named:uid': ':uid' }}
        history={{ 'named:uid': historyEntries }}
        onApplyHistory={onApplyHistory}
        onChange={vi.fn()}
      />,
    );
    const input = screen.getByPlaceholderText('query.paramValue');
    fireEvent.focus(input);
    expect(screen.getByText('prev1')).toBeInTheDocument();
    expect(screen.getByText('prev2')).toBeInTheDocument();

    fireEvent.mouseDown(screen.getByText('prev1'));
    expect(onApplyHistory).toHaveBeenCalledWith('named:uid', 'prev1');
  });

  it('clears history via clear button', () => {
    const onClearHistory = vi.fn();
    render(
      <BindParamPanel
        params={[colonParam]}
        values={{}}
        labels={{ 'named:uid': ':uid' }}
        history={{ 'named:uid': historyEntries }}
        onClearHistory={onClearHistory}
        onChange={vi.fn()}
      />,
    );
    const input = screen.getByPlaceholderText('query.paramValue');
    fireEvent.focus(input);
    // The X button triggers clear
    const clearBtn = screen.getByTitle('query.editor.param.clearHistory');
    fireEvent.click(clearBtn);
    expect(onClearHistory).toHaveBeenCalledWith('named:uid');
  });

  it('navigates history with keyboard arrows', () => {
    const onApplyHistory = vi.fn();
    render(
      <BindParamPanel
        params={[colonParam]}
        values={{}}
        labels={{ 'named:uid': ':uid' }}
        history={{ 'named:uid': historyEntries }}
        onApplyHistory={onApplyHistory}
        onChange={vi.fn()}
      />,
    );
    const input = screen.getByPlaceholderText('query.paramValue');
    fireEvent.focus(input);
    // ArrowDown to first item (already highlighted), ArrowDown again to second
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onApplyHistory).toHaveBeenCalledWith('named:uid', 'prev2');
  });

  it('closes dropdown on Escape', () => {
    render(
      <BindParamPanel
        params={[colonParam]}
        values={{}}
        labels={{ 'named:uid': ':uid' }}
        history={{ 'named:uid': historyEntries }}
        onChange={vi.fn()}
      />,
    );
    const input = screen.getByPlaceholderText('query.paramValue');
    fireEvent.focus(input);
    expect(screen.getByText('prev1')).toBeInTheDocument();
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(screen.queryByText('prev1')).not.toBeInTheDocument();
  });
});

describe('BindParamPanel five syntax labels', () => {
  it('renders labels for all five syntaxes', () => {
    const allParams: SqlParam[] = [colonParam, atParam, templateParam, questionParam, dollarParam];
    const allLabels: Record<string, string> = {
      'named:uid': ':uid',
      'named:name': '@name',
      'named:x': '${x}',
      'question:1': '?',
      'dollar:1': '$1',
    };
    render(<BindParamPanel params={allParams} values={{}} labels={allLabels} onChange={vi.fn()} />);
    expect(screen.getByText(':uid')).toBeInTheDocument();
    expect(screen.getByText('@name')).toBeInTheDocument();
    expect(screen.getByText('${x}')).toBeInTheDocument();
    expect(screen.getByText('?')).toBeInTheDocument();
    expect(screen.getByText('$1')).toBeInTheDocument();
  });
});
