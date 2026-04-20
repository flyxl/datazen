import path from 'node:path';
import url from 'node:url';

const __dirname = path.dirname(url.fileURLToPath(import.meta.url));

function getAppBinaryPath(): string {
  const target = path.resolve(__dirname, '../src-tauri/target');
  if (process.platform === 'win32') {
    return path.join(target, 'debug/datazen.exe');
  }
  return path.join(target, 'debug/datazen');
}

export const config: WebdriverIO.Config = {
  runner: 'local',
  specs: ['./specs/**/*.ts'],
  maxInstances: 1,
  capabilities: [
    {
      'tauri:options': {
        application: getAppBinaryPath(),
      },
    } as WebdriverIO.Capabilities,
  ],
  hostname: '127.0.0.1',
  port: 4445,
  path: '/',
  logLevel: 'info',
  waitforTimeout: 10000,
  connectionRetryTimeout: 30000,
  connectionRetryCount: 3,
  framework: 'mocha',
  reporters: ['spec'],
  mochaOpts: {
    ui: 'bdd',
    timeout: 60000,
  },
};
