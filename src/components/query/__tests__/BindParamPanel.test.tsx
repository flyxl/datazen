import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup, screen } from '@testing-library/react';
import { BindParamPanel } from '../BindParamPanel';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

afterEach(cleanup);

describe('BindParamPanel', () => {
  it('renders nothing without params', () => {
    const { container } = render(
      <BindParamPanel params={[]} values={{}} onChange={vi.fn()} />,
    );
    expect(container.firstChild).toBeNull();
  });

  it('shows named and positional fields and reports edits', () => {
    const onChange = vi.fn();
    render(
      <BindParamPanel
        params={[
          { name: 'uid', kind: 'named' },
          { name: '1', kind: 'positional' },
        ]}
        values={{ uid: '7' }}
        onChange={onChange}
      />,
    );
    expect(screen.getByText(':uid')).toBeInTheDocument();
    expect(screen.getByText('$1')).toBeInTheDocument();
    fireEvent.change(screen.getAllByPlaceholderText('query.paramValue')[0], {
      target: { value: '42' },
    });
    expect(onChange).toHaveBeenCalledWith('uid', '42');
  });
});
