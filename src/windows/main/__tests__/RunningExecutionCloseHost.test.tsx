import { afterEach, describe, expect, it } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { RunningExecutionCloseHost } from '../RunningExecutionCloseHost';
import {
  askRunningExecutionGate,
  setRunningExecutionGate,
  type RunningExecutionRequest,
} from '../../../stores/runningExecutionClose';

/**
 * The store asks a gate before it takes away a tab whose query is still running.
 * This is the other half of that contract: the gate must actually reach a human
 * and hand back their answer, and it must name the tab and show the SQL so the
 * question is answerable.
 */

afterEach(() => {
  cleanup();
  setRunningExecutionGate(null);
});

function request(overrides: Partial<RunningExecutionRequest> = {}): RunningExecutionRequest {
  return {
    panelId: 'panel-1',
    title: 'local · app · SQL',
    paneId: 'main',
    sql: 'SELECT pg_sleep(10)',
    runningPanes: 1,
    ...overrides,
  };
}

describe('RunningExecutionCloseHost', () => {
  it('shows which tab and which SQL it is asking about', async () => {
    render(<RunningExecutionCloseHost />);

    await act(async () => {
      void askRunningExecutionGate(request());
    });

    expect(screen.getByText('Close a tab with a running query?')).toBeTruthy();
    expect(screen.getByText('This tab has a running query. Close it anyway?')).toBeTruthy();
    expect(screen.getByText('local · app · SQL')).toBeTruthy();
    expect(screen.getByText('SELECT pg_sleep(10)')).toBeTruthy();
    expect(screen.getByText('Stop and close')).toBeTruthy();

    await act(async () => {
      fireEvent.click(screen.getByTestId('confirm-dialog-cancel'));
    });
  });

  it('resolves true when the user chooses to stop and close', async () => {
    render(<RunningExecutionCloseHost />);
    let answer: Promise<boolean> | null = null;

    await act(async () => {
      answer = askRunningExecutionGate(request());
    });
    await act(async () => {
      fireEvent.click(screen.getByTestId('confirm-dialog-ok'));
    });

    await expect(answer).resolves.toBe(true);
  });

  it('resolves false when the user keeps the tab', async () => {
    render(<RunningExecutionCloseHost />);
    let answer: Promise<boolean> | null = null;

    await act(async () => {
      answer = askRunningExecutionGate(request());
    });
    await act(async () => {
      fireEvent.click(screen.getByTestId('confirm-dialog-cancel'));
    });

    await expect(answer).resolves.toBe(false);
  });

  it('says how many statements are running when a tab has several panes', async () => {
    render(<RunningExecutionCloseHost />);

    await act(async () => {
      void askRunningExecutionGate(request({ runningPanes: 3 }));
    });
    expect(screen.getByText('This tab has 3 running queries. Close it anyway?')).toBeTruthy();

    await act(async () => {
      fireEvent.click(screen.getByTestId('confirm-dialog-cancel'));
    });
  });
});
