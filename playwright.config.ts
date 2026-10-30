import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: 'e2e',
  webServer: {
    // `pnpm test:e2e` builds first; this serves dist/app.
    command: 'pnpm exec vite preview',
    url: 'http://localhost:4173',
    reuseExistingServer: !process.env.CI,
  },
  use: { baseURL: 'http://localhost:4173' },
  // Software WebGL, so the Universe draws on a machine with no GPU.
  projects: [
    { name: 'chromium', use: { browserName: 'chromium', launchOptions: { args: ['--use-angle=swiftshader'] } } },
  ],
});
