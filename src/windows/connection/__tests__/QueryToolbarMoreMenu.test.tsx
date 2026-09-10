import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { describe, it, expect, vi, afterEach } from 'vitest';
import {
  QueryToolbarMoreMenu,
  type QueryToolbarMoreMenuProps,
} from '../query/QueryToolbarMoreMenu';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/usePlatform', () => ({
  usePlatform: () => 'macos',
}));

afterEach(cleanup);

function renderMenu(overrides: Partial<QueryToolbarMoreMenuProps> = {}) {
  const defaults = {
    onFormat: vi.fn(),
    onExplain: vi.fn(),
    onBeginTx: vi.fn(),
    onCommitTx: vi.fn(),
    onRollbackTx: vi.fn(),
    supportsExplain: true,
  };
  return render(<QueryToolbarMoreMenu {...defaults} {...overrides} />);
}

function openMenu() {
  fireEvent.click(screen.getByTestId('query-toolbar-more-menu-trigger'));
}

describe('QueryToolbarMoreMenu', () => {
  it('renders trigger button and opens menu on click', () => {
    const onFormat = vi.fn();

    renderMenu({ onFormat });

    const trigger = screen.getByTestId('query-toolbar-more-menu-trigger');
    expect(trigger).toBeInTheDocument();

    openMenu();
    expect(screen.getByTestId('more-menu-format')).toBeInTheDocument();
    expect(screen.getByTestId('more-menu-explain')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('more-menu-format'));
    expect(onFormat).toHaveBeenCalled();
    expect(screen.queryByTestId('query-toolbar-more-menu-dropdown')).not.toBeInTheDocument();
  });

  it('calls onExplain when Explain is clicked', () => {
    const onExplain = vi.fn();
    renderMenu({ onExplain, supportsExplain: true });

    openMenu();
    fireEvent.click(screen.getByTestId('more-menu-explain'));
    expect(onExplain).toHaveBeenCalled();
  });

  it('hides Explain when supportsExplain is false', () => {
    renderMenu({ supportsExplain: false });

    openMenu();
    expect(screen.queryByTestId('more-menu-explain')).not.toBeInTheDocument();
  });

  it('disables Format when formatDisabled is true', () => {
    const onFormat = vi.fn();
    renderMenu({ onFormat, formatDisabled: true });

    openMenu();
    const formatItem = screen.getByTestId('more-menu-format');
    expect(formatItem).toBeDisabled();
    fireEvent.click(formatItem);
    expect(onFormat).not.toHaveBeenCalled();
  });

  it('shows Begin transaction when not in transaction', () => {
    const onBeginTx = vi.fn();
    renderMenu({ inTransaction: false, onBeginTx });

    openMenu();
    expect(screen.getByTestId('more-menu-begin-tx')).toBeInTheDocument();
    expect(screen.queryByTestId('more-menu-commit-tx')).not.toBeInTheDocument();
    expect(screen.queryByTestId('more-menu-rollback-tx')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('more-menu-begin-tx'));
    expect(onBeginTx).toHaveBeenCalled();
  });

  it('shows Commit and Rollback when in transaction, hides Begin', () => {
    const onCommitTx = vi.fn();
    renderMenu({ inTransaction: true, onCommitTx });

    openMenu();
    expect(screen.queryByTestId('more-menu-begin-tx')).not.toBeInTheDocument();
    expect(screen.getByTestId('more-menu-commit-tx')).toBeInTheDocument();
    expect(screen.getByTestId('more-menu-rollback-tx')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('more-menu-commit-tx'));
    expect(onCommitTx).toHaveBeenCalled();
  });

  it('calls onRollbackTx when Rollback is clicked in transaction', () => {
    const onRollbackTx = vi.fn();
    renderMenu({ inTransaction: true, onRollbackTx });

    openMenu();
    fireEvent.click(screen.getByTestId('more-menu-rollback-tx'));
    expect(onRollbackTx).toHaveBeenCalled();
  });

  it('shows Refresh Completions when onRefreshCompletion is provided', () => {
    const onRefreshCompletion = vi.fn();
    renderMenu({ onRefreshCompletion });

    openMenu();
    fireEvent.click(screen.getByTestId('more-menu-refresh-completion'));
    expect(onRefreshCompletion).toHaveBeenCalled();
  });

  it('closes menu on Escape key', () => {
    renderMenu();

    openMenu();
    expect(screen.getByTestId('query-toolbar-more-menu-dropdown')).toBeInTheDocument();

    fireEvent.keyDown(screen.getByTestId('query-toolbar-more-menu-dropdown'), {
      key: 'Escape',
    });
    expect(screen.queryByTestId('query-toolbar-more-menu-dropdown')).not.toBeInTheDocument();
  });

  it('closes menu when clicking outside', () => {
    render(
      <div>
        <div data-testid="outside-element">Outside</div>
        <QueryToolbarMoreMenu
          onFormat={vi.fn()}
          onExplain={vi.fn()}
          onBeginTx={vi.fn()}
          onCommitTx={vi.fn()}
          onRollbackTx={vi.fn()}
          supportsExplain
        />
      </div>,
    );

    openMenu();
    expect(screen.getByTestId('query-toolbar-more-menu-dropdown')).toBeInTheDocument();

    fireEvent.mouseDown(screen.getByTestId('outside-element'));
    expect(screen.queryByTestId('query-toolbar-more-menu-dropdown')).not.toBeInTheDocument();
  });
});
