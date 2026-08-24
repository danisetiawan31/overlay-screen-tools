import type { Options } from '@wdio/types';
import path from 'path';
import type { Server } from 'http';
import { createServer as createViteServer, type ViteDevServer } from 'vite';
import { createMockServer } from './e2e/mock-server';

let mockServer: Server | null = null;
let viteServer: ViteDevServer | null = null;

export const config: Options.Testrunner = {
  runner: 'local',
  autoCompileOpts: {
    autoCompile: true,
    tsNodeOpts: {
      project: './tsconfig.node.json',
      transpileOnly: true,
    },
  },
  specs: ['./e2e/**/*.spec.ts'],
  exclude: [],
  maxInstances: 1,
  capabilities: [
    {
      maxInstances: 1,
      'tauri:options': {
        application: path.resolve('./src-tauri/target/debug/poc-overlay.exe'),
        args: ['--e2e-test-mode'],
      },
    },
  ],
  logLevel: 'warn',
  bail: 0,
  waitforTimeout: 15000,
  connectionRetryTimeout: 60000,
  connectionRetryCount: 3,
  services: [['tauri', { driverProvider: 'external' }]],
  framework: 'mocha',
  reporters: ['spec'],
  mochaOpts: {
    ui: 'bdd',
    timeout: 60000,
  },
  onPrepare: async function () {
    process.env.E2E_TEST_MODE = 'true';
    process.env.GROQ_API_URL = 'http://127.0.0.1:4545/groq';
    process.env.OPENROUTER_API_URL = 'http://127.0.0.1:4545/openrouter';

    // 1. Spin up mock HTTP server
    await new Promise<void>((resolve) => {
      mockServer = createMockServer();
      mockServer.listen(4545, '127.0.0.1', () => {
        resolve();
      });
    });

    // 2. Spin up Vite dev server on port 1420
    viteServer = await createViteServer({
      configFile: path.resolve('./vite.config.ts'),
      server: { port: 1420 },
    });
    await viteServer.listen();
  },
  onComplete: async function () {
    if (mockServer) {
      await new Promise<void>((resolve) => {
        mockServer!.close(() => resolve());
      });
    }
    if (viteServer) {
      await viteServer.close();
    }
  },
};
