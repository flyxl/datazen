import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { OnboardingGuideBar } from '../OnboardingGuideBar';
import { useOnboardingStore } from '../../../stores/onboardingStore';

describe('OnboardingGuideBar', () => {
  beforeEach(() => {
    localStorage.clear();
    useOnboardingStore.getState().resetOnboarding();
  });

  afterEach(() => {
    cleanup();
  });

  it('does not render when status is not active', () => {
    useOnboardingStore.getState().skipOnboarding();
    const { container } = render(<OnboardingGuideBar />);
    expect(container.firstChild).toBeNull();
  });

  it('renders step 2 message and quick-run button when step is 2', () => {
    useOnboardingStore.getState().startOnboarding('sample_sqlite');
    const onRun = vi.fn();
    render(<OnboardingGuideBar onExecuteSampleQuery={onRun} />);

    expect(screen.getByTestId('onboarding-guide-bar')).toBeInTheDocument();
    expect(screen.getByText(/2\/3/)).toBeInTheDocument();

    const runBtn = screen.getByTestId('onboarding-quick-run-btn');
    fireEvent.click(runBtn);
    expect(onRun).toHaveBeenCalled();
  });

  it('renders step 3 message when step is 3 and handles completion', () => {
    useOnboardingStore.getState().startOnboarding('sample_sqlite');
    useOnboardingStore.getState().markQueryExecuted();

    render(<OnboardingGuideBar />);
    expect(screen.getByText(/3\/3/)).toBeInTheDocument();

    const completeBtn = screen.getByTestId('onboarding-complete-btn');
    fireEvent.click(completeBtn);
    expect(useOnboardingStore.getState().status).toBe('completed');
  });

  it('skips onboarding when clicking skip button', () => {
    useOnboardingStore.getState().startOnboarding('sample_sqlite');
    render(<OnboardingGuideBar />);

    const skipBtn = screen.getByTestId('onboarding-skip-btn');
    fireEvent.click(skipBtn);
    expect(useOnboardingStore.getState().status).toBe('skipped');
  });
});
