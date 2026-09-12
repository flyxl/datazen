import { StrictMode } from 'react';
import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { OnboardingWizard } from '../OnboardingWizard';
import { WIZARD_FOOTER_HEIGHT_CLASS } from '../onboardingLayout';

/* ---------- mocks ---------- */

const invokeMock = vi.fn();
const detectMock = vi.fn();
const pickImportFileMock = vi.fn();
const importAtPathMock = vi.fn();
const saveConnectionMock = vi.fn().mockResolvedValue(undefined);
const fetchConnectionsMock = vi.fn().mockResolvedValue(undefined);
const fetchGroupsMock = vi.fn().mockResolvedValue(undefined);
const updateSettingsMock = vi.fn().mockResolvedValue(undefined);
const saveProfileMock = vi.fn().mockResolvedValue(true);
const validateConfigMock = vi.fn().mockResolvedValue(false);

const connectionStoreState = {
  connections: [] as Array<{ id: string; name: string }>,
  connectionsLoaded: true,
  error: null as string | null,
  fetchConnections: fetchConnectionsMock,
  fetchGroups: fetchGroupsMock,
  saveConnection: saveConnectionMock,
};

const aiStoreState = {
  providers: [],
  saving: false,
  savingState: false,
  validating: false,
  remoteModels: [],
  fetchingRemoteModels: false,
  configError: null,
  fetchRemoteModels: vi.fn().mockResolvedValue([]),
  clearError: vi.fn(),
  saveProfile: saveProfileMock,
  validateConfig: validateConfigMock,
  loadProviders: vi.fn().mockResolvedValue(undefined),
};

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock('@tauri-apps/api/app', () => ({
  getVersion: async () => '9.9.9',
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, string | number>) => {
      if (!params) return key;
      return Object.entries(params).reduce(
        (text, [k, v]) => text.replace(`{${k}}`, String(v)),
        key,
      );
    },
  }),
}));

vi.mock('../../../commands/connection', () => ({
  connectionCommands: {
    detectConnectionImportPath: (...args: unknown[]) => detectMock(...args),
    pickConnectionsImportFile: (...args: unknown[]) => pickImportFileMock(...args),
    importConnectionsAtPath: (...args: unknown[]) => importAtPathMock(...args),
    importConnectionsFromApp: vi.fn(),
    pickConnectionImportPathWithDialog: vi.fn(),
    getAvailableDrivers: async () => ['postgresql', 'sqlite'],
  },
}));

vi.mock('../../../stores/connectionStore', () => ({
  useConnectionStore: (sel: (s: typeof connectionStoreState) => unknown) =>
    sel(connectionStoreState),
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: { updateSettings: typeof updateSettingsMock }) => unknown) =>
    sel({ updateSettings: updateSettingsMock }),
}));

vi.mock('../../../stores/aiStore', () => ({
  // Supports both `useAiStore(selector)` and the whole-store form the shared
  // AI profile draft uses.
  useAiStore: (sel?: (s: typeof aiStoreState) => unknown) =>
    sel ? sel(aiStoreState) : aiStoreState,
}));

// The connection form is a heavy, separately tested component: stub it and keep
// the shell wiring (onSaved → step 2) under test.
vi.mock('../steps/ManualConnectionStep', () => ({
  ManualConnectionStep: ({ onSaved }: { onSaved: (name: string) => void }) => (
    <div data-testid="onboarding-step-s1-manual">
      <button type="button" data-testid="stub-save" onClick={() => onSaved('Prod DB')}>
        save
      </button>
    </div>
  ),
}));

beforeEach(() => {
  vi.clearAllMocks();
  detectMock.mockResolvedValue({ path: '', found: false });
  importAtPathMock.mockResolvedValue({
    imported: 3,
    overwritten: 0,
    groupsAdded: 1,
    sourceFormat: 'DataZen',
  });
  invokeMock.mockResolvedValue('/data/sample/playground.db');
});

afterEach(() => {
  cleanup();
});

/* ---------- helpers ---------- */

async function reachImportStep() {
  render(<OnboardingWizard />);
  fireEvent.click(screen.getByTestId('onboarding-entry-import'));
  await screen.findByTestId('onboarding-import-form');
}

async function completeFileImport() {
  pickImportFileMock.mockResolvedValue('/tmp/export.datazenconnection');
  fireEvent.click(screen.getByTestId('onboarding-import-submit'));
  await screen.findByTestId('import-selected-file');
  // Encrypted DataZen exports require the password before importing.
  fireEvent.change(screen.getByPlaceholderText('connShare.passwordImportPlaceholder'), {
    target: { value: 'secret' },
  });
  fireEvent.click(screen.getByTestId('onboarding-import-submit'));
  await screen.findByTestId('onboarding-import-success');
}

/* ---------- tests ---------- */

describe('OnboardingWizard shell', () => {
  it('renders the S0 entry choice with the step indicator hidden', async () => {
    render(<OnboardingWizard />);
    expect(screen.getByTestId('onboarding-step-s0')).toBeInTheDocument();
    expect(screen.getByTestId('onboarding-entry-import')).toBeInTheDocument();
    expect(screen.getByTestId('onboarding-entry-manual')).toBeInTheDocument();
    expect(screen.getByTestId('onboarding-entry-sample')).toBeInTheDocument();
    // No step number before step 1, and no Continue: the cards are the only way forward.
    expect(screen.queryByTestId('onboarding-continue')).not.toBeInTheDocument();
    expect(screen.queryByTestId('onboarding-back')).not.toBeInTheDocument();
    expect(await screen.findByText('v9.9.9')).toBeInTheDocument();
  });

  it('aligns the sidebar version row with the footer step row (shared height)', () => {
    render(<OnboardingWizard />);
    const sidebarFoot = screen.getByTestId('onboarding-sidebar-foot');
    const footer = screen.getByTestId('onboarding-footer');
    // Both rows are the same fixed height and both centre their content, so the
    // version and the step indicator end on the same bottom line.
    expect(sidebarFoot.className).toContain(WIZARD_FOOTER_HEIGHT_CLASS);
    expect(footer.className).toContain(WIZARD_FOOTER_HEIGHT_CLASS);
    expect(sidebarFoot.className).toContain('items-center');
    expect(footer.className).toContain('items-center');
  });

  it('shows the import form inline — never a dialog — after clicking the import card', async () => {
    await reachImportStep();
    expect(screen.getByTestId('onboarding-step-s1-import')).toBeInTheDocument();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(screen.getByTestId('onboarding-step-label').textContent).toBe('onboarding.s1.stepLabel');
    expect(screen.getByTestId('onboarding-continue')).toBeDisabled();
  });

  it('import → Continue → step 2 is the AI provider step, then Done', async () => {
    await reachImportStep();
    await completeFileImport();
    expect(await screen.findByText(/onboarding\.s1\.importSuccess/)).toBeInTheDocument();

    const continueBtn = screen.getByTestId('onboarding-continue');
    await waitFor(() => expect(continueBtn).toBeEnabled());
    fireEvent.click(continueBtn);

    expect(await screen.findByTestId('onboarding-step-s2-ai')).toBeInTheDocument();
    expect(screen.getByTestId('onboarding-step-label').textContent).toBe('onboarding.s2.stepLabel');
    // Continue is replaced by Finish on step 2.
    expect(screen.queryByTestId('onboarding-continue')).not.toBeInTheDocument();
    expect(screen.getByTestId('onboarding-finish')).toBeInTheDocument();
  });

  it('sample → Continue → step 2 is the AI provider step', async () => {
    render(<OnboardingWizard />);
    fireEvent.click(screen.getByTestId('onboarding-entry-sample'));
    await screen.findByTestId('onboarding-sample-path');
    // Seeded database + one Sample Playground connection.
    await waitFor(() => expect(saveConnectionMock).toHaveBeenCalledTimes(1));
    expect(saveConnectionMock.mock.calls[0][0]).toMatchObject({
      name: 'Sample Playground',
      databaseType: 'sqlite',
      database: '/data/sample/playground.db',
    });

    const continueBtn = screen.getByTestId('onboarding-continue');
    await waitFor(() => expect(continueBtn).toBeEnabled());
    fireEvent.click(continueBtn);
    expect(await screen.findByTestId('onboarding-step-s2-ai')).toBeInTheDocument();
  });

  it('sample step recovers from the StrictMode double mount (never stuck on "preparing")', async () => {
    // The app runs under <StrictMode>: effects mount → clean up → mount again.
    // A naïve "started once" guard leaves the first (cancelled) run's result
    // discarded and the panel stuck on "preparing the sample dataset".
    render(
      <StrictMode>
        <OnboardingWizard />
      </StrictMode>,
    );
    fireEvent.click(screen.getByTestId('onboarding-entry-sample'));
    expect(await screen.findByTestId('onboarding-sample-path')).toBeInTheDocument();
    expect(screen.getByTestId('onboarding-sample-status').textContent).toContain(
      'onboarding.s1.sampleReady',
    );
  });

  it('manual entry keeps step 1 in charge: no footer Continue, Save leads to the AI step', async () => {
    render(<OnboardingWizard />);
    fireEvent.click(screen.getByTestId('onboarding-entry-manual'));
    expect(await screen.findByTestId('onboarding-step-s1-manual')).toBeInTheDocument();
    expect(screen.queryByTestId('onboarding-continue')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('stub-save'));
    expect(await screen.findByTestId('onboarding-step-s2-ai')).toBeInTheDocument();
  });

  it('SKIP finishes the journey and marks onboarding completed', async () => {
    render(<OnboardingWizard />);
    fireEvent.click(screen.getByTestId('onboarding-skip'));
    expect(await screen.findByTestId('onboarding-step-s3')).toBeInTheDocument();
    expect(screen.getByTestId('onboarding-summary-ai').textContent).toContain(
      'onboarding.s3.aiNotConfigured',
    );

    fireEvent.click(screen.getByTestId('onboard-open-datazen'));
    await waitFor(() =>
      expect(updateSettingsMock).toHaveBeenCalledWith({
        onboarding: { completed: true, version: 1 },
      }),
    );
  });

  it('writes a real AI profile when step 2 is finished with a key', async () => {
    await reachImportStep();
    await completeFileImport();
    fireEvent.click(screen.getByTestId('onboarding-continue'));
    await screen.findByTestId('onboarding-step-s2-ai');

    // Step 2 renders the settings AI form (same fields as Settings → AI).
    expect(screen.getByTestId('onboarding-ai-form')).toBeInTheDocument();
    fireEvent.change(screen.getByPlaceholderText('settings.ai.apiKeyPlaceholder'), {
      target: { value: 'sk-test' },
    });
    fireEvent.change(screen.getByPlaceholderText('e.g. gpt-4o, claude-3-7-sonnet, qwen2.5-coder'), {
      target: { value: 'gpt-4o-mini' },
    });
    fireEvent.click(screen.getByTestId('onboarding-finish'));

    await waitFor(() => expect(saveProfileMock).toHaveBeenCalledTimes(1));
    expect(saveProfileMock.mock.calls[0][0]).toMatchObject({
      providerType: 'open_ai',
      apiKey: 'sk-test',
      model: 'gpt-4o-mini',
      isDefault: true,
    });
    // Exactly one profile is written — the journey must not double-save.
    expect(saveProfileMock).toHaveBeenCalledTimes(1);
    expect(await screen.findByTestId('onboarding-step-s3')).toBeInTheDocument();
    expect(screen.getByTestId('onboarding-summary-ai').textContent).toContain('gpt-4o-mini');
  });
});
