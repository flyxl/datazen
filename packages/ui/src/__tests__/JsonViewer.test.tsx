import { describe, expect, it, vi, afterEach } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { JsonViewer } from '../index';

afterEach(cleanup);

describe('@datazen/ui JsonViewer (DB-13)', () => {
  it('renders the parsed document as a collapsible tree by default', () => {
    render(<JsonViewer text={'{"user":{"name":"ada","tags":["admin","dev"]},"count":2}'} />);

    const tree = screen.getByTestId('json-viewer-tree');
    // The tree names the branches and the leaves; a text dump would not.
    expect(tree).toHaveTextContent('user');
    expect(tree).toHaveTextContent('ada');
    expect(tree).toHaveTextContent('2 items');
  });

  it('keeps a branch collapsed until it is opened', () => {
    render(<JsonViewer text={'{"a":{"deep":{"deeper":1}}}'} />);

    // `a` opens by default (depth < 2), `deep` does not — neither its value
    // nor its child keys show while collapsed; only the size does.
    const tree = screen.getByTestId('json-viewer-tree');
    expect(tree).toHaveTextContent('deep');
    expect(tree).toHaveTextContent('1 key');
    expect(tree).not.toHaveTextContent('deeper');

    // The toggle next to `deep` opens it.
    const toggles = screen.getAllByRole('button', { name: 'jsonViewer.expand' });
    fireEvent.click(toggles[0]!);
    expect(screen.getByTestId('json-viewer-tree')).toHaveTextContent('deeper');
  });

  it('switches to the text modes: raw, pretty and minify', () => {
    render(<JsonViewer text={'{"b":1,"a":2}'} />);

    // Raw is the text exactly as it came — no re-format, no re-order.
    fireEvent.click(screen.getByTestId('json-viewer-mode-raw'));
    expect(screen.getByTestId('json-viewer-text').textContent).toBe('{"b":1,"a":2}');

    // Pretty indents; minify strips — both re-serialise the parsed document.
    fireEvent.click(screen.getByTestId('json-viewer-mode-pretty'));
    expect(screen.getByTestId('json-viewer-text').textContent).toBe(
      JSON.stringify({ b: 1, a: 2 }, null, 2),
    );
    fireEvent.click(screen.getByTestId('json-viewer-mode-minify'));
    expect(screen.getByTestId('json-viewer-text').textContent).toBe('{"b":1,"a":2}');
  });

  it('shows only the raw text when the document does not parse', () => {
    render(<JsonViewer text={'{"broken":'} />);

    // No mode can promise a parse; raw shows exactly what is wrong.
    expect(screen.queryByTestId('json-viewer-mode-tree')).toBeNull();
    expect(screen.getByTestId('json-viewer-text')).toHaveTextContent('{"broken":');
    expect(screen.getByTestId('json-viewer-parse-error')).toBeInTheDocument();
  });

  it('parses once per text change, not per mode change', () => {
    const spy = vi.spyOn(JSON, 'parse');
    render(<JsonViewer text={'{"x":1}'} />);
    fireEvent.click(screen.getByTestId('json-viewer-mode-pretty'));
    fireEvent.click(screen.getByTestId('json-viewer-mode-minify'));
    fireEvent.click(screen.getByTestId('json-viewer-mode-raw'));
    // The four renders of one document cost one parse.
    expect(spy.mock.calls.length).toBeLessThanOrEqual(2);
    spy.mockRestore();
  });
});
