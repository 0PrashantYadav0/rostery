import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: 'e2e',
  // Chromium draws in software here, so a sky of many Planets on a busy machine takes its time.
  timeout: 60_000,
  expect: { timeout: 15_000 },
  webServer: {
    // `pnpm test:e2e` builds first; this serves dist/app.
    command: 'pnpm exec vite preview',
    url: 'http://localhost:4173',
    reuseExistingServer: !process.env.CI,
  },
  // Reduced motion, so a sky that turns by itself does not change what a test is looking at. A test of the turning asks for it.
  use: { baseURL: 'http://localhost:4173', contextOptions: { reducedMotion: 'reduce' } },
  // macOS and Linux both ship WebKit webviews, so that is what the app really runs in.
  projects: [
    // Software WebGL, so the Universe draws on a machine with no GPU.
    { name: 'chromium', use: { browserName: 'chromium', launchOptions: { args: ['--use-angle=swiftshader'] } } },
    { name: 'webkit', use: { browserName: 'webkit' } },
  ],
});
