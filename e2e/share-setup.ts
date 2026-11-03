// Builds the demo Share Sites that share.spec.ts opens, from the dist/share that `pnpm build` made.
import { execFileSync } from 'node:child_process';

export default function setup() {
  for (const args of [['dist/share', 'dist/demo-site'], ['dist/share', 'dist/demo-site-hostile', '--hostile']]) {
    execFileSync('cargo', ['run', '-q', '-p', 'rostery-core', '--example', 'demo_site', '--', ...args], { stdio: 'inherit' });
  }
}
