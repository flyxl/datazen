import { legacyInvoke as invoke } from '@datazen/backend-client';

/**
 * First-run sample dataset IPC (Rust `commands/sample.rs`).
 * Seeds an English `demo_sales` SQLite playground used by the onboarding
 * "Explore with sample data" entry; idempotent and returns the db path.
 */
export const sampleCommands = {
  seedSampleDb: () => invoke<string>('seed_sample_db'),
};
