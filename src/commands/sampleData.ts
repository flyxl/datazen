import { invoke } from '@tauri-apps/api/core';
import type { ConnectionConfig } from '../types';

export const sampleDataCommands = {
  initSampleDatabase: () => invoke<ConnectionConfig>('init_sample_database'),
};
