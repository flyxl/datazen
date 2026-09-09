import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { SnippetEditDialog } from '../SnippetEditDialog';

vi.mock('../../../hooks/useI18n', () => {
  const t = (key: string) => key;
  return {
    useI18n: () => ({ t }),
  };
});

describe('SnippetEditDialog', () => {
  afterEach(() => {
    cleanup();
  });

  it('renders with initial values when editing existing snippet', () => {
    render(
      <SnippetEditDialog
        open={true}
        snippet={{
          id: 's-1',
          prefix: 'selw',
          descriptionKey: 'Select with WHERE',
          template: 'SELECT * FROM ${1:tbl};${2}',
        }}
        onClose={vi.fn()}
        onSave={vi.fn()}
      />,
    );

    expect(screen.getByDisplayValue('selw')).toBeInTheDocument();
    expect(screen.getByDisplayValue('Select with WHERE')).toBeInTheDocument();
    expect(screen.getByDisplayValue('SELECT * FROM ${1:tbl};${2}')).toBeInTheDocument();
  });

  it('validates prefix format and prevents saving invalid prefix', () => {
    const onSave = vi.fn();
    render(<SnippetEditDialog open={true} snippet={null} onClose={vi.fn()} onSave={onSave} />);

    const prefixInput = screen.getByPlaceholderText('query.snippets.prefixPlaceholder');
    fireEvent.change(prefixInput, { target: { value: '123-invalid' } });

    const saveButton = screen.getByRole('button', { name: 'common.save' });
    fireEvent.click(saveButton);

    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByText('query.snippets.prefixInvalid')).toBeInTheDocument();
  });

  it('shows duplicate prefix error when existingPrefixes contains the entered prefix', () => {
    const onSave = vi.fn();
    render(
      <SnippetEditDialog
        open={true}
        snippet={null}
        existingPrefixes={['selw', 'selcustom']}
        onClose={vi.fn()}
        onSave={onSave}
      />,
    );

    fireEvent.change(screen.getByPlaceholderText('query.snippets.prefixPlaceholder'), {
      target: { value: 'selw' },
    });
    fireEvent.change(screen.getByPlaceholderText('query.snippets.templatePlaceholder'), {
      target: { value: 'SELECT 1;${1}' },
    });

    fireEvent.click(screen.getByRole('button', { name: 'common.save' }));

    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByText('query.snippets.prefixDuplicate')).toBeInTheDocument();
  });

  it('shows template-required error when template is empty', () => {
    const onSave = vi.fn();
    render(<SnippetEditDialog open={true} snippet={null} onClose={vi.fn()} onSave={onSave} />);

    fireEvent.change(screen.getByPlaceholderText('query.snippets.prefixPlaceholder'), {
      target: { value: 'my_prefix' },
    });
    fireEvent.change(screen.getByPlaceholderText('query.snippets.templatePlaceholder'), {
      target: { value: '   ' },
    });

    fireEvent.click(screen.getByRole('button', { name: 'common.save' }));

    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByText('query.snippets.templateRequired')).toBeInTheDocument();
  });

  it('calls onSave with valid data and generated UUID for new snippets', () => {
    const onSave = vi.fn();
    render(<SnippetEditDialog open={true} snippet={null} onClose={vi.fn()} onSave={onSave} />);

    fireEvent.change(screen.getByPlaceholderText('query.snippets.prefixPlaceholder'), {
      target: { value: 'my_prefix' },
    });
    fireEvent.change(screen.getByPlaceholderText('query.snippets.descriptionPlaceholder'), {
      target: { value: 'My custom snippet' },
    });
    fireEvent.change(screen.getByPlaceholderText('query.snippets.templatePlaceholder'), {
      target: { value: 'SELECT 1;${1}' },
    });

    fireEvent.click(screen.getByRole('button', { name: 'common.save' }));

    expect(onSave).toHaveBeenCalledWith({
      id: expect.any(String),
      prefix: 'my_prefix',
      descriptionKey: 'My custom snippet',
      template: 'SELECT 1;${1}',
    });
  });
});
