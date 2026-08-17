import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup, screen, act } from '@testing-library/react';
import { useConfirmDialog } from '../useConfirmDialog';

vi.mock('../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => {
      const map: Record<string, string> = {
        'common.cancel': 'Cancel',
        'common.confirm': 'Confirm',
      };
      return map[key] ?? key;
    },
  }),
}));

afterEach(cleanup);

function TestHarness({ onResult }: { onResult: (val: boolean) => void }) {
  const [confirm, dialog] = useConfirmDialog();
  return (
    <div>
      <button
        data-testid="trigger"
        onClick={async () => {
          const result = await confirm({
            title: 'Test Title',
            message: 'Test Message',
            kind: 'warning',
          });
          onResult(result);
        }}
      >
        Open
      </button>
      {dialog}
    </div>
  );
}

describe('useConfirmDialog', () => {
  it('resolves true when confirm is clicked', async () => {
    const onResult = vi.fn();
    render(<TestHarness onResult={onResult} />);

    await act(async () => {
      fireEvent.click(screen.getByTestId('trigger'));
    });

    expect(screen.getByText('Test Title')).toBeTruthy();
    expect(screen.getByText('Test Message')).toBeTruthy();

    await act(async () => {
      fireEvent.click(screen.getByTestId('confirm-dialog-ok'));
    });

    expect(onResult).toHaveBeenCalledWith(true);
  });

  it('resolves false when cancel is clicked', async () => {
    const onResult = vi.fn();
    render(<TestHarness onResult={onResult} />);

    await act(async () => {
      fireEvent.click(screen.getByTestId('trigger'));
    });

    await act(async () => {
      fireEvent.click(screen.getByText('Cancel'));
    });

    expect(onResult).toHaveBeenCalledWith(false);
  });

  it('does not render dialog before trigger', () => {
    const onResult = vi.fn();
    render(<TestHarness onResult={onResult} />);
    expect(screen.queryByText('Test Title')).toBeNull();
  });
});
